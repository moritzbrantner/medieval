import { expect, test } from "@playwright/test";

async function state(page) {
  return page.evaluate(() => window.__TAURI__.core.invoke("campaign_state"));
}

async function pendingBattle(page) {
  await page.goto("/");
  await page.locator("#open-campaign").click();
  await page.locator('[data-province="normandy"]').click();
  await page.getByRole("button", { name: "Issue movement order", exact: true }).click();
  await page.locator('[data-province="paris"]').click();
  await expect(page.locator("#fight-battle")).toBeEnabled();
  await expect(page.locator("#resolve-battle")).toBeEnabled();
}

test("Auto-resolve commits source-aware campaign consequences", async ({ page }) => {
  await pendingBattle(page);
  await page.locator("#battle-seed").fill("42");
  await page.locator("#resolve-battle").click();
  await expect(page.locator("#pending-battle")).toBeHidden();
  await expect(page.locator("#battle-report")).toContainText("Auto-resolve · seed 42");
  const campaign = await state(page);
  const result = campaign.tacticalBattleReports[0].result;
  expect(result.autoResolveSeed).toBe(42);
  expect(result.seed.attackerArmyId).toBe("england-main");
  expect(result.armies.find((army) => army.sourceArmyId === "england-main").units.reduce((sum, unit) => sum + unit.initialSoldiers, 0)).toBe(260);
  expect(campaign.pendingBattle).toBeNull();
  await page.screenshot({ path: test.info().outputPath("auto-resolve.png") });
});

test("Fight deploys the pending campaign army and commits its played withdrawal", async ({ page }) => {
  await pendingBattle(page);
  await page.locator("#fight-battle").click();
  const battle = page.frameLocator("#campaign-battle-frame");
  await expect(battle.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await expect(battle.locator("#army-setup")).toBeHidden();
  await expect(battle.locator("#battle-location")).toBeDisabled();
  await expect(battle.locator(".unit-row").first()).toContainText("england-main");
  await battle.getByRole("button", { name: "Withdraw", exact: true }).click();
  await expect(page.locator("#campaign-battle-dialog")).not.toBeVisible({ timeout: 15_000 });
  await expect(page.locator("#pending-battle")).toBeHidden();
  await expect(page.locator("#battle-report")).toContainText("Fought on the battlefield.");
  const campaign = await state(page);
  const result = campaign.tacticalBattleReports[0].result;
  expect(result.reason).toBe("withdrawal");
  expect(result.winner).toBe("defender");
  expect(result.autoResolveSeed).toBeUndefined();
  const attacker = result.armies.find((army) => army.sourceArmyId === "england-main");
  expect(attacker.units.reduce((sum, unit) => sum + unit.initialSoldiers, 0)).toBe(260);
  for (const unit of attacker.units) {
    expect(unit.survivingSoldiers).toBe(unit.initialSoldiers);
    expect(unit.escapedSoldiers).toBe(unit.survivingSoldiers);
  }
  expect(campaign.provinces.find((province) => province.id === "paris").owner).toBe("france");
  expect(campaign.armies.find((army) => army.id === "england-main").province).toBe("normandy");
  await page.screenshot({ path: test.info().outputPath("played-withdrawal.png") });
});

test("leaving an unfinished Fight preserves the pre-battle campaign exactly", async ({ page }) => {
  await pendingBattle(page);
  const before = await state(page);
  await page.locator("#fight-battle").click();
  await expect(page.frameLocator("#campaign-battle-frame").locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await page.locator("#exit-campaign-battle").click();
  await expect(page.locator("#campaign-battle-dialog")).not.toBeVisible();
  expect(await state(page)).toEqual(before);
  await expect(page.locator("#resolve-battle")).toBeEnabled();
});

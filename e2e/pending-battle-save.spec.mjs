import { expect, test } from "@playwright/test";

async function state(page) {
  return page.evaluate(() => window.__TAURI__.core.invoke("campaign_state"));
}

async function begin(page) {
  await page.goto("/");
  await page.locator("#open-campaign").click();
  await page.locator('[data-province="normandy"]').click();
  await page.getByRole("button", { name: "Issue movement order", exact: true }).click();
}

for (const mode of ["Fight", "Auto-resolve"]) {
  test(`pending boundary autosave survives exit and resumes ${mode}`, async ({ page }) => {
    await begin(page);
    await page.locator('[data-province="paris"]').click();
    await expect(page.locator("#pending-battle")).toBeVisible();
    const before = await state(page);
    const saved = await page.evaluate(() => JSON.parse(localStorage.getItem("medieval-campaign-save-v1")));
    expect(saved.schemaVersion).toBe(2);
    expect(saved.campaign).toEqual(before);
    await page.reload();
    await page.locator("#open-campaign").click();
    await page.locator("#load-campaign").click();
    await expect(page.locator("#pending-battle")).toBeVisible();
    expect(await state(page)).toEqual(before);
    if (mode === "Fight") {
      await page.locator("#fight-battle").click();
      const battle = page.frameLocator("#campaign-battle-frame");
      await expect(battle.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
      await battle.getByRole("button", { name: "Withdraw", exact: true }).click();
    } else {
      await page.locator("#resolve-battle").click();
    }
    await expect(page.locator("#pending-battle")).toBeHidden({ timeout: 15_000 });
    const completed = await state(page);
    await page.reload();
    await page.locator("#open-campaign").click();
    await page.locator("#load-campaign").click();
    await expect(page.locator("#battle-report")).toBeVisible();
    expect(await state(page)).toEqual(completed);
  });
}

test("failed boundary storage leaves hostile movement uncommitted", async ({ page }) => {
  await begin(page);
  const before = await state(page);
  await page.evaluate(() => {
    Storage.prototype.setItem = () => { throw new Error("test storage unavailable"); };
  });
  await page.locator('[data-province="paris"]').click();
  await expect(page.locator("#error")).toContainText("test storage unavailable");
  expect(await state(page)).toEqual(before);
  await expect(page.locator("#pending-battle")).toBeHidden();
});

test("corrupt saved pending provenance fails without replacing the live campaign", async ({ page }) => {
  await begin(page);
  await page.locator('[data-province="paris"]').click();
  await expect(page.locator("#pending-battle")).toBeVisible();
  const before = await state(page);
  await page.evaluate(() => {
    const save = JSON.parse(localStorage.getItem("medieval-campaign-save-v1"));
    save.campaign.pendingBattle.targetProvince = "wessex";
    localStorage.setItem("medieval-campaign-save-v1", JSON.stringify(save));
  });
  await page.locator("#load-campaign").click();
  await expect(page.locator("#error")).toContainText("pending battle");
  expect(await state(page)).toEqual(before);
});

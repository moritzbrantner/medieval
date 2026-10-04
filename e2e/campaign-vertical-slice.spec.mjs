import { expect, test } from "@playwright/test";

async function campaignState(page) {
  return page.evaluate(() => window.__TAURI__.core.invoke("campaign_state"));
}

async function tacticalStatus(frame) {
  return frame.evaluate(async () => {
    const runtime = await import("./pkg/medieval_web_battle.js");
    return JSON.parse(runtime.battle_sandbox_status());
  });
}

async function clickUnit(page, frame, unitId, shift = false, button = "left") {
  const point = await frame.evaluate(async (id) => {
    const runtime = await import("./pkg/medieval_web_battle.js");
    const rect = document.querySelector("#battle-canvas").getBoundingClientRect();
    return JSON.parse(runtime.battle_sandbox_unit_viewport(id, rect.width, rect.height));
  }, unitId);
  const rect = await page.frameLocator("#campaign-battle-frame").locator("#battle-canvas").boundingBox();
  if (!rect) throw new Error("campaign canvas has no bounds");
  if (shift) await page.keyboard.down("Shift");
  await page.mouse.click(rect.x + point.xPx, rect.y + point.yPx, { button });
  if (shift) await page.keyboard.up("Shift");
}

test("recruit, physically fight, capture and reload the reconciled campaign", async ({ page }) => {
  test.setTimeout(120_000);
  await page.goto("/");
  await page.locator("#open-campaign").click();
  await page.locator("#new-campaign").click();
  // A validated starting variant keeps the opponent away from Normandy during
  // recruitment and uses a field battle, independent of future siege breaches.
  // Only starting campaign data changes; battle state and outcomes remain real.
  await page.evaluate(async () => {
    await window.__TAURI__.core.invoke("save_campaign");
    const save = JSON.parse(localStorage.getItem("medieval-campaign-save-v1"));
    const defender = save.campaign.armies.find((army) => army.id === "france-main");
    Object.assign(defender, { province: "flanders", levy: 60, spearmen: 10, archers: 10, knights: 0 });
    const paris = save.campaign.provinces.find((province) => province.id === "paris");
    paris.battlefield = { location: "forestClearing", fortified: false };
    localStorage.setItem("medieval-campaign-save-v1", JSON.stringify(save));
  });
  await page.locator("#load-campaign").click();
  await page.locator('[data-province="normandy"]').click();
  const initial = await campaignState(page);
  await page.getByRole("button", { name: "Archers · 20 · 260 gold", exact: true }).click();
  await expect(page.locator(".recruitment-queued")).toContainText("20 Archers queued");
  await page.locator("#end-turn").click();
  await expect.poll(async () => (await campaignState(page)).turn).toBe(3);
  const recruited = await campaignState(page);
  expect(recruited.armies.find((army) => army.id === "england-main").archers).toBe(60);
  expect(recruited.recruitmentQueue.some((order) => order.factionId === "england")).toBe(false);
  expect(initial.armies.find((army) => army.id === "england-main").archers).toBe(40);
  await page.locator('[data-province="normandy"]').click();
  await page.getByRole("button", { name: "Issue movement order", exact: true }).click();
  await page.locator('[data-province="paris"]').click();
  await expect(page.locator("#fight-battle")).toBeEnabled();
  await page.locator("#fight-battle").click();
  const battle = page.frameLocator("#campaign-battle-frame");
  await expect(battle.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await battle.getByRole("button", { name: "Pause", exact: true }).click();
  const frame = page.frames().find((candidate) => candidate.url().includes("battle.html?campaign"));
  if (!frame) throw new Error("campaign frame did not open");
  const deployed = await tacticalStatus(frame);
  expect(deployed.battlefieldLocation).toBe("forestClearing");
  expect(deployed.siege).toBeNull();
  const players = deployed.units.filter((unit) => unit.side === "player");
  expect(players.find((unit) => unit.unitKind === "archers").soldiers).toBe(60);
  expect(players.every((unit) => unit.sourceArmyId === "england-main")).toBe(true);
  const enemy = deployed.units.find((unit) => unit.side === "opponent" && unit.unitKind === "archers");
  for (const [index, unit] of players.entries()) {
    await clickUnit(page, frame, unit.id, index > 0);
  }
  await expect.poll(async () => (await tacticalStatus(frame)).selectedUnits.length).toBe(players.length);
  await battle.getByRole("button", { name: "Line formation", exact: true }).click();
  await clickUnit(page, frame, enemy.id, false, "right");
  expect((await tacticalStatus(frame)).units.filter((unit) => unit.side === "player").every((unit) => unit.engagementTarget === enemy.id)).toBe(true);
  await page.screenshot({ path: test.info().outputPath("campaign-deployment-and-orders.png") });
  await battle.getByRole("button", { name: "Resume", exact: true }).click();
  let targetId = enemy.id;
  for (let round = 0; round < 3; round += 1) {
    await expect.poll(async () => {
      if (frame.isDetached() || !await page.locator("#campaign-battle-dialog").isVisible()) return true;
      let status;
      try {
        status = await tacticalStatus(frame);
      } catch (error) {
        if (frame.isDetached() || !await page.locator("#campaign-battle-dialog").isVisible()) return true;
        throw error;
      }
      const target = status.units.find((unit) => unit.id === targetId);
      return status.battleState.phase === "finished" || !target || target.routed || target.escaped || target.soldiers === 0;
    }, { timeout: 30_000 }).toBe(true).catch(async (error) => {
      await test.info().attach("stalled-tactical-status", { body: JSON.stringify(await tacticalStatus(frame), null, 2), contentType: "application/json" });
      throw error;
    });
    if (frame.isDetached() || !await page.locator("#campaign-battle-dialog").isVisible()) break;
    const status = await tacticalStatus(frame);
    if (status.battleState.phase === "finished") break;
    const next = status.units.find((unit) => unit.side === "opponent" && !unit.routed && !unit.escaped && unit.soldiers > 0);
    if (!next) break;
    await battle.getByRole("button", { name: "Pause", exact: true }).click();
    await clickUnit(page, frame, next.id, false, "right");
    targetId = next.id;
    await battle.getByRole("button", { name: "Resume", exact: true }).click();
  }
  await expect(page.locator("#pending-battle")).toBeHidden({ timeout: 5_000 });
  const completed = await campaignState(page);
  const report = completed.tacticalBattleReports.at(-1);
  expect(report.result.winner).toBe("attacker");
  expect(report.result.autoResolveSeed).toBeUndefined();
  expect(report.capturedProvince).toBe("paris");
  expect(completed.provinces.find((province) => province.id === "paris").owner).toBe("england");
  const source = report.result.armies.find((army) => army.sourceArmyId === "england-main");
  const survivor = completed.armies.find((army) => army.id === "england-main");
  expect(survivor.province).toBe("paris");
  for (const unit of source.units) {
    expect(unit.initialSoldiers).toBe(unit.survivingSoldiers + unit.casualties);
    expect(survivor[unit.kind]).toBe(unit.survivingSoldiers);
  }
  expect(report.result.armies.flatMap((army) => army.units).some((unit) => unit.casualties > 0)).toBe(true);
  await expect(page.locator("#battle-report")).toContainText("Paris captured.");
  await page.locator("#save-campaign").click();
  await page.reload();
  await page.locator("#open-campaign").click();
  await page.locator("#load-campaign").click();
  await expect(page.locator("#battle-report")).toContainText("Paris captured.");
  expect(await campaignState(page)).toEqual(completed);
  await page.locator("#battle-report").scrollIntoViewIfNeeded();
  await page.screenshot({ path: test.info().outputPath("captured-and-reloaded.png") });
});

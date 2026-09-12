import { expect, test } from "@playwright/test";

async function status(page) {
  return page.evaluate(() => window.__medievalControlsE2E.status());
}

async function canvasPoint(page, kind, ...args) {
  const point = await page.evaluate(
    ({ kind, args }) => window.__medievalControlsE2E[kind](...args),
    { kind, args },
  );
  const box = await page.locator("#battle-canvas").boundingBox();
  if (!box) throw new Error("battle canvas has no browser bounding box");
  return { x: box.x + point.xPx, y: box.y + point.yPx };
}

async function clickUnit(page, unitId, options = {}) {
  const point = await canvasPoint(page, "unitViewport", unitId);
  if (options.shift) await page.keyboard.down("Shift");
  await page.mouse.click(point.x, point.y, { button: options.button ?? "left" });
  if (options.shift) await page.keyboard.up("Shift");
}

test("physical tactical controls reach Rust-owned battle state", async ({ page }) => {
  await page.goto("/battle.html?e2e-controls=1");
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await expect(page.locator("#battle-error")).toBeHidden();

  let current = await status(page);
  expect(current.deploymentZones).toEqual([
    { side: "attacker", minXMm: 0, maxXMm: 33_333, minYMm: 0, maxYMm: 100_000 },
    { side: "defender", minXMm: 66_667, maxXMm: 100_000, minYMm: 0, maxYMm: 100_000 },
  ]);
  expect(current.forestCells).toEqual([
    { cellX: 2, cellZ: 1 },
    { cellX: 3, cellZ: 1 },
    { cellX: 3, cellZ: 2 },
    { cellX: 4, cellZ: 5 },
    { cellX: 4, cellZ: 6 },
    { cellX: 5, cellZ: 6 },
  ]);

  await clickUnit(page, "attacker-spears");
  await expect(page.locator("#battle-selection")).toContainText("Spears");
  expect((await status(page)).selectedUnits).toEqual(["attacker-spears"]);

  await clickUnit(page, "attacker-archers", { shift: true });
  expect((await status(page)).selectedUnits).toEqual([
    "attacker-archers",
    "attacker-spears",
  ]);

  await clickUnit(page, "defender-spears", { button: "right" });
  current = await status(page);
  for (const unitId of ["attacker-archers", "attacker-spears"]) {
    expect(current.units.find((unit) => unit.id === unitId)?.engagementTarget)
      .toBe("defender-spears");
  }

  await page.keyboard.press("Space");
  current = await status(page);
  for (const unitId of ["attacker-archers", "attacker-spears"]) {
    const unit = current.units.find((candidate) => candidate.id === unitId);
    expect(unit?.engagementTarget).toBeNull();
    expect(unit?.destination).toBeNull();
  }

  await clickUnit(page, "attacker-spears");
  const ground = await canvasPoint(page, "groundViewport", 35_000, 35_000);
  await page.mouse.click(ground.x, ground.y, { button: "right" });
  current = await status(page);
  const destination = current.units.find((unit) => unit.id === "attacker-spears")?.destination;
  expect(destination).not.toBeNull();
  expect(Math.abs(destination.xMm - 35_000)).toBeLessThanOrEqual(2);
  expect(Math.abs(destination.yMm - 35_000)).toBeLessThanOrEqual(2);

  const beforeCamera = structuredClone(current.camera);
  await page.keyboard.press("KeyP");
  await expect(page.locator("#battle-state")).toContainText("Paused");
  expect((await status(page)).paused).toBe(true);

  await page.keyboard.press("KeyW");
  current = await status(page);
  expect(current.camera.targetZMm).toBeLessThan(beforeCamera.targetZMm);

  await page.mouse.move(ground.x, ground.y);
  await page.mouse.wheel(0, -120);
  const zoomed = await status(page);
  expect(zoomed.camera.distanceMm).toBeLessThan(current.camera.distanceMm);

  await page.keyboard.press("Digit0");
  const fitted = await status(page);
  expect(fitted.camera.targetXMm).toBe(50_000);
  expect(fitted.camera.targetZMm).toBe(50_000);

  await page.keyboard.press("KeyP");
  expect((await status(page)).paused).toBe(false);

  await page.getByRole("button", { name: "Reset battle" }).click();
  current = await status(page);
  expect(current.selectedUnits).toEqual([]);
  const resetSpears = current.units.find((unit) => unit.id === "attacker-spears");
  expect(resetSpears).toMatchObject({ xMm: 22_000, yMm: 24_000 });
  await expect(page.locator("#battle-error")).toBeHidden();
});

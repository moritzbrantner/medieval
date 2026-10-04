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
  await page.mouse.click(
    point.x + (options.offsetX ?? 0),
    point.y + (options.offsetY ?? 0),
    { button: options.button ?? "left" },
  );
  if (options.shift) await page.keyboard.up("Shift");
}

test("battlefield selector rebuilds the Rust-owned sandbox across all three locations", async ({ page }) => {
  await page.goto("/battle.html?e2e-controls=1");
  await expect(page.locator("#army-setup")).toBeVisible();
  await page.getByRole("button", { name: "Add Levy battalion" }).click();
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  const selector = page.locator("#battle-location");

  await expect(selector).toHaveValue("mountainPass");
  expect((await status(page)).battlefieldLocation).toBe("mountainPass");

  await selector.selectOption("forestClearing");
  await expect.poll(async () => (await status(page)).battlefieldLocation).toBe("forestClearing");
  let current = await status(page);
  expect(current.riverCells).toEqual([]);
  expect(current.riverCrossingCells).toEqual([]);
  expect(current.siege).toBeNull();

  await selector.selectOption("riverFord");
  await expect.poll(async () => (await status(page)).battlefieldLocation).toBe("riverFord");
  current = await status(page);
  expect(current.riverCells).toHaveLength(8);
  expect(current.riverCrossingCells).toHaveLength(2);
  expect(current.siege).toBeNull();

  await selector.selectOption("mountainPass");
  await expect.poll(async () => (await status(page)).battlefieldLocation).toBe("mountainPass");
  current = await status(page);
  expect(current.siege).toMatchObject({ gateState: "closed" });
  await expect(page.locator("#battle-error")).toBeHidden();
});

test("physical tactical controls reach Rust-owned battle state", async ({ page }) => {
  await page.goto("/battle.html?e2e-controls=1");
  await expect(page.locator("#army-setup")).toBeVisible();
  await expect(page.locator("#army-budget")).toHaveText("1500 gold");
  await expect(page.locator("#army-spent")).toHaveText("980 gold");
  await expect(page.locator("#army-remaining")).toHaveText("520 gold");

  await page.getByRole("button", { name: "Add Levy battalion" }).click();
  await expect(page.locator("#army-spent")).toHaveText("1100 gold");
  await expect(page.locator("#army-remaining")).toHaveText("400 gold");
  await page.getByRole("button", { name: "Remove Levy battalion" }).click();
  await expect(page.locator("#army-spent")).toHaveText("980 gold");

  await expect(page.getByRole("button", { name: "Enter battle" })).toBeEnabled();
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await expect(page.locator("#battle-error")).toBeHidden();

  let current = await status(page);
  expect(current.deploymentZones).toEqual([
    { side: "attacker", minXMm: 0, maxXMm: 35_000, minYMm: 0, maxYMm: 100_000 },
    { side: "defender", minXMm: 65_000, maxXMm: 100_000, minYMm: 0, maxYMm: 100_000 },
  ]);
  expect(current.siege).toMatchObject({
    gateState: "closed",
    layout: {
      wallSegments: [
        { minXMm: 49_000, maxXMm: 51_000, minYMm: 0, maxYMm: 44_999 },
        { minXMm: 49_000, maxXMm: 51_000, minYMm: 55_001, maxYMm: 100_000 },
      ],
      gate: { minXMm: 49_000, maxXMm: 51_000, minYMm: 45_000, maxYMm: 55_000 },
      capturePoint: {
        center: { xMm: 80_000, yMm: 50_000 },
        radiusMm: 5_000,
      },
    },
    capture: { capturingSide: null, progress: 0, capturedBy: null },
  });
  expect(current.siege.layout.towers).toHaveLength(4);
  expect(current.terrainProfile).toBe("combatTerrainV4");
  expect(current.forestCells).toEqual([
    { cellX: 2, cellZ: 1 },
    { cellX: 2, cellZ: 2 },
    { cellX: 2, cellZ: 3 },
    { cellX: 4, cellZ: 5 },
    { cellX: 4, cellZ: 6 },
    { cellX: 5, cellZ: 6 },
  ]);
  expect(current.riverCells).toEqual([
    { cellX: 3, cellZ: 0 },
    { cellX: 3, cellZ: 1 },
    { cellX: 3, cellZ: 2 },
    { cellX: 3, cellZ: 3 },
    { cellX: 3, cellZ: 4 },
    { cellX: 3, cellZ: 5 },
    { cellX: 3, cellZ: 6 },
    { cellX: 3, cellZ: 7 },
  ]);
  expect(current.riverCrossingCells).toEqual([
    { cellX: 3, cellZ: 3 },
    { cellX: 3, cellZ: 4 },
  ]);
  const initialArchers = current.units.find((unit) => unit.id === "attacker-archers");
  expect(initialArchers).toMatchObject({
    formation: "line",
    formationFiles: 24,
    attackRangeMm: 25_000,
    groundCover: "open",
    rangedTargetDamageFactorMilli: 1_000,
    engagementElevationDamageFactorMilli: null,
  });
  expect(Number.isInteger(initialArchers?.terrainElevationMm)).toBe(true);
  expect(current.units.find((unit) => unit.id === "attacker-spears")?.attackRangeMm).toBe(1_500);

  // Select well away from the old 34 px anchor-only radius. This exercises the
  // rendered formation footprint that now defines the unit's browser hit target.
  await clickUnit(page, "attacker-spears", { offsetX: 40 });
  await expect(page.locator("#battle-selection")).toContainText("Spears");
  expect((await status(page)).selectedUnits).toEqual(["attacker-spears"]);

  await page.getByRole("button", { name: "Column formation" }).click();
  current = await status(page);
  expect(current.units.find((unit) => unit.id === "attacker-spears")).toMatchObject({
    formation: "column",
    formationFiles: 28,
  });
  await page.getByRole("button", { name: "Line formation" }).click();
  expect((await status(page)).units.find((unit) => unit.id === "attacker-spears")).toMatchObject({
    formation: "line",
    formationFiles: 28,
  });

  await clickUnit(page, "attacker-archers", { shift: true });
  expect((await status(page)).selectedUnits).toEqual([
    "attacker-archers",
    "attacker-spears",
  ]);

  await clickUnit(page, "defender-spears", { button: "right" });
  current = await status(page);
  for (const unitId of ["attacker-archers", "attacker-spears"]) {
    const unit = current.units.find((candidate) => candidate.id === unitId);
    expect(unit?.engagementTarget).toBe("defender-spears");
    expect(unit?.engagementElevationDamageFactorMilli).toBeGreaterThanOrEqual(900);
    expect(unit?.engagementElevationDamageFactorMilli).toBeLessThanOrEqual(1_100);
  }

  await page.keyboard.press("Space");
  current = await status(page);
  for (const unitId of ["attacker-archers", "attacker-spears"]) {
    const unit = current.units.find((candidate) => candidate.id === unitId);
    expect(unit?.engagementTarget).toBeNull();
    expect(unit?.destination).toBeNull();
    expect(unit?.engagementElevationDamageFactorMilli).toBeNull();
  }

  // Send a physical right-click order to the opposite bank. Core pathing must
  // preserve the final target while routing the unit through the explicit ford
  // and toward the still-closed authoritative siege gate.
  await clickUnit(page, "attacker-spears");
  const ground = await canvasPoint(page, "groundViewport", 70_000, 10_000);
  await page.mouse.click(ground.x, ground.y, { button: "right" });
  current = await status(page);
  const destination = current.units.find((unit) => unit.id === "attacker-spears")?.destination;
  expect(destination).not.toBeNull();
  expect(Math.abs(destination.xMm - 70_000)).toBeLessThanOrEqual(2);
  expect(Math.abs(destination.yMm - 10_000)).toBeLessThanOrEqual(2);

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

  await expect.poll(
    async () => {
      const spear = (await status(page)).units.find((unit) => unit.id === "attacker-spears");
      if (!spear) return false;
      return spear.xMm >= 37_500 && spear.xMm < 50_000
        && spear.yMm >= 37_500 && spear.yMm < 50_000;
    },
    { timeout: 10_000, intervals: [100, 200, 400] },
  ).toBe(true);

  current = await status(page);
  const crossingSpears = current.units.find((unit) => unit.id === "attacker-spears");
  expect(crossingSpears?.destination).not.toBeNull();
  expect(Math.abs(crossingSpears.destination.xMm - 70_000)).toBeLessThanOrEqual(2);
  expect(Math.abs(crossingSpears.destination.yMm - 10_000)).toBeLessThanOrEqual(2);
  expect(current.siege.gateState).toBe("closed");

  await page.getByRole("button", { name: "Reset battle" }).click();
  current = await status(page);
  expect(current.selectedUnits).toEqual([]);
  expect(current.terrainProfile).toBe("combatTerrainV4");
  expect(current.siege).toMatchObject({
    gateState: "closed",
    capture: { capturingSide: null, progress: 0, capturedBy: null },
  });
  const resetSpears = current.units.find((unit) => unit.id === "attacker-spears");
  expect(resetSpears).toMatchObject({
    xMm: 22_000,
    yMm: 10_000,
    formation: "line",
    formationFiles: 28,
    attackRangeMm: 1_500,
    groundCover: "open",
    rangedTargetDamageFactorMilli: 1_000,
  });
  await expect(page.locator("#battle-error")).toBeHidden();
});

test("unit kinds reach status and visible rows directly from core", async ({ page }) => {
  await page.goto("/battle.html?e2e-controls=1");
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  const projected = await status(page);
  expect(new Set(projected.units.map((unit) => unit.unitKind))).toEqual(new Set(["spearmen", "archers", "knights"]));
  for (const kind of ["spearmen", "archers", "knights"]) {
    await expect(page.locator(`.unit-row[data-unit-kind="${kind}"]`).first()).toBeVisible();
  }
  await page.screenshot({ path: test.info().outputPath("unit-kinds.png") });
});

test("withdrawal preserves escaped soldiers and projects the core terminal result", async ({ page }) => {
  await page.goto("/battle.html?e2e-controls=1");
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  const before = await status(page);
  await page.getByRole("button", { name: "Withdraw", exact: true }).click();
  expect((await status(page)).canWithdraw).toBe(false);
  await page.getByRole("button", { name: "Resume", exact: true }).click();
  await expect.poll(async () => (await status(page)).outcome).toBe("playerDefeat");
  const after = await status(page);
  expect(after.battleState.phase).toBe("finished");
  expect(after.battleState.reason).toBe("withdrawal");
  const escapedPlayers = after.units.filter((unit) => unit.side === "player");
  expect(escapedPlayers).toHaveLength(3);
  for (const unit of escapedPlayers) {
    expect(unit.escaped).toBe(true);
    expect(unit.soldiers).toBe(before.units.find((original) => original.id === unit.id).soldiers);
    expect(unit.pursuitCasualties).toBe(0);
  }
  await expect(page.locator("#battle-state")).toHaveText("Your force withdrew from the battlefield.");
  await expect(page.getByRole("button", { name: "Withdraw", exact: true })).toBeDisabled();
  await page.screenshot({ path: test.info().outputPath("withdrawal.png") });
});


test("formation facing follows physical movement and appears in the battlefield", async ({ page }) => {
  await page.goto("/battle.html?e2e-controls=1");
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  expect((await status(page)).units.find(unit => unit.id === "attacker-spears").facing).toEqual({ x: 1, y: 0 });
  await clickUnit(page, "attacker-spears");
  const ground = await canvasPoint(page, "groundViewport", 22_000, 30_000);
  await page.mouse.click(ground.x, ground.y, { button: "right" });
  await page.getByRole("button", { name: "Resume", exact: true }).click();
  await expect.poll(async () => (await status(page)).units.find(unit => unit.id === "attacker-spears").facing).toEqual({ x: 0, y: 1 });
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await expect(page.locator("#battle-error")).toBeHidden();
  await page.screenshot({ path: test.info().outputPath("formation-facing.png") });
});

test("physical facing and frontage orders validate the complete formation", async ({ page }) => {
  await page.goto("/battle.html?e2e-controls=1&location=forestClearing");
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await page.getByRole("button", { name: "Pause" , exact: true }).click();
  await clickUnit(page, "attacker-spears");
  await page.keyboard.press("KeyQ");
  expect((await status(page)).units.find(unit => unit.id === "attacker-spears").facing).toEqual({ x: 0, y: -1 });
  await page.locator("#frontage-metres").fill("10");
  await page.getByRole("button", { name: "Set frontage" }).click();
  expect((await status(page)).units.find(unit => unit.id === "attacker-spears").formationFiles).toBe(10);
  const before = (await status(page)).units;
  await page.locator("#frontage-metres").fill("100");
  await page.getByRole("button", { name: "Set frontage" }).click();
  await expect(page.locator("#battle-error")).toBeVisible();
  expect((await status(page)).units).toEqual(before);
  const point = await canvasPoint(page,"groundViewport",22_000,30_000);
  await page.keyboard.down("Shift");
  await page.mouse.click(point.x,point.y,{button:"right"});
  await page.keyboard.up("Shift");
  await expect(page.locator("#battle-error")).toBeHidden();
  expect((await status(page)).units.find(unit => unit.id === "attacker-spears").destination).not.toBeNull();
  await page.getByRole("button", { name: "Resume", exact: true }).click();
  await expect.poll(async () => (await status(page)).units.find(unit => unit.id === "attacker-spears").destination).toBeNull();
  expect((await status(page)).units.find(unit => unit.id === "attacker-spears").facing).toEqual({x:0,y:-1});
});

test("physical group movement, queued waypoints and attack-move reach the core", async ({ page }) => {
  await page.goto("/battle.html?e2e-controls=1&location=forestClearing");
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await clickUnit(page,"attacker-spears");
  await clickUnit(page,"attacker-archers",{shift:true});
  const initial=await status(page);
  const a=initial.units.find(unit=>unit.id==="attacker-spears");
  const b=initial.units.find(unit=>unit.id==="attacker-archers");
  const firstPoint=await canvasPoint(page,"groundViewport",60_000,50_000);
  await page.mouse.click(firstPoint.x,firstPoint.y,{button:"right"});
  const moved=await status(page);
  const movedA=moved.units.find(unit=>unit.id===a.id);
  const movedB=moved.units.find(unit=>unit.id===b.id);
  expect(movedA.destination.xMm-movedB.destination.xMm).toBe(a.xMm-b.xMm);
  expect(movedA.destination.yMm-movedB.destination.yMm).toBe(a.yMm-b.yMm);
  const secondPoint=await canvasPoint(page,"groundViewport",75_000,60_000);
  await page.keyboard.down("Control");
  await page.mouse.click(secondPoint.x,secondPoint.y,{button:"right"});
  await page.keyboard.up("Control");
  let current=await status(page);
  for(const id of [a.id,b.id]) {
    const unit=current.units.find(unit=>unit.id===id);
    expect(unit.queuedMovements).toHaveLength(1);
    expect(unit.destination).toEqual(moved.units.find(unit=>unit.id===id).destination);
  }
  await page.keyboard.press("KeyF");
  await expect(page.locator("#attack-move")).toHaveAttribute("aria-pressed","true");
  const attackPoint=await canvasPoint(page,"groundViewport",65_000,50_000);
  await page.mouse.click(attackPoint.x,attackPoint.y,{button:"right"});
  current=await status(page);
  expect(current.attackMoveArmed).toBe(false);
  for(const id of [a.id,b.id]) {
    const unit=current.units.find(unit=>unit.id===id);
    expect(unit.movementMode).toBe("attackMove");
    expect(unit.queuedMovements).toEqual([]);
  }
  await page.keyboard.press("Space");
  current=await status(page);
  for(const id of [a.id,b.id]) {
    const unit=current.units.find(unit=>unit.id===id);
    expect(unit.destination).toBeNull();
    expect(unit.movementMode).toBe("march");
    expect(unit.queuedMovements).toEqual([]);
  }
  await expect(page.locator("#battle-error")).toBeHidden();
});

// Acceptance journeys for issue #94: tactical deployment phase (pre-battle
// positioning, facing, formation and frontage, then Start battle), aligned
// with the owner decision of 2026-10-11 (option A).
//
// Contract named by these journeys (roles and Rust status, not styling hooks):
//
// * Phase. A supported single-player battle (the sandbox at every battlefield
//   location and for every legal muster, and a campaign "Fight" battle) opens
//   in a deployment phase. The Rust status reports it as
//   `status.battleState.phase === "deployment"` (the core
//   `TacticalBattleState::Deployment` variant), then `"running"` after the
//   start. While deploying, `#battle-state` mentions deployment.
// * Footprints. Every status unit carries `footprint`, the core
//   `FormationFootprint` (`minXMm`, `maxXMm`, `minYMm`, `maxYMm`). These
//   journeys read it only to check what Rust decided; the page never computes
//   it (see web-tests/tactical-deployment-contract.test.mjs).
// * Placement rule (owner decision, option A). A formation is legal only if
//   its whole footprint lies inside its own side's deployment zone and it does
//   not overlap another formation of the same side. Every layout a battle opens
//   with obeys the rule for both sides, so the old sandbox spear layout, which
//   overhung the zone edge, must change.
// * Deployment UI. A region named "Deployment" holds a button named
//   "Start battle". Both are gone (hidden, or the button disabled) once the
//   battle has started or the player has withdrawn.
// * Frozen clock. No tick runs before the start, however many frames pass.
//   The opponent does not move either.
// * Placement. With an own formation selected, a right click on legal ground
//   repositions that formation there at once: its position changes, no
//   movement order (`destination`) is left behind, and the tick stays 0. A
//   right click whose resulting footprint would leave the player's zone (the
//   neutral centre, the enemy zone, or an anchor inside the zone with the
//   footprint hanging over its edge) changes nothing and reports the zone in
//   the page alert (`#battle-error`). The same placement through the shared
//   Rust control `{ kind: "deploySelected", xMm, yMm }` is refused by Rust with
//   an error naming the zone or the overlap. Enemy formations can never be
//   selected or repositioned.
// * Facing / formation / frontage. Q/E, the Line and Column formation buttons
//   and Set frontage work during deployment through the Rust order APIs and
//   change the formation in place, with no tick. An edit whose resulting
//   footprint would leave the zone or overlap a friendly formation is rejected
//   atomically and reported in the alert.
// * Start battle. Starting locks deployment and begins simulation once: the
//   deployed state is the tick-0 state, ticks then run at the normal 1×
//   schedule, a second Start (even two clicks in the same task) changes
//   nothing, and right clicks become ordinary move orders again.
// * Withdraw is allowed during deployment and ends the battle as the player's
//   withdrawal without losses.
// * Enemy placement is the existing deterministic layout: the same battle
//   always places the same opponent formations, legally inside the opponent's
//   zone, and player deployment never changes them.
// * Campaign fog (#94 body, #132 B). During a campaign battle's deployment the
//   page does not show exact opponent soldier counts.
import { expect, test } from "@playwright/test";

const SANDBOX_LOCATIONS = ["mountainPass", "forestClearing", "riverFord"];

// Edge margin and friendly clearance for physical right clicks, generous
// enough for pointer-to-ground projection and unit picking.
const CLICK_MARGIN_MM = 2_500;
const CLICK_CLEARANCE_MM = 6_000;

async function authoritativeStatus(page) {
  return page.evaluate(async () => {
    const { battle_sandbox_status } = await import("./pkg/medieval_web_battle.js");
    return JSON.parse(battle_sandbox_status());
  });
}

// Sends one shared Rust tactical control; returns the error text or null.
async function rustControl(page, request) {
  return page.evaluate(async (request) => {
    const { battle_sandbox_control } = await import("./pkg/medieval_web_battle.js");
    try {
      battle_sandbox_control(JSON.stringify(request));
      return null;
    } catch (error) {
      return String(error?.message ?? error);
    }
  }, request);
}

async function selectOnly(page, unitId) {
  expect(await rustControl(page, { kind: "selectReplace", unitIds: [unitId] })).toBeNull();
}

function inDeployment(status) {
  return status.battleState?.phase === "deployment";
}

function unitById(status, unitId) {
  const unit = status.units.find((candidate) => candidate.id === unitId);
  if (!unit) throw new Error(`status has no unit ${unitId}`);
  return unit;
}

function zoneFor(status, side) {
  const zone = status.deploymentZones.find((candidate) => candidate.side === side);
  if (!zone) throw new Error(`status has no ${side} deployment zone`);
  return zone;
}

function zoneOfUnit(status, unit) {
  return zoneFor(status, unit.side === "player" ? status.playerSide : status.opponentSide);
}

function footprintOf(unit) {
  const footprint = unit.footprint;
  for (const key of ["minXMm", "maxXMm", "minYMm", "maxYMm"]) {
    if (!Number.isFinite(footprint?.[key])) {
      throw new Error(`${unit.id} status has no Rust footprint.${key}`);
    }
  }
  return footprint;
}

// Footprint extents around the anchor, from the Rust-reported footprint.
function extentsOf(unit) {
  const footprint = footprintOf(unit);
  return {
    left: unit.xMm - footprint.minXMm,
    right: footprint.maxXMm - unit.xMm,
    below: unit.yMm - footprint.minYMm,
    above: footprint.maxYMm - unit.yMm,
  };
}

function shifted(unit, xMm, yMm) {
  const { left, right, below, above } = extentsOf(unit);
  return { minXMm: xMm - left, maxXMm: xMm + right, minYMm: yMm - below, maxYMm: yMm + above };
}

function insideZone(zone, footprint, margin = 0) {
  return footprint.minXMm >= zone.minXMm + margin && footprint.maxXMm <= zone.maxXMm - margin
    && footprint.minYMm >= zone.minYMm + margin && footprint.maxYMm <= zone.maxYMm - margin;
}

// Mirrors the core `FormationFootprint::overlaps` (touching edges overlap).
function overlaps(a, b, clearance = 0) {
  return !(a.maxXMm + 1 + clearance <= b.minXMm || b.maxXMm + 1 + clearance <= a.minXMm
    || a.maxYMm + 1 + clearance <= b.minYMm || b.maxYMm + 1 + clearance <= a.minYMm);
}

function friendsOf(status, unit) {
  return status.units.filter((other) => other.id !== unit.id && other.side === unit.side);
}

// The owner's rule, checked against what Rust reports for every formation.
function expectLegalLayout(status, label) {
  for (const unit of status.units) {
    const footprint = footprintOf(unit);
    expect(footprint.minXMm <= unit.xMm && unit.xMm <= footprint.maxXMm
      && footprint.minYMm <= unit.yMm && unit.yMm <= footprint.maxYMm,
    `${label}: ${unit.id} footprint contains its anchor`).toBe(true);
    expect(footprint.maxXMm - footprint.minXMm, `${label}: ${unit.id} footprint has width`).toBeGreaterThan(0);
    expect(insideZone(zoneOfUnit(status, unit), footprint),
      `${label}: ${unit.id} footprint ${JSON.stringify(footprint)} lies inside its zone`).toBe(true);
    for (const other of friendsOf(status, unit)) {
      expect(overlaps(footprint, footprintOf(other)), `${label}: ${unit.id} does not overlap ${other.id}`).toBe(false);
    }
  }
}

// A point near the preferred one where the unit's whole footprint sits
// `margin` inside its zone and `clearance` away from every friend.
function legalTarget(status, unitId, preferred, { margin = CLICK_MARGIN_MM, clearance = CLICK_CLEARANCE_MM } = {}) {
  const unit = unitById(status, unitId);
  const zone = zoneOfUnit(status, unit);
  const { left, right } = extentsOf(unit);
  const xMm = Math.min(Math.max(preferred.xMm, zone.minXMm + left + margin), zone.maxXMm - right - margin);
  for (let step = 0; step <= 200; step += 1) {
    const offset = (step % 2 === 0 ? 1 : -1) * Math.ceil(step / 2) * 1_000;
    const yMm = preferred.yMm + offset;
    const footprint = shifted(unit, xMm, yMm);
    if (insideZone(zone, footprint, margin)
      && friendsOf(status, unit).every((other) => !overlaps(footprint, footprintOf(other), clearance))) {
      return { xMm, yMm };
    }
  }
  throw new Error(`no legal deployment spot for ${unitId} near ${JSON.stringify(preferred)}`);
}

// Simulation truth: what the core owns. Selection is presentation state.
function simulationTruth(current) {
  return {
    tick: current.tick,
    battleState: current.battleState,
    outcome: current.outcome,
    siege: current.siege,
    units: current.units.map(({ selected, ...unit }) => unit),
  };
}

// Where and how every formation stands; orders are deliberately excluded
// because the opponent may plan its first orders when the battle starts.
function layout(current) {
  return current.units.map(({ id, xMm, yMm, facing, formation, formationFiles, soldiers, footprint }) => (
    { id, xMm, yMm, facing, formation, formationFiles, soldiers, footprint }
  ));
}

function enemyPlacement(current) {
  return current.units
    .filter((unit) => unit.side === "opponent")
    .map(({ id, xMm, yMm, facing, formation, formationFiles, soldiers, unitKind, footprint }) => (
      { id, xMm, yMm, facing, formation, formationFiles, soldiers, unitKind, footprint }
    ));
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
  await page.mouse.click(point.x, point.y, { button: options.button ?? "left" });
}

async function rightClickGround(page, xMm, yMm) {
  const point = await canvasPoint(page, "groundViewport", xMm, yMm);
  await page.mouse.click(point.x, point.y, { button: "right" });
}

function deploymentRegion(page) {
  return page.getByRole("region", { name: "Deployment" });
}

function startBattleButton(page) {
  return page.getByRole("button", { name: "Start battle", exact: true });
}

// Replaces requestAnimationFrame with a manual clock (as in
// e2e/tactical-hud.spec.mjs): one 50 ms frame is one tick at 1×.
async function installManualFrameClock(page) {
  await page.addInitScript(() => {
    let queue = [];
    let now = 1_000;
    window.requestAnimationFrame = (callback) => {
      queue.push(callback);
      return queue.length;
    };
    window.cancelAnimationFrame = () => {};
    window.__manualFrames = {
      step(deltaMs) {
        now += deltaMs;
        const callbacks = queue;
        queue = [];
        for (const callback of callbacks) callback(now);
        return callbacks.length;
      },
    };
  });
}

async function stepFrames(page, frames, deltaMs = 50) {
  await page.evaluate(
    ({ frames, deltaMs }) => {
      for (let index = 0; index < frames; index += 1) window.__manualFrames.step(deltaMs);
    },
    { frames, deltaMs },
  );
}

const DEFAULT_MUSTER = { levy: 0, spearmen: 1, archers: 1, knights: 1 };
const MUSTER_LABELS = { levy: "Levy", spearmen: "Spearmen", archers: "Archers", knights: "Knights" };

async function setMuster(page, muster) {
  for (const kind of Object.keys(DEFAULT_MUSTER)) {
    const label = MUSTER_LABELS[kind];
    for (let count = DEFAULT_MUSTER[kind]; count > 0; count -= 1) {
      await page.getByRole("button", { name: `Remove ${label} battalion` }).click();
    }
  }
  for (const [kind, count] of Object.entries(muster)) {
    for (let index = 0; index < count; index += 1) {
      await page.getByRole("button", { name: `Add ${MUSTER_LABELS[kind]} battalion` }).click();
    }
  }
}

async function enterDeployment(page, location, muster) {
  const query = location ? `&location=${location}` : "";
  await page.goto(`/battle.html?e2e-controls=1${query}`);
  if (muster) await setMuster(page, muster);
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await expect(page.locator("#battle-error")).toBeHidden();
  await expect(deploymentRegion(page)).toBeVisible();
  await expect(startBattleButton(page)).toBeEnabled();
  const current = await authoritativeStatus(page);
  expect(inDeployment(current), "the Rust status reports the deployment phase").toBe(true);
  expect(current.tick).toBe(0);
  return current;
}

async function expectBattleStarted(page) {
  await expect(deploymentRegion(page)).toBeHidden();
  const start = startBattleButton(page);
  if (await start.count()) {
    expect(await start.isHidden() || await start.isDisabled(), "Start battle is gone after the start").toBe(true);
  }
  const current = await authoritativeStatus(page);
  expect(inDeployment(current), "the Rust status left the deployment phase").toBe(false);
  return current;
}

function expectNear(unit, xMm, yMm, tolerance = 1_000) {
  expect(Math.abs(unit.xMm - xMm), `${unit.id} x near ${xMm} (was ${unit.xMm})`).toBeLessThanOrEqual(tolerance);
  expect(Math.abs(unit.yMm - yMm), `${unit.id} y near ${yMm} (was ${unit.yMm})`).toBeLessThanOrEqual(tolerance);
}

function expectPlacedInPlace(status, unitId) {
  const unit = unitById(status, unitId);
  expect(unit.destination, `${unitId} is placed, not marching`).toBeNull();
  expect(unit.queuedMovements).toEqual([]);
  expect(status.tick).toBe(0);
  expect(inDeployment(status)).toBe(true);
}

// Places the given own formations exactly through the shared Rust control.
// Several passes resolve a target that is blocked only by a formation that
// has not been moved yet.
async function arrange(page, targets) {
  let pending = Object.entries(targets);
  for (let pass = 0; pass < 3 && pending.length; pass += 1) {
    const failed = [];
    for (const [unitId, target] of pending) {
      await selectOnly(page, unitId);
      const error = await rustControl(page, { kind: "deploySelected", xMm: target.xMm, yMm: target.yMm });
      if (error) failed.push([unitId, target, error]);
    }
    pending = failed.map(([unitId, target]) => [unitId, target]);
    if (pass === 2 && failed.length) {
      throw new Error(`could not arrange ${JSON.stringify(failed)}`);
    }
  }
  const current = await authoritativeStatus(page);
  for (const [unitId, target] of Object.entries(targets)) {
    const unit = unitById(current, unitId);
    expect([unit.xMm, unit.yMm], `${unitId} arranged exactly`).toEqual([target.xMm, target.yMm]);
  }
  expectLegalLayout(current, "arranged");
  return current;
}

test.describe("deployment phase", () => {
  test.beforeEach(async ({ page }) => {
    await installManualFrameClock(page);
  });

  for (const location of SANDBOX_LOCATIONS) {
    test(`${location} sandbox opens in a Rust deployment phase with a legal layout and no ticks`, async ({ page }) => {
      const opened = await enterDeployment(page, location);
      await expect(page.locator("#battle-state")).toContainText(/deploy/i);
      // The owner decision: every starting footprint, both sides, is legal.
      expectLegalLayout(opened, `${location} start`);

      // Two seconds of wall time at 1×: forty ticks if the clock were running.
      await stepFrames(page, 40);
      const held = await authoritativeStatus(page);
      expect(inDeployment(held)).toBe(true);
      expect(simulationTruth(held)).toEqual(simulationTruth(opened));
      await expect(page.locator("#battle-error")).toBeHidden();
    });
  }

  for (const [name, muster] of [
    ["three knights", { knights: 3 }],
    ["twelve levies", { levy: 12 }],
    ["a mixed five", { spearmen: 2, archers: 2, knights: 1 }],
  ]) {
    test(`a muster of ${name} starts with a legal layout at every location`, async ({ page }) => {
      for (const location of SANDBOX_LOCATIONS) {
        const opened = await enterDeployment(page, location, muster);
        const own = opened.units.filter((unit) => unit.side === "player");
        expect(own).toHaveLength(Object.values(muster).reduce((sum, count) => sum + count, 0));
        expectLegalLayout(opened, `${name} at ${location}`);
      }
    });
  }

  test("the player repositions own formations to legal ground with no tick and no movement order", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    const target = legalTarget(opened, "attacker-spears", { xMm: 15_000, yMm: 60_000 });

    await clickUnit(page, "attacker-spears");
    expect((await authoritativeStatus(page)).selectedUnits).toEqual(["attacker-spears"]);
    await rightClickGround(page, target.xMm, target.yMm);
    await expect(page.locator("#battle-error")).toBeHidden();

    let current = await authoritativeStatus(page);
    expectPlacedInPlace(current, "attacker-spears");
    expectNear(unitById(current, "attacker-spears"), target.xMm, target.yMm);
    expectLegalLayout(current, "after placing the spears");
    for (const unit of opened.units.filter((candidate) => candidate.id !== "attacker-spears")) {
      const { selected, ...now } = unitById(current, unit.id);
      const { selected: _, ...before } = unit;
      expect(now, `${unit.id} is untouched`).toEqual(before);
    }

    const knightsTarget = legalTarget(current, "attacker-knights", { xMm: 10_000, yMm: 85_000 });
    await clickUnit(page, "attacker-knights");
    await rightClickGround(page, knightsTarget.xMm, knightsTarget.yMm);
    await expect(page.locator("#battle-error")).toBeHidden();
    current = await authoritativeStatus(page);
    expectPlacedInPlace(current, "attacker-knights");
    expectNear(unitById(current, "attacker-knights"), knightsTarget.xMm, knightsTarget.yMm);
    expectLegalLayout(current, "after placing the knights");

    // Frames still run no ticks after placements.
    await stepFrames(page, 20);
    expect(simulationTruth(await authoritativeStatus(page))).toEqual(simulationTruth(current));
  });

  test("Rust rejects overhanging footprints and placements outside the player's zone", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    const playerZone = zoneFor(opened, opened.playerSide);
    const enemyZone = zoneFor(opened, opened.opponentSide);
    expect(playerZone.maxXMm).toBeLessThan(50_000);
    expect(enemyZone.minXMm).toBeGreaterThan(50_000);
    expect(enemyZone.minXMm).toBeLessThan(90_000);

    await clickUnit(page, "attacker-spears");
    const before = simulationTruth(await authoritativeStatus(page));

    // Neutral centre of the field.
    await rightClickGround(page, 50_000, 60_000);
    await expect(page.locator("#battle-error")).toBeVisible();
    await expect(page.locator("#battle-error")).toContainText(/zone/i);
    expect(simulationTruth(await authoritativeStatus(page)), "neutral ground is rejected").toEqual(before);

    // Open ground inside the enemy zone (not on an enemy formation).
    await rightClickGround(page, 90_000, 64_000);
    await expect(page.locator("#battle-error")).toBeVisible();
    expect(simulationTruth(await authoritativeStatus(page)), "the enemy zone is rejected").toEqual(before);

    // The anchor inside the zone, the footprint hanging over its edge.
    const spears = unitById(opened, "attacker-spears");
    const { right } = extentsOf(spears);
    expect(right, "the spears' footprint reaches well past their anchor").toBeGreaterThanOrEqual(5_000);
    const row = legalTarget(opened, "attacker-spears", { xMm: 15_000, yMm: 60_000 });
    await rightClickGround(page, playerZone.maxXMm - CLICK_MARGIN_MM, row.yMm);
    await expect(page.locator("#battle-error")).toBeVisible();
    await expect(page.locator("#battle-error")).toContainText(/zone/i);
    expect(simulationTruth(await authoritativeStatus(page)), "an overhanging footprint is rejected").toEqual(before);

    // Exactly at the boundary through the shared Rust control: one millimetre
    // over is refused, touching the edge is legal.
    const edgeX = playerZone.maxXMm - right;
    const overhang = await rustControl(page, { kind: "deploySelected", xMm: edgeX + 1, yMm: row.yMm });
    expect(overhang, "Rust refuses a footprint 1 mm over the zone edge").toMatch(/zone/i);
    expect(simulationTruth(await authoritativeStatus(page))).toEqual(before);
    expect(await rustControl(page, { kind: "deploySelected", xMm: edgeX, yMm: row.yMm })).toBeNull();
    let current = await authoritativeStatus(page);
    expect(footprintOf(unitById(current, "attacker-spears")).maxXMm).toBe(playerZone.maxXMm);
    expectPlacedInPlace(current, "attacker-spears");
    expectLegalLayout(current, "spears on the zone edge");

    // Enemy formations cannot be selected, physically or through Rust controls,
    // and right clicking one is not an engagement during deployment.
    const placed = simulationTruth(current);
    await clickUnit(page, "defender-spears", { button: "right" });
    current = await authoritativeStatus(page);
    expect(simulationTruth(current), "no engagement or move order before the start").toEqual(placed);
    expect(unitById(current, "attacker-spears").engagementTarget).toBeNull();
    await clickUnit(page, "defender-archers");
    expect((await authoritativeStatus(page)).selectedUnits).toEqual([]);
    const rejected = await rustControl(page, { kind: "selectReplace", unitIds: ["defender-spears"] });
    expect(rejected, "Rust refuses to hand an enemy formation to the player").not.toBeNull();
    current = await authoritativeStatus(page);
    expect(current.selectedUnits).toEqual([]);
    expect(enemyPlacement(current)).toEqual(enemyPlacement(opened));

    // A legal placement afterwards still works and clears the error.
    await clickUnit(page, "attacker-spears");
    const target = legalTarget(current, "attacker-spears", { xMm: 15_000, yMm: 60_000 });
    await rightClickGround(page, target.xMm, target.yMm);
    await expect(page.locator("#battle-error")).toBeHidden();
    current = await authoritativeStatus(page);
    expectNear(unitById(current, "attacker-spears"), target.xMm, target.yMm);
    expectPlacedInPlace(current, "attacker-spears");
  });

  test("Rust rejects placements that overlap a friendly formation", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    const archers = unitById(opened, "attacker-archers");
    const archersFootprint = footprintOf(archers);
    await selectOnly(page, "attacker-spears");
    const before = simulationTruth(await authoritativeStatus(page));

    // Onto the archers, and only partly onto them (anchors apart, footprints
    // intersecting by a metre). Each target keeps the spears' whole footprint
    // inside the zone, so the only thing wrong with it is the overlap.
    const spears = unitById(opened, "attacker-spears");
    const zone = zoneFor(opened, opened.playerSide);
    const { left, right, below, above } = extentsOf(spears);
    const xMm = Math.min(Math.max(archers.xMm, zone.minXMm + left), zone.maxXMm - right);
    const partlyAbove = archersFootprint.maxYMm + below - 1_000;
    const partly = partlyAbove + above <= zone.maxYMm
      ? partlyAbove
      : archersFootprint.minYMm - above + 1_000;
    const yOnto = Math.min(Math.max(archers.yMm, zone.minYMm + below), zone.maxYMm - above);
    for (const target of [{ xMm, yMm: yOnto }, { xMm, yMm: partly }]) {
      const footprint = shifted(spears, target.xMm, target.yMm);
      expect(insideZone(zone, footprint), "the overlap target is inside the zone").toBe(true);
      expect(overlaps(footprint, archersFootprint), "the target footprint meets the archers").toBe(true);
      const error = await rustControl(page, { kind: "deploySelected", ...target });
      expect(error, `Rust refuses overlapping the archers at ${JSON.stringify(target)}`).toMatch(/overlap/i);
      expect(simulationTruth(await authoritativeStatus(page))).toEqual(before);
    }

    // A metre clear of them is legal (if that ground is free and in the zone).
    const clear = { xMm, yMm: archersFootprint.maxYMm + below + 1_001 };
    const clearFootprint = shifted(spears, clear.xMm, clear.yMm);
    const blocked = !insideZone(zone, clearFootprint)
      || friendsOf(opened, spears).some((other) => overlaps(clearFootprint, footprintOf(other)));
    if (!blocked) {
      expect(await rustControl(page, { kind: "deploySelected", ...clear })).toBeNull();
      const current = await authoritativeStatus(page);
      expectPlacedInPlace(current, "attacker-spears");
      expectLegalLayout(current, "spears a metre clear of the archers");
    }
  });

  test("facing, formation and frontage use the Rust order APIs and obey the footprint rule", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    expect(unitById(opened, "attacker-spears").facing).toEqual({ x: 1, y: 0 });

    // Give the spears room for every edit in this first part.
    const roomy = legalTarget(opened, "attacker-spears", { xMm: 16_000, yMm: 55_000 }, { clearance: 16_000 });
    await arrange(page, { "attacker-spears": roomy });

    await clickUnit(page, "attacker-spears");
    await page.locator("#battle-canvas").focus();
    await page.keyboard.press("KeyQ");
    let current = await authoritativeStatus(page);
    let spears = unitById(current, "attacker-spears");
    expect(spears.facing).toEqual({ x: 0, y: -1 });
    expect([spears.xMm, spears.yMm]).toEqual([roomy.xMm, roomy.yMm]);
    expectPlacedInPlace(current, "attacker-spears");

    await page.locator("#frontage-metres").fill("10");
    await page.getByRole("button", { name: "Set frontage" }).click();
    await expect(page.locator("#battle-error")).toBeHidden();
    current = await authoritativeStatus(page);
    spears = unitById(current, "attacker-spears");
    expect([spears.formation, spears.formationFiles]).toEqual(["line", 10]);
    expect(spears.facing).toEqual({ x: 0, y: -1 });
    expect([spears.xMm, spears.yMm]).toEqual([roomy.xMm, roomy.yMm]);
    expectPlacedInPlace(current, "attacker-spears");
    expectLegalLayout(current, "after frontage 10");

    await page.getByRole("button", { name: "Column formation" }).click();
    await expect(page.locator("#battle-error")).toBeHidden();
    current = await authoritativeStatus(page);
    expect(unitById(current, "attacker-spears").formation).toBe("column");
    expectPlacedInPlace(current, "attacker-spears");
    expectLegalLayout(current, "after column");
    await page.getByRole("button", { name: "Line formation" }).click();
    current = await authoritativeStatus(page);
    expect(unitById(current, "attacker-spears").formation).toBe("line");

    // A frontage wider than the zone itself is rejected atomically by Rust.
    let beforeRejected = simulationTruth(current);
    await page.locator("#frontage-metres").fill("100");
    await page.getByRole("button", { name: "Set frontage" }).click();
    await expect(page.locator("#battle-error")).toBeVisible();
    await expect(page.locator("#battle-error")).toContainText(/zone/i);
    expect(simulationTruth(await authoritativeStatus(page))).toEqual(beforeRejected);

    // On the zone edge, a wider frontage would push the footprint out.
    const zone = zoneFor(current, current.playerSide);
    spears = unitById(current, "attacker-spears");
    const edge = { xMm: zone.maxXMm - extentsOf(spears).right, yMm: spears.yMm };
    current = await arrange(page, { "attacker-spears": edge });
    await clickUnit(page, "attacker-spears");
    beforeRejected = simulationTruth(current);
    await page.locator("#frontage-metres").fill(String(spears.formationFiles + 4));
    await page.getByRole("button", { name: "Set frontage" }).click();
    await expect(page.locator("#battle-error")).toBeVisible();
    await expect(page.locator("#battle-error")).toContainText(/zone/i);
    expect(simulationTruth(await authoritativeStatus(page)), "a frontage edit over the edge changes nothing")
      .toEqual(beforeRejected);

    // Hemmed in by friends (archers on one axis, knights on the other, each a
    // metre clear): a wider frontage and a deeper column both overlap.
    spears = unitById(current, "attacker-spears");
    const archers = unitById(current, "attacker-archers");
    const knights = unitById(current, "attacker-knights");
    const s = extentsOf(spears);
    const a = extentsOf(archers);
    const k = extentsOf(knights);
    const centre = {
      xMm: zone.minXMm + Math.max(s.left, a.left) + 1_000,
      yMm: Math.round((zone.minYMm + zone.maxYMm) / 2),
    };
    const hem = {
      "attacker-spears": centre,
      "attacker-archers": { xMm: centre.xMm, yMm: centre.yMm - s.below - 1_001 - a.above },
      "attacker-knights": { xMm: centre.xMm + s.right + 1_001 + k.left, yMm: centre.yMm },
    };
    expect(hem["attacker-knights"].xMm + k.right, "the hem fits the zone width").toBeLessThanOrEqual(zone.maxXMm);
    current = await arrange(page, hem);
    await clickUnit(page, "attacker-spears");
    expect((await authoritativeStatus(page)).selectedUnits).toEqual(["attacker-spears"]);
    beforeRejected = simulationTruth(current);
    await page.locator("#frontage-metres").fill(String(spears.formationFiles + 4));
    await page.getByRole("button", { name: "Set frontage" }).click();
    await expect(page.locator("#battle-error")).toBeVisible();
    await expect(page.locator("#battle-error")).toContainText(/overlap/i);
    expect(simulationTruth(await authoritativeStatus(page))).toEqual(beforeRejected);
    await page.getByRole("button", { name: "Column formation" }).click();
    await expect(page.locator("#battle-error")).toBeVisible();
    await expect(page.locator("#battle-error")).toContainText(/overlap/i);
    expect(simulationTruth(await authoritativeStatus(page)), "a deeper column next to friends changes nothing")
      .toEqual(beforeRejected);

    // The arranged formation is the battle's starting formation.
    const arranged = await authoritativeStatus(page);
    expect(inDeployment(arranged)).toBe(true);
    await startBattleButton(page).click();
    const started = await expectBattleStarted(page);
    expect(started.tick).toBe(0);
    expect(layout(started)).toEqual(layout(arranged));
    expect(unitById(started, "attacker-spears").facing).toEqual({ x: 0, y: -1 });
  });
});

test.describe("Start battle", () => {
  test.beforeEach(async ({ page }) => {
    await installManualFrameClock(page);
  });

  test("locks deployment and begins simulation exactly once", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    const target = legalTarget(opened, "attacker-spears", { xMm: 15_000, yMm: 60_000 });
    await clickUnit(page, "attacker-spears");
    await rightClickGround(page, target.xMm, target.yMm);
    await expect(page.locator("#battle-error")).toBeHidden();
    const deployed = await authoritativeStatus(page);
    expect(inDeployment(deployed)).toBe(true);

    // Two activations in the same task, before any re-render: one start.
    const start = await startBattleButton(page).elementHandle();
    await start.evaluate((button) => {
      button.click();
      button.click();
    });
    const started = await expectBattleStarted(page);
    await expect(page.locator("#battle-state")).not.toContainText(/deploy/i);
    expect(started.tick, "starting does not itself advance ticks").toBe(0);
    expect(layout(started), "the deployed layout is the tick-0 layout").toEqual(layout(deployed));

    // The scheduler now runs at the normal 1× rate: one 50 ms frame, one tick.
    await stepFrames(page, 1);
    const settled = (await authoritativeStatus(page)).tick;
    await stepFrames(page, 10);
    expect((await authoritativeStatus(page)).tick - settled).toBe(10);

    // A later Start (the handle survives even if the button was hidden or
    // removed) neither restarts, resumes nor rewinds the battle.
    await page.getByRole("button", { name: "Pause", exact: true }).click();
    const paused = await authoritativeStatus(page);
    expect(paused.paused).toBe(true);
    expect(paused.tick).toBeGreaterThan(0);
    await start.evaluate((button) => button.click());
    await stepFrames(page, 10);
    const afterSecondStart = await authoritativeStatus(page);
    expect(inDeployment(afterSecondStart)).toBe(false);
    expect(afterSecondStart.paused).toBe(true);
    expect(simulationTruth(afterSecondStart)).toEqual(simulationTruth(paused));
    expect(await rustControl(page, { kind: "startBattle" }), "Rust refuses a second start").not.toBeNull();
    expect(simulationTruth(await authoritativeStatus(page))).toEqual(simulationTruth(paused));

    // Deployment is locked: a right click is an ordinary move order again,
    // not an instant reposition.
    await clickUnit(page, "attacker-knights");
    const knightsBefore = unitById(await authoritativeStatus(page), "attacker-knights");
    const moveTo = legalTarget(paused, "attacker-knights", { xMm: 10_000, yMm: 85_000 });
    await rightClickGround(page, moveTo.xMm, moveTo.yMm);
    const knights = unitById(await authoritativeStatus(page), "attacker-knights");
    expect([knights.xMm, knights.yMm]).toEqual([knightsBefore.xMm, knightsBefore.yMm]);
    expect(knights.destination).not.toBeNull();
    await expect(page.locator("#battle-error")).toBeHidden();
  });

  test("reset and a new battlefield return to a fresh, legal deployment phase", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    const target = legalTarget(opened, "attacker-spears", { xMm: 15_000, yMm: 60_000 });
    await clickUnit(page, "attacker-spears");
    await rightClickGround(page, target.xMm, target.yMm);
    await startBattleButton(page).click();
    await expectBattleStarted(page);
    await stepFrames(page, 1);
    await stepFrames(page, 5);
    expect((await authoritativeStatus(page)).tick).toBeGreaterThan(0);

    await page.getByRole("button", { name: "Reset battle" }).click();
    await expect(deploymentRegion(page)).toBeVisible();
    await expect(startBattleButton(page)).toBeEnabled();
    const reset = await authoritativeStatus(page);
    expect(inDeployment(reset)).toBe(true);
    expect(reset.tick).toBe(0);
    expect(layout(reset), "reset restores the opening layout").toEqual(layout(opened));

    await page.locator("#battle-location").selectOption("riverFord");
    await expect.poll(async () => (await authoritativeStatus(page)).battlefieldLocation).toBe("riverFord");
    const moved = await authoritativeStatus(page);
    expect(inDeployment(moved)).toBe(true);
    expect(moved.tick).toBe(0);
    expectLegalLayout(moved, "riverFord after a location change");
    await expect(deploymentRegion(page)).toBeVisible();
    await expect(page.locator("#battle-error")).toBeHidden();
  });

  test("Withdraw is allowed during deployment and ends the battle without losses", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    expect(opened.canWithdraw, "the player may withdraw before the start").toBe(true);
    const withdraw = page.getByRole("button", { name: "Withdraw", exact: true });
    await expect(withdraw).toBeEnabled();
    await withdraw.click();
    await expect(deploymentRegion(page)).toBeHidden();
    expect(inDeployment(await authoritativeStatus(page))).toBe(false);

    for (let frame = 0; frame < 4_000; frame += 50) {
      if ((await authoritativeStatus(page)).outcome) break;
      await stepFrames(page, 50);
    }
    const after = await authoritativeStatus(page);
    expect(after.outcome).toBe("playerDefeat");
    expect(after.battleState.phase).toBe("finished");
    expect(after.battleState.reason).toBe("withdrawal");
    for (const unit of after.units.filter((candidate) => candidate.side === "player")) {
      expect(unit.escaped, `${unit.id} escaped`).toBe(true);
      expect(unit.soldiers).toBe(unitById(opened, unit.id).soldiers);
    }
    await expect(withdraw).toBeDisabled();
  });
});

test("enemy placement is deterministic, legal and unaffected by player deployment", async ({ page }) => {
  await installManualFrameClock(page);
  for (const location of ["forestClearing", "mountainPass"]) {
    const first = await enterDeployment(page, location);
    const enemies = enemyPlacement(first);
    expect(enemies.length).toBeGreaterThan(0);
    expectLegalLayout(first, `${location} opponent layout`);

    // The player's arrangement never moves the opponent.
    const target = legalTarget(first, "attacker-knights", { xMm: 12_000, yMm: 70_000 });
    await clickUnit(page, "attacker-knights");
    await rightClickGround(page, target.xMm, target.yMm);
    await page.locator("#battle-canvas").focus();
    await page.keyboard.press("KeyE");
    await stepFrames(page, 20);
    expect(enemyPlacement(await authoritativeStatus(page))).toEqual(enemies);

    // The same battle on a fresh load places the opponent identically.
    const again = await enterDeployment(page, location);
    expect(enemyPlacement(again), `${location} opponent placement is reproducible`).toEqual(enemies);
  }
});

test("a campaign Fight opens the campaign army in a legal deployment until Start battle", async ({ page }) => {
  test.setTimeout(60_000);
  await page.goto("/");
  await page.locator("#open-campaign").click();
  await page.locator('[data-province="normandy"]').click();
  await page.getByRole("button", { name: "Issue movement order", exact: true }).click();
  await page.locator('[data-province="paris"]').click();
  await expect(page.locator("#fight-battle")).toBeEnabled();
  await page.locator("#fight-battle").click();

  const battle = page.frameLocator("#campaign-battle-frame");
  await expect(battle.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  const frame = page.frame({ url: /battle\.html/ });
  if (!frame) throw new Error("campaign battle frame is not attached");
  const tacticalStatus = () => frame.evaluate(async () => {
    const runtime = await import("./pkg/medieval_web_battle.js");
    return JSON.parse(runtime.battle_sandbox_status());
  });

  await expect(battle.getByRole("region", { name: "Deployment" })).toBeVisible();
  const opened = await tacticalStatus();
  expect(inDeployment(opened)).toBe(true);
  expect(opened.units.some((unit) => unit.sourceArmyId === "england-main")).toBe(true);
  expectLegalLayout(opened, "campaign packing");
  await page.waitForTimeout(1_500);
  expect(simulationTruth(await tacticalStatus())).toEqual(simulationTruth(opened));

  // Fog (#132 B): before the start, the page shows no exact enemy counts.
  const opponentRows = battle.locator(".unit-row").filter({
    has: battle.locator(".unit-side", { hasText: /^opponent$/ }),
  });
  for (let index = 0; index < await opponentRows.count(); index += 1) {
    await expect(opponentRows.nth(index)).not.toContainText(/\d+\s+soldiers/i);
  }

  await battle.getByRole("button", { name: "Start battle", exact: true }).click();
  await expect(battle.getByRole("region", { name: "Deployment" })).toBeHidden();
  await expect.poll(async () => (await tacticalStatus()).tick, { timeout: 10_000 }).toBeGreaterThan(0);
  expect(inDeployment(await tacticalStatus())).toBe(false);
});

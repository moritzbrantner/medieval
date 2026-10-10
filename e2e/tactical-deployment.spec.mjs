// Acceptance journeys for issue #94: tactical deployment phase (pre-battle
// positioning, facing, formation and frontage, then Start battle).
//
// Contract named by these journeys (roles and Rust status, not styling hooks):
//
// * Phase. A supported single-player battle (the sandbox at every battlefield
//   location, and a campaign "Fight" battle, per the issue goal "arrange
//   campaign-seeded forces before simulation begins") opens in a deployment
//   phase. The Rust status reports it either as `status.phase === "deployment"`
//   (then `"battle"` after the start) or as
//   `status.battleState.phase === "deployment"` (a core battle-state variant).
//   While deploying, `#battle-state` mentions deployment.
// * Deployment UI. A region named "Deployment" holds a button named
//   "Start battle". Both are gone (hidden, or the button disabled) once the
//   battle has started.
// * Frozen clock. No tick runs before the start, however many frames pass.
//   The opponent does not move either.
// * Placement. With an own formation selected, a right click on the field
//   inside the player's deployment zone repositions that formation there at
//   once: its position changes, no movement order (`destination`) is left
//   behind, and the tick stays 0. Rust decides legality. A right click outside
//   the player's zone (the neutral centre or the enemy zone) changes nothing
//   and reports an error in the page's alert (`#battle-error`). Enemy
//   formations can never be selected or repositioned.
// * Facing / formation / frontage. The existing controls (Q/E, the Line and
//   Column formation buttons, Set frontage) work during deployment through the
//   same Rust order APIs and change the formation in place, with no tick.
//   Core rejections (for example a frontage that does not fit the field) still
//   reject atomically.
// * Start battle. Starting locks deployment and begins simulation once: the
//   deployed state is the tick-0 state, ticks then run at the normal 1×
//   schedule, a second Start (even two clicks in the same task) changes
//   nothing, and right clicks become ordinary move orders again.
// * Enemy placement is deterministic: the same battle always places the same
//   opponent formations inside the opponent's deployment zone, and player
//   deployment never changes them.
import { expect, test } from "@playwright/test";

const SANDBOX_LOCATIONS = ["mountainPass", "forestClearing", "riverFord"];

async function authoritativeStatus(page) {
  return page.evaluate(async () => {
    const { battle_sandbox_status } = await import("./pkg/medieval_web_battle.js");
    return JSON.parse(battle_sandbox_status());
  });
}

function inDeployment(status) {
  return status.phase === "deployment" || status.battleState?.phase === "deployment";
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

function anchorInside(zone, unit) {
  return unit.xMm >= zone.minXMm && unit.xMm <= zone.maxXMm
    && unit.yMm >= zone.minYMm && unit.yMm <= zone.maxYMm;
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
  return current.units.map(({ id, xMm, yMm, facing, formation, formationFiles, soldiers }) => (
    { id, xMm, yMm, facing, formation, formationFiles, soldiers }
  ));
}

function enemyPlacement(current) {
  return current.units
    .filter((unit) => unit.side === "opponent")
    .map(({ id, xMm, yMm, facing, formation, formationFiles, soldiers, unitKind }) => (
      { id, xMm, yMm, facing, formation, formationFiles, soldiers, unitKind }
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

async function enterDeployment(page, location) {
  const query = location ? `&location=${location}` : "";
  await page.goto(`/battle.html?e2e-controls=1${query}`);
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

test.describe("deployment phase", () => {
  test.beforeEach(async ({ page }) => {
    await installManualFrameClock(page);
  });

  for (const location of SANDBOX_LOCATIONS) {
    test(`${location} sandbox opens in a Rust deployment phase that runs no ticks`, async ({ page }) => {
      const opened = await enterDeployment(page, location);
      await expect(page.locator("#battle-state")).toContainText(/deploy/i);

      // Two seconds of wall time at 1×: forty ticks if the clock were running.
      await stepFrames(page, 40);
      const held = await authoritativeStatus(page);
      expect(inDeployment(held)).toBe(true);
      expect(simulationTruth(held)).toEqual(simulationTruth(opened));

      // Every formation starts on its own side's zone, as Rust reports it.
      for (const unit of held.units) {
        const side = unit.side === "player" ? held.playerSide : held.opponentSide;
        expect(anchorInside(zoneFor(held, side), unit), `${unit.id} starts in the ${side} zone`).toBe(true);
      }
      await expect(page.locator("#battle-error")).toBeHidden();
    });
  }

  test("the player repositions own formations inside the zone with no tick and no movement order", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    const zone = zoneFor(opened, opened.playerSide);
    expect(zone.maxXMm).toBeGreaterThan(30_000);

    await clickUnit(page, "attacker-spears");
    expect((await authoritativeStatus(page)).selectedUnits).toEqual(["attacker-spears"]);
    await rightClickGround(page, 15_000, 60_000);
    await expect(page.locator("#battle-error")).toBeHidden();

    let current = await authoritativeStatus(page);
    expect(inDeployment(current)).toBe(true);
    expect(current.tick).toBe(0);
    const spears = unitById(current, "attacker-spears");
    expectNear(spears, 15_000, 60_000);
    expect(anchorInside(zone, spears)).toBe(true);
    expect(spears.destination, "deployment places the formation; it does not march").toBeNull();
    expect(spears.queuedMovements).toEqual([]);
    for (const unit of opened.units.filter((candidate) => candidate.id !== "attacker-spears")) {
      const { selected, ...now } = unitById(current, unit.id);
      const { selected: _, ...before } = unit;
      expect(now, `${unit.id} is untouched`).toEqual(before);
    }

    await clickUnit(page, "attacker-knights");
    await rightClickGround(page, 10_000, 85_000);
    await expect(page.locator("#battle-error")).toBeHidden();
    current = await authoritativeStatus(page);
    expectNear(unitById(current, "attacker-knights"), 10_000, 85_000);
    expect(unitById(current, "attacker-knights").destination).toBeNull();
    expect(current.tick).toBe(0);

    // Frames still run no ticks after placements.
    await stepFrames(page, 20);
    expect(simulationTruth(await authoritativeStatus(page))).toEqual(simulationTruth(current));
  });

  test("Rust rejects placements outside the player's zone and never moves enemy formations", async ({ page }) => {
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
    expect(simulationTruth(await authoritativeStatus(page)), "neutral ground is rejected").toEqual(before);

    // Open ground inside the enemy zone (not on an enemy formation).
    await rightClickGround(page, 90_000, 64_000);
    await expect(page.locator("#battle-error")).toBeVisible();
    expect(simulationTruth(await authoritativeStatus(page)), "the enemy zone is rejected").toEqual(before);

    // Right clicking an enemy formation is not an engagement during deployment.
    await clickUnit(page, "defender-spears", { button: "right" });
    let current = await authoritativeStatus(page);
    expect(simulationTruth(current), "no engagement or move order before the start").toEqual(before);
    expect(unitById(current, "attacker-spears").engagementTarget).toBeNull();

    // Enemy formations cannot be selected, physically or through Rust controls.
    await clickUnit(page, "defender-archers");
    expect((await authoritativeStatus(page)).selectedUnits).toEqual([]);
    const rejected = await page.evaluate(async () => {
      const { battle_sandbox_control } = await import("./pkg/medieval_web_battle.js");
      try {
        battle_sandbox_control(JSON.stringify({ kind: "selectReplace", unitIds: ["defender-spears"] }));
        return null;
      } catch (error) {
        return String(error);
      }
    });
    expect(rejected, "Rust refuses to hand an enemy formation to the player").not.toBeNull();
    current = await authoritativeStatus(page);
    expect(current.selectedUnits).toEqual([]);
    expect(enemyPlacement(current)).toEqual(enemyPlacement(opened));

    // A legal placement afterwards still works and clears the error.
    await clickUnit(page, "attacker-spears");
    await rightClickGround(page, 15_000, 60_000);
    await expect(page.locator("#battle-error")).toBeHidden();
    current = await authoritativeStatus(page);
    expectNear(unitById(current, "attacker-spears"), 15_000, 60_000);
    expect(inDeployment(current)).toBe(true);
    expect(current.tick).toBe(0);
  });

  test("facing, formation and frontage use the Rust order APIs during deployment", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    const origin = unitById(opened, "attacker-spears");
    expect(origin.facing).toEqual({ x: 1, y: 0 });

    await clickUnit(page, "attacker-spears");
    await page.locator("#battle-canvas").focus();
    await page.keyboard.press("KeyQ");
    let current = await authoritativeStatus(page);
    let spears = unitById(current, "attacker-spears");
    expect(spears.facing).toEqual({ x: 0, y: -1 });
    expect([spears.xMm, spears.yMm, spears.destination]).toEqual([origin.xMm, origin.yMm, null]);
    expect(current.tick).toBe(0);
    expect(inDeployment(current)).toBe(true);

    await page.locator("#frontage-metres").fill("10");
    await page.getByRole("button", { name: "Set frontage" }).click();
    await expect(page.locator("#battle-error")).toBeHidden();
    current = await authoritativeStatus(page);
    spears = unitById(current, "attacker-spears");
    expect([spears.formation, spears.formationFiles]).toEqual(["line", 10]);
    expect(spears.facing).toEqual({ x: 0, y: -1 });
    expect([spears.xMm, spears.yMm, spears.destination]).toEqual([origin.xMm, origin.yMm, null]);

    await page.getByRole("button", { name: "Column formation" }).click();
    current = await authoritativeStatus(page);
    expect(unitById(current, "attacker-spears").formation).toBe("column");

    // A frontage that cannot fit the field is rejected atomically by Rust.
    const beforeRejected = simulationTruth(current);
    await page.locator("#frontage-metres").fill("100");
    await page.getByRole("button", { name: "Set frontage" }).click();
    await expect(page.locator("#battle-error")).toBeVisible();
    const afterRejected = await authoritativeStatus(page);
    expect(simulationTruth(afterRejected)).toEqual(beforeRejected);
    expect(afterRejected.tick).toBe(0);
    expect(inDeployment(afterRejected)).toBe(true);

    // The arranged formation is the battle's starting formation.
    await startBattleButton(page).click();
    const started = await expectBattleStarted(page);
    spears = unitById(started, "attacker-spears");
    expect(started.tick).toBe(0);
    expect(spears.facing).toEqual({ x: 0, y: -1 });
    expect(spears.formation).toBe("column");
    expect(spears.formationFiles).toBe(10);
  });
});

test.describe("Start battle", () => {
  test.beforeEach(async ({ page }) => {
    await installManualFrameClock(page);
  });

  test("locks deployment and begins simulation exactly once", async ({ page }) => {
    await enterDeployment(page, "forestClearing");
    await clickUnit(page, "attacker-spears");
    await rightClickGround(page, 15_000, 60_000);
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

    // Deployment is locked: a right click inside the zone is an ordinary move
    // order again, not an instant reposition.
    await clickUnit(page, "attacker-knights");
    const knightsBefore = unitById(await authoritativeStatus(page), "attacker-knights");
    await rightClickGround(page, 10_000, 85_000);
    const knights = unitById(await authoritativeStatus(page), "attacker-knights");
    expect([knights.xMm, knights.yMm]).toEqual([knightsBefore.xMm, knightsBefore.yMm]);
    expect(knights.destination).not.toBeNull();
    await expect(page.locator("#battle-error")).toBeHidden();
  });

  test("reset and a new battlefield return to a fresh deployment phase", async ({ page }) => {
    const opened = await enterDeployment(page, "forestClearing");
    await clickUnit(page, "attacker-spears");
    await rightClickGround(page, 15_000, 60_000);
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
    expect(unitById(reset, "attacker-spears").xMm).toBe(unitById(opened, "attacker-spears").xMm);
    expect(unitById(reset, "attacker-spears").yMm).toBe(unitById(opened, "attacker-spears").yMm);

    await page.locator("#battle-location").selectOption("riverFord");
    await expect.poll(async () => (await authoritativeStatus(page)).battlefieldLocation).toBe("riverFord");
    const moved = await authoritativeStatus(page);
    expect(inDeployment(moved)).toBe(true);
    expect(moved.tick).toBe(0);
    await expect(deploymentRegion(page)).toBeVisible();
    await expect(page.locator("#battle-error")).toBeHidden();
  });
});

test("enemy placement is deterministic and unaffected by player deployment", async ({ page }) => {
  await installManualFrameClock(page);
  for (const location of ["forestClearing", "mountainPass"]) {
    const first = await enterDeployment(page, location);
    const enemies = enemyPlacement(first);
    expect(enemies.length).toBeGreaterThan(0);
    const zone = zoneFor(first, first.opponentSide);
    for (const unit of first.units.filter((candidate) => candidate.side === "opponent")) {
      expect(anchorInside(zone, unit), `${unit.id} is placed in the opponent zone`).toBe(true);
    }

    // The player's arrangement never moves the opponent.
    await clickUnit(page, "attacker-knights");
    await rightClickGround(page, 12_000, 70_000);
    await stepFrames(page, 20);
    expect(enemyPlacement(await authoritativeStatus(page))).toEqual(enemies);

    // The same battle on a fresh load places the opponent identically.
    const again = await enterDeployment(page, location);
    expect(enemyPlacement(again), `${location} opponent placement is reproducible`).toEqual(enemies);
  }
});

test("a campaign Fight opens the campaign army in deployment until Start battle", async ({ page }) => {
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
  await page.waitForTimeout(1_500);
  expect(simulationTruth(await tacticalStatus())).toEqual(simulationTruth(opened));

  await battle.getByRole("button", { name: "Start battle", exact: true }).click();
  await expect(battle.getByRole("region", { name: "Deployment" })).toBeHidden();
  await expect.poll(async () => (await tacticalStatus()).tick, { timeout: 10_000 }).toBeGreaterThan(0);
  expect(inDeployment(await tacticalStatus())).toBe(false);
});

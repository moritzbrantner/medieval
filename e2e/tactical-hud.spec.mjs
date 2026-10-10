// Acceptance journey for issue #96: tactical battle HUD (unit status, battle
// state, pause, simulation speed, control groups, compact layout).
//
// The contract below names the accessible structure the HUD must expose so it
// can be driven by roles and labels instead of styling hooks:
//
// * Battle status HUD: a region named "Battle status" (for example
//   `<section aria-label="Battle status">`) that contains `#battle-state`,
//   an element `[data-field="player-side"]` naming the player's side
//   ("Attacker" in the single-player sandbox), an element
//   `[data-field="opponent-side"]` naming the opponent side ("Defender"), and,
//   only while the Rust status carries a siege, a progressbar named
//   "Siege capture progress" whose min/max/now values are 0 / 1000 /
//   `status.siege.capture.progress` (1000 is core
//   `SIEGE_CAPTURE_MAX_PROGRESS`).
// * Simulation speed: a group (fieldset/legend or role=radiogroup) named
//   "Simulation speed" with one radio per documented multiplier, named
//   "0.5×", "1×", "2×" and "4×" ("x" is accepted for "×"). 1× is the default.
//   The Rust sandbox status reports the active multiplier as
//   `status.speedMultiplier` (a number), next to the existing `paused` flag.
// * Selected unit cards: a region named "Selected units" holding exactly one
//   card per Rust-selected unit, each carrying `data-unit-id="<unit id>"` and
//   child fields `[data-field="kind" | "soldiers" | "morale" | "fatigue" |
//   "ammunition" | "formation" | "order"]` projected from Rust status.
// * Control groups: a region or group named "Control groups". Ctrl+1..9
//   assigns the current selection (native parity, see
//   src-tauri/src/native_battle/input.rs), and every assigned group appears as
//   a focusable button whose name starts "Control group <n>" and mentions its
//   unit count ("2 units"). Activating the button (or pressing the digit on the
//   battlefield) recalls the group through Rust tactical controls.
import { expect, test } from "@playwright/test";

const SPEEDS = [
  { label: /^0\.5\s?[×x]$/, multiplier: 0.5 },
  { label: /^1\s?[×x]$/, multiplier: 1 },
  { label: /^2\s?[×x]$/, multiplier: 2 },
  { label: /^4\s?[×x]$/, multiplier: 4 },
];
const SIEGE_CAPTURE_MAX_PROGRESS = 1_000;

async function status(page) {
  return page.evaluate(() => window.__medievalControlsE2E.status());
}

// Reads the authoritative Rust status directly instead of the page's
// 200 ms-throttled HUD copy.
async function authoritativeStatus(page) {
  return page.evaluate(async () => {
    const { battle_sandbox_status } = await import("./pkg/medieval_web_battle.js");
    return JSON.parse(battle_sandbox_status());
  });
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

async function enterBattle(page, location) {
  const query = location ? `&location=${location}` : "";
  await page.goto(`/battle.html?e2e-controls=1${query}`);
  await page.getByRole("button", { name: "Enter battle" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-controls-e2e-ready", "true");
  await expect(page.locator("#battle-error")).toBeHidden();
}

function speedGroup(page) {
  return page
    .getByRole("group", { name: "Simulation speed" })
    .or(page.getByRole("radiogroup", { name: "Simulation speed" }));
}

function speedRadio(page, multiplier) {
  const { label } = SPEEDS.find((speed) => speed.multiplier === multiplier);
  return speedGroup(page).getByRole("radio", { name: label });
}

function battleStatusHud(page) {
  return page.getByRole("region", { name: "Battle status" });
}

function selectedUnitCards(page) {
  return page.getByRole("region", { name: "Selected units" });
}

function controlGroups(page) {
  return page
    .getByRole("region", { name: "Control groups" })
    .or(page.getByRole("group", { name: "Control groups" }));
}

// Replaces requestAnimationFrame with a manual clock so the test, not the
// display refresh rate, decides how much wall time each frame represents.
// The page's own animate() loop still drives battle_sandbox_frame().
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

async function stepFrames(page, frames, deltaMs) {
  await page.evaluate(
    ({ frames, deltaMs }) => {
      for (let index = 0; index < frames; index += 1) window.__manualFrames.step(deltaMs);
    },
    { frames, deltaMs },
  );
}

// Steps frames of `deltaMs` until the authoritative tick reaches `target`.
// Returns the tick it stopped on, so overshoot is visible to the caller.
async function runUntilTick(page, target, deltaMs, maxFrames = 1_000) {
  return page.evaluate(
    async ({ target, deltaMs, maxFrames }) => {
      const { battle_sandbox_status } = await import("./pkg/medieval_web_battle.js");
      const tick = () => JSON.parse(battle_sandbox_status()).tick;
      for (let frame = 0; frame < maxFrames && tick() < target; frame += 1) {
        window.__manualFrames.step(deltaMs);
      }
      return tick();
    },
    { target, deltaMs, maxFrames },
  );
}

// Simulation truth: everything the core owns. Camera, selection, pause and
// speed are presentation/scheduling state and are deliberately excluded.
function simulationTruth(current) {
  return {
    tick: current.tick,
    battleState: current.battleState,
    outcome: current.outcome,
    canWithdraw: current.canWithdraw,
    siege: current.siege,
    units: current.units.map(({ selected, ...unit }) => unit),
  };
}

async function selectSpeed(page, multiplier) {
  await speedRadio(page, multiplier).check();
  await expect(speedRadio(page, multiplier)).toBeChecked();
  expect((await authoritativeStatus(page)).speedMultiplier).toBe(multiplier);
}

async function resetBattle(page) {
  await page.getByRole("button", { name: "Reset battle" }).click();
  expect((await authoritativeStatus(page)).tick).toBe(0);
}

test.describe("simulation speed and deterministic pause", () => {
  test.beforeEach(async ({ page }) => {
    await installManualFrameClock(page);
  });

  test("documented speed multipliers default to 1× and scale ticks per frame", async ({ page }) => {
    await enterBattle(page, "forestClearing");
    await expect(speedGroup(page)).toBeVisible();
    for (const { multiplier } of SPEEDS) {
      await expect(speedRadio(page, multiplier)).toBeVisible();
    }
    await expect(speedRadio(page, 1)).toBeChecked();
    expect((await authoritativeStatus(page)).speedMultiplier).toBe(1);

    // TACTICAL_TICKS_PER_SECOND is 20, so one 50 ms frame is exactly one
    // tick of wall time at 1×. Speed must only change how many ticks the
    // scheduler runs for that wall time.
    for (const { multiplier } of SPEEDS) {
      await selectSpeed(page, multiplier);
      await stepFrames(page, 1, 50); // settle the frame clock after the change
      const before = (await authoritativeStatus(page)).tick;
      await stepFrames(page, 2, 50);
      const after = (await authoritativeStatus(page)).tick;
      expect(after - before, `ticks for 100 ms of wall time at ${multiplier}×`).toBe(2 * multiplier);
    }
    await expect(page.locator("#battle-error")).toBeHidden();
  });

  test("pause holds the tick and simulation truth at every speed and resumes without catch-up", async ({ page }) => {
    await enterBattle(page, "forestClearing");
    await runUntilTick(page, 10, 50);

    await page.getByRole("button", { name: "Pause", exact: true }).click();
    await expect(page.locator("#battle-state")).toContainText("Paused");
    const paused = await authoritativeStatus(page);
    expect(paused.paused).toBe(true);

    for (const { multiplier } of SPEEDS) {
      await speedRadio(page, multiplier).check();
      await stepFrames(page, 20, 50);
      const held = await authoritativeStatus(page);
      expect(held.paused, `still paused after selecting ${multiplier}×`).toBe(true);
      expect(held.speedMultiplier).toBe(multiplier);
      expect(simulationTruth(held), `paused at ${multiplier}×`).toEqual(simulationTruth(paused));
    }

    await speedRadio(page, 1).check();
    await page.getByRole("button", { name: "Resume", exact: true }).click();
    // One second of wall time passed while paused; resuming must not replay it.
    await stepFrames(page, 1, 50);
    const resumed = await authoritativeStatus(page);
    expect(resumed.paused).toBe(false);
    expect(resumed.tick - paused.tick).toBeLessThanOrEqual(1);
    await stepFrames(page, 4, 50);
    expect((await authoritativeStatus(page)).tick).toBeGreaterThan(paused.tick);
  });

  test("the same tick yields identical simulation truth at every speed", async ({ page }) => {
    // Default mountainPass siege battle so siege capture state is compared too.
    await enterBattle(page);
    const target = 40;

    await resetBattle(page);
    await selectSpeed(page, 1);
    expect(await runUntilTick(page, target, 50)).toBe(target);
    const reference = simulationTruth(await authoritativeStatus(page));
    expect(reference.siege).not.toBeNull();

    for (const multiplier of [0.5, 2, 4]) {
      await resetBattle(page);
      await selectSpeed(page, multiplier);
      expect(await runUntilTick(page, target, 50), `reached tick ${target} at ${multiplier}×`).toBe(target);
      expect(simulationTruth(await authoritativeStatus(page)), `truth at ${multiplier}×`).toEqual(reference);
    }
  });

  test("simulation truth does not depend on how a fast speed splits ticks across frames", async ({ page }) => {
    // At 4× a 37.5 ms frame schedules exactly three ticks, so tick batches no
    // longer line up with once-per-second boundaries (opponent replanning,
    // combat pulses). Truth at a given tick must still match 1×.
    await enterBattle(page);
    const target = 60;

    await resetBattle(page);
    await selectSpeed(page, 1);
    expect(await runUntilTick(page, target, 50)).toBe(target);
    const reference = simulationTruth(await authoritativeStatus(page));

    await resetBattle(page);
    await selectSpeed(page, 4);
    expect(await runUntilTick(page, target, 37.5)).toBe(target);
    expect(simulationTruth(await authoritativeStatus(page))).toEqual(reference);
  });
});

test("battle status HUD names both sides and projects Rust siege capture progress", async ({ page }) => {
  await enterBattle(page);
  const hud = battleStatusHud(page);
  await expect(hud).toBeVisible();
  await expect(hud.locator("#battle-state")).toBeVisible();
  await expect(hud.locator('[data-field="player-side"]')).toContainText(/attacker/i);
  await expect(hud.locator('[data-field="opponent-side"]')).toContainText(/defender/i);

  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await expect(hud.locator("#battle-state")).toContainText("Paused");
  const current = await authoritativeStatus(page);
  expect(current.siege).not.toBeNull();
  const progress = hud.getByRole("progressbar", { name: "Siege capture progress" });
  await expect(progress).toBeVisible();
  const values = await progress.evaluate((element) => ({
    min: Number(element.getAttribute("aria-valuemin") ?? element.min ?? 0),
    max: Number(element.getAttribute("aria-valuemax") ?? element.max),
    now: Number(element.getAttribute("aria-valuenow") ?? element.value),
  }));
  expect(values).toEqual({
    min: 0,
    max: SIEGE_CAPTURE_MAX_PROGRESS,
    now: current.siege.capture.progress,
  });

  await page.locator("#battle-location").selectOption("forestClearing");
  await expect.poll(async () => (await authoritativeStatus(page)).siege).toBeNull();
  await expect(hud.getByRole("progressbar", { name: "Siege capture progress" })).toBeHidden();
  await expect(hud.locator('[data-field="player-side"]')).toContainText(/attacker/i);
  await expect(page.locator("#battle-error")).toBeHidden();
});

test("selected unit cards project kind, strength, morale, fatigue, ammunition, formation and order from Rust", async ({ page }) => {
  await enterBattle(page, "forestClearing");
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  const cards = selectedUnitCards(page);

  await expect(cards.locator("[data-unit-id]")).toHaveCount(0);

  async function expectCard(unitId) {
    const current = await authoritativeStatus(page);
    const unit = current.units.find((candidate) => candidate.id === unitId);
    const card = cards.locator(`[data-unit-id="${unitId}"]`);
    await expect(card).toBeVisible();
    const kind = { levy: "Levy", spearmen: "Spearmen", archers: "Archers", knights: "Knights" }[unit.unitKind];
    await expect(card.locator('[data-field="kind"]')).toContainText(kind);
    await expect(card.locator('[data-field="soldiers"]')).toContainText(String(unit.soldiers));
    await expect(card.locator('[data-field="morale"]')).toContainText(String(unit.morale));
    await expect(card.locator('[data-field="fatigue"]')).toContainText(String(unit.fatigue));
    const ammunition = card.locator('[data-field="ammunition"]');
    await expect(ammunition).toBeAttached();
    if (Number.isInteger(unit.ammunition)) {
      await expect(ammunition).toContainText(String(unit.ammunition));
    } else {
      await expect(ammunition).not.toContainText(/\d/);
    }
    const formation = card.locator('[data-field="formation"]');
    await expect(formation).toContainText(new RegExp(unit.formation, "i"));
    await expect(formation).toContainText(String(unit.formationFiles));
    return { card, order: card.locator('[data-field="order"]'), unit };
  }

  await clickUnit(page, "attacker-archers");
  expect((await authoritativeStatus(page)).selectedUnits).toEqual(["attacker-archers"]);
  await expect(cards.locator("[data-unit-id]")).toHaveCount(1);
  let { order } = await expectCard("attacker-archers");
  await expect(order).toContainText(/hold|idle|no order|stopped/i);

  await clickUnit(page, "attacker-spears", { shift: true });
  await expect(cards.locator("[data-unit-id]")).toHaveCount(2);
  await expectCard("attacker-spears");
  ({ order } = await expectCard("attacker-archers"));

  // Move order.
  await clickUnit(page, "attacker-spears");
  await expect(cards.locator("[data-unit-id]")).toHaveCount(1);
  const ground = await canvasPoint(page, "groundViewport", 22_000, 30_000);
  await page.mouse.click(ground.x, ground.y, { button: "right" });
  expect((await authoritativeStatus(page)).units.find((unit) => unit.id === "attacker-spears").destination).not.toBeNull();
  ({ order } = await expectCard("attacker-spears"));
  await expect(order).toContainText(/move|march/i);
  await expect(order).not.toContainText(/attack/i);

  // Attack-move order.
  await page.keyboard.press("KeyF");
  const attackPoint = await canvasPoint(page, "groundViewport", 40_000, 30_000);
  await page.mouse.click(attackPoint.x, attackPoint.y, { button: "right" });
  expect((await authoritativeStatus(page)).units.find((unit) => unit.id === "attacker-spears").movementMode).toBe("attackMove");
  ({ order } = await expectCard("attacker-spears"));
  await expect(order).toContainText(/attack.?move/i);

  // Engagement order names its target.
  await clickUnit(page, "defender-spears", { button: "right" });
  expect((await authoritativeStatus(page)).units.find((unit) => unit.id === "attacker-spears").engagementTarget).toBe("defender-spears");
  ({ order } = await expectCard("attacker-spears"));
  await expect(order).toContainText(/engag/i);
  await expect(order).toContainText(/spear/i);

  // Stop returns the order to holding.
  await page.keyboard.press("Space");
  ({ order } = await expectCard("attacker-spears"));
  await expect(order).toContainText(/hold|idle|no order|stopped/i);

  // Formation change is reflected from Rust, not echoed from the button.
  await page.getByRole("button", { name: "Column formation" }).click();
  expect((await authoritativeStatus(page)).units.find((unit) => unit.id === "attacker-spears").formation).toBe("column");
  await expectCard("attacker-spears");
  await expect(page.locator("#battle-error")).toBeHidden();
});

test("control groups are visible, focusable and recall through Rust", async ({ page }) => {
  await enterBattle(page, "forestClearing");
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  const groups = controlGroups(page);

  await clickUnit(page, "attacker-spears");
  await clickUnit(page, "attacker-archers", { shift: true });
  await page.locator("#battle-canvas").focus();
  await page.keyboard.press("Control+Digit1");

  await expect(groups).toBeVisible();
  const groupOne = groups.getByRole("button", { name: /^Control group 1\b/ });
  await expect(groupOne).toBeVisible();
  await expect(groupOne).toHaveAccessibleName(/2 units/);

  await clickUnit(page, "attacker-knights");
  expect((await authoritativeStatus(page)).selectedUnits).toEqual(["attacker-knights"]);
  await page.locator("#battle-canvas").focus();
  await page.keyboard.press("Control+Digit2");
  const groupTwo = groups.getByRole("button", { name: /^Control group 2\b/ });
  await expect(groupTwo).toHaveAccessibleName(/1 unit\b/);

  // Keyboard focus reaches the group, and activating it recalls the Rust group.
  await groupOne.focus();
  await expect(groupOne).toBeFocused();
  await page.keyboard.press("Enter");
  expect((await authoritativeStatus(page)).selectedUnits).toEqual(["attacker-archers", "attacker-spears"]);
  await expect(selectedUnitCards(page).locator("[data-unit-id]")).toHaveCount(2);

  // The battlefield digit binding recalls the same group state.
  await page.locator("#battle-canvas").focus();
  await page.keyboard.press("Digit2");
  expect((await authoritativeStatus(page)).selectedUnits).toEqual(["attacker-knights"]);

  // Groups belong to the Rust battle controls, so a reset battle has none.
  await page.getByRole("button", { name: "Reset battle" }).click();
  await expect(groups.getByRole("button", { name: /^Control group \d/ })).toHaveCount(0);
  await expect(page.locator("#battle-error")).toBeHidden();
});

test("battle HUD stays compact and action-oriented", async ({ page }) => {
  await enterBattle(page);
  // Primary battle controls are reachable without scrolling at 1280×900.
  await expect(battleStatusHud(page)).toBeInViewport();
  await expect(page.getByRole("button", { name: "Pause", exact: true })).toBeInViewport();
  await expect(speedGroup(page)).toBeInViewport();
  // The battlefield remains the dominant surface.
  const canvas = await page.locator("#battle-canvas").boundingBox();
  const viewport = page.viewportSize();
  expect(canvas.width).toBeGreaterThanOrEqual(viewport.width * 0.6);
  // No developer/diagnostic prose inside the battle interface.
  await expect(page.locator("#battle-stage").getByText(/JavaScript only adapts/i)).toHaveCount(0);
  await page.screenshot({ path: test.info().outputPath("tactical-hud.png") });
});

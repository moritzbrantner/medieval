from pathlib import Path
import json


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"missing expected block in {path}: {old[:160]!r}")
    file.write_text(text.replace(old, new, 1), encoding="utf-8")


replace_once(
    "web-battle-wasm/src/lib.rs",
    '''struct SandboxStatus {
    tick: u64,
    paused: bool,
    outcome: Option<&'static str>,
    selected_units: Vec<String>,
    units: Vec<UnitStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnitStatus {''',
    '''struct SandboxStatus {
    tick: u64,
    paused: bool,
    outcome: Option<&'static str>,
    selected_units: Vec<String>,
    camera: CameraStatus,
    units: Vec<UnitStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CameraStatus {
    target_x_mm: f32,
    target_z_mm: f32,
    distance_mm: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewportPoint {
    x_px: f64,
    y_px: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnitStatus {''',
)

replace_once(
    "web-battle-wasm/src/lib.rs",
    '''    engagement_target: Option<String>,
    x_mm: u32,
    y_mm: u32,
}''',
    '''    engagement_target: Option<String>,
    destination: Option<BattlePoint>,
    x_mm: u32,
    y_mm: u32,
}''',
)

replace_once(
    "web-battle-wasm/src/lib.rs",
    '''                engagement_target: unit.engagement_target().map(str::to_owned),
                x_mm: unit.position().x_mm,''',
    '''                engagement_target: unit.engagement_target().map(str::to_owned),
                destination: unit.destination(),
                x_mm: unit.position().x_mm,''',
)

replace_once(
    "web-battle-wasm/src/lib.rs",
    '''            selected_units: selected.into_iter().map(str::to_owned).collect(),
            units,
        })''',
    '''            selected_units: selected.into_iter().map(str::to_owned).collect(),
            camera: CameraStatus {
                target_x_mm: snapshot.camera.target_x_mm(),
                target_z_mm: snapshot.camera.target_z_mm(),
                distance_mm: snapshot.camera.distance_mm(),
            },
            units,
        })''',
)

replace_once(
    "web-battle-wasm/src/lib.rs",
    '''#[wasm_bindgen]
pub fn battle_sandbox_control(request_json: &str) -> Result<String, JsValue> {''',
    '''#[wasm_bindgen]
pub fn battle_sandbox_unit_viewport(
    unit_id: &str,
    width_px: f64,
    height_px: f64,
) -> Result<String, JsValue> {
    with_sandbox(|sandbox| {
        validate_viewport(0.0, 0.0, width_px, height_px)?;
        let snapshot = sandbox.snapshot();
        let unit = snapshot
            .units
            .iter()
            .find(|unit| unit.unit_id == unit_id)
            .ok_or_else(|| format!("unit {unit_id} is not visible"))?;
        let (x_px, y_px) = projected_pixel(
            &snapshot,
            unit.interaction_anchor_mm(),
            width_px,
            height_px,
        )
        .ok_or_else(|| format!("unit {unit_id} does not project into the viewport"))?;
        serde_json::to_string(&ViewportPoint { x_px, y_px }).map_err(|error| error.to_string())
    })
}

#[wasm_bindgen]
pub fn battle_sandbox_ground_viewport(
    x_mm: u32,
    y_mm: u32,
    width_px: f64,
    height_px: f64,
) -> Result<String, JsValue> {
    with_sandbox(|sandbox| {
        validate_viewport(0.0, 0.0, width_px, height_px)?;
        let snapshot = sandbox.snapshot();
        if x_mm > snapshot.battlefield.width_mm || y_mm > snapshot.battlefield.depth_mm {
            return Err("projected ground point must be inside the battlefield".to_owned());
        }
        let [x_px, y_px] = snapshot
            .camera
            .project_ground_point(
                snapshot.battlefield,
                BattlePoint::new(x_mm, y_mm),
                width_px as f32,
                height_px as f32,
            )
            .ok_or_else(|| "ground point does not project into the viewport".to_owned())?;
        serde_json::to_string(&ViewportPoint {
            x_px: f64::from(x_px),
            y_px: f64::from(y_px),
        })
        .map_err(|error| error.to_string())
    })
}

#[wasm_bindgen]
pub fn battle_sandbox_control(request_json: &str) -> Result<String, JsValue> {''',
)

replace_once(
    "web/battle-sandbox.js",
    '''  battle_sandbox_frame,
  battle_sandbox_pan,
  battle_sandbox_pointer,''',
    '''  battle_sandbox_frame,
  battle_sandbox_ground_viewport,
  battle_sandbox_pan,
  battle_sandbox_pointer,''',
)
replace_once(
    "web/battle-sandbox.js",
    '''  battle_sandbox_start,
  battle_sandbox_status,
} from "./pkg/medieval_web_battle.js";''',
    '''  battle_sandbox_start,
  battle_sandbox_status,
  battle_sandbox_unit_viewport,
} from "./pkg/medieval_web_battle.js";''',
)
replace_once(
    "web/battle-sandbox.js",
    '''let currentStatus;
let animationActive = true;''',
    '''let currentStatus;
const controlsE2E = new URLSearchParams(window.location.search).has("e2e-controls");
let animationActive = true;''',
)
replace_once(
    "web/battle-sandbox.js",
    '''function runControl(request) {''',
    '''function projectForControlsE2E(projector, ...args) {
  const rect = canvas.getBoundingClientRect();
  return JSON.parse(projector(...args, rect.width, rect.height));
}

if (controlsE2E) {
  window.__medievalControlsE2E = Object.freeze({
    status: () => structuredClone(currentStatus),
    unitViewport: (unitId) => projectForControlsE2E(battle_sandbox_unit_viewport, unitId),
    groundViewport: (xMm, yMm) => projectForControlsE2E(
      battle_sandbox_ground_viewport,
      xMm,
      yMm,
    ),
  });
}

function runControl(request) {''',
)
replace_once(
    "web/battle-sandbox.js",
    '''    renderStatus(await battle_sandbox_start(canvas.id));
    canvas.focus();
    requestAnimationFrame(animate);''',
    '''    renderStatus(await battle_sandbox_start(canvas.id));
    canvas.focus();
    if (controlsE2E) document.documentElement.dataset.controlsE2eReady = "true";
    requestAnimationFrame(animate);''',
)

package = {
    "name": "medieval-browser-acceptance",
    "private": True,
    "scripts": {
        "test:e2e:controls": "playwright test e2e/tactical-controls.spec.mjs --project=chromium"
    },
    "devDependencies": {"@playwright/test": "1.63.0"},
}
Path("package.json").write_text(json.dumps(package, indent=2) + "\n", encoding="utf-8")

Path("playwright.config.mjs").write_text(
    '''import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  expect: { timeout: 5_000 },
  retries: 0,
  workers: 1,
  reporter: "line",
  use: {
    baseURL: "http://127.0.0.1:4173",
    headless: true,
    viewport: { width: 1280, height: 900 },
    launchOptions: {
      args: [
        "--enable-unsafe-webgpu",
        "--enable-features=Vulkan",
        "--use-angle=swiftshader",
      ],
    },
  },
  projects: [{ name: "chromium", use: { browserName: "chromium" } }],
  webServer: {
    command: "python3 -m http.server 4173 --directory web --bind 127.0.0.1",
    url: "http://127.0.0.1:4173/battle.html",
    reuseExistingServer: false,
    timeout: 15_000,
  },
});
''',
    encoding="utf-8",
)

Path("e2e").mkdir(exist_ok=True)
Path("e2e/tactical-controls.spec.mjs").write_text(
    '''import { expect, test } from "@playwright/test";

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

  await clickUnit(page, "attacker-spears");
  await expect(page.locator("#battle-selection")).toContainText("Spears");
  expect((await status(page)).selectedUnits).toEqual(["attacker-spears"]);

  await clickUnit(page, "attacker-archers", { shift: true });
  expect((await status(page)).selectedUnits).toEqual([
    "attacker-archers",
    "attacker-spears",
  ]);

  await clickUnit(page, "defender-spears", { button: "right" });
  let current = await status(page);
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
''',
    encoding="utf-8",
)

replace_once(
    ".github/workflows/pages.yml",
    '''      - name: Check browser JavaScript syntax
        run: find web -type f \\( -name '*.js' -o -name '*.mjs' \\) -print0 | sort -z | xargs -0 -n1 node --check
      - name: Test web contracts
        run: node --test web-tests/*.test.mjs
      - name: Upload Pages artifact''',
    '''      - uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020
        with:
          node-version: 24
      - name: Install browser acceptance dependencies
        run: npm ci
      - name: Install Chromium for tactical controls E2E
        run: npx playwright install --with-deps chromium
      - name: Check browser JavaScript syntax
        run: find web -type f \\( -name '*.js' -o -name '*.mjs' \\) -print0 | sort -z | xargs -0 -n1 node --check
      - name: Test web contracts
        run: node --test web-tests/*.test.mjs
      - name: Test tactical controls end to end
        run: npm run test:e2e:controls
      - name: Upload Pages artifact''',
)

replace_once(
    "docs/BATTLE-ARCHITECTURE.md",
    '''- browser and desktop consume the same Rust renderer and semantic commands;
- GPU projection, unit picking, and order placement derive from the same camera geometry;''',
    '''- browser and desktop consume the same Rust renderer and semantic commands;
- browser controls have end-to-end acceptance that drives real pointer/keyboard events through JavaScript, WASM, and Rust-owned control/battle state without calling control commands directly from the test;
- GPU projection, unit picking, and order placement derive from the same camera geometry;''',
)

use std::{cell::RefCell, cmp::Ordering, collections::BTreeSet};

use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TACTICAL_TICKS_PER_SECOND, TacticalBattle,
    TacticalUnit,
};
use medieval_renderer::{BattleRenderSnapshot, GpuBattleRenderer};
use serde::Serialize;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::HtmlCanvasElement;

mod controls;
use controls::{TacticalControlRequest, TacticalControls};

const MAX_FRAME_DELTA_MS: f64 = 250.0;
const MAX_TICKS_PER_FRAME: u32 = 5;
const OPPONENT_REPLAN_TICKS: u64 = TACTICAL_TICKS_PER_SECOND as u64;
const PICK_RADIUS_PX: f64 = 34.0;
const MIN_CAMERA_ZOOM: f32 = 0.05;
const CAMERA_PAN_MM: f32 = 5_000.0;
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.055,
    g: 0.047,
    b: 0.035,
    a: 1.0,
};

thread_local! {
    static SANDBOX: RefCell<Option<BrowserSandbox>> = const { RefCell::new(None) };
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SandboxStatus {
    tick: u64,
    paused: bool,
    outcome: Option<&'static str>,
    selected_units: Vec<String>,
    units: Vec<UnitStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnitStatus {
    id: String,
    side: &'static str,
    soldiers: u16,
    morale: u16,
    fatigue: u16,
    routed: bool,
    destroyed: bool,
    selected: bool,
    engagement_target: Option<String>,
    x_mm: u32,
    y_mm: u32,
}

struct BrowserSandbox {
    canvas: HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: GpuBattleRenderer,
    battle: TacticalBattle,
    controls: TacticalControls,
    last_frame_ms: Option<f64>,
    tick_accumulator: f64,
    last_opponent_plan_tick: u64,
    paused: bool,
}

impl BrowserSandbox {
    async fn new(canvas: HtmlCanvasElement) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let surface: wgpu::Surface<'static> = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|error| format!("could not create WebGPU canvas surface: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("could not select a WebGPU adapter: {error}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Medieval browser tactical device"),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("could not create the WebGPU device: {error}"))?;

        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| "selected WebGPU adapter cannot present to the battle canvas".to_owned())?;
        surface.configure(&device, &config);

        let mut battle = sample_battle()?;
        drive_opponent(&mut battle)?;
        let controls = TacticalControls::new(&battle, BattleSide::Attacker);
        let snapshot = BattleRenderSnapshot::project(&battle, &controls.render_view(&battle));
        let mut renderer = GpuBattleRenderer::new(&device, config.format);
        renderer.upload_snapshot(&device, &queue, &snapshot);

        let mut sandbox = Self {
            canvas,
            surface,
            device,
            queue,
            config,
            renderer,
            battle,
            controls,
            last_frame_ms: None,
            tick_accumulator: 0.0,
            last_opponent_plan_tick: 0,
            paused: false,
        };
        sandbox.render()?;
        Ok(sandbox)
    }

    fn reset(&mut self) -> Result<(), String> {
        let mut battle = sample_battle()?;
        drive_opponent(&mut battle)?;
        self.controls = TacticalControls::new(&battle, BattleSide::Attacker);
        self.battle = battle;
        self.last_frame_ms = None;
        self.tick_accumulator = 0.0;
        self.last_opponent_plan_tick = 0;
        self.paused = false;
        self.render()
    }

    fn frame(&mut self, timestamp_ms: f64) -> Result<(), String> {
        if !timestamp_ms.is_finite() {
            return Err("animation timestamp must be finite".to_owned());
        }

        let previous = self.last_frame_ms.replace(timestamp_ms);
        if !self.paused {
            if let Some(previous) = previous {
                let elapsed_ms = (timestamp_ms - previous).clamp(0.0, MAX_FRAME_DELTA_MS);
                self.tick_accumulator +=
                    elapsed_ms * f64::from(TACTICAL_TICKS_PER_SECOND) / 1_000.0;
                let pending_ticks = (self.tick_accumulator.floor() as u32).min(MAX_TICKS_PER_FRAME);
                if pending_ticks > 0 {
                    self.tick_accumulator -= f64::from(pending_ticks);
                    self.battle.advance_ticks(pending_ticks);
                    if self
                        .battle
                        .tick()
                        .saturating_sub(self.last_opponent_plan_tick)
                        >= OPPONENT_REPLAN_TICKS
                    {
                        drive_opponent(&mut self.battle)?;
                        self.last_opponent_plan_tick = self.battle.tick();
                    }
                    if self.outcome().is_some() {
                        self.paused = true;
                    }
                }
            }
        }

        self.render()
    }

    fn apply_control(&mut self, request: TacticalControlRequest) -> Result<(), String> {
        self.controls
            .apply_request(&mut self.battle, request)
            .map_err(|error| error.to_string())?;
        self.render()
    }

    fn pointer(
        &mut self,
        button: u16,
        x_px: f64,
        y_px: f64,
        width_px: f64,
        height_px: f64,
        shift: bool,
    ) -> Result<(), String> {
        validate_viewport(x_px, y_px, width_px, height_px)?;
        let snapshot = self.snapshot();
        let picked = pick_unit(&snapshot, x_px, y_px, width_px, height_px);
        let selected = snapshot.units.iter().any(|unit| unit.selected);

        match button {
            0 => {
                let selectable = picked.as_deref().and_then(|unit_id| {
                    self.battle.units().iter().find(|unit| {
                        unit.id() == unit_id
                            && unit.side() == BattleSide::Attacker
                            && !unit.is_routed()
                            && !unit.is_destroyed()
                    })
                });
                let request = if let Some(unit) = selectable {
                    TacticalControlRequest {
                        kind: if shift { "selectToggle" } else { "selectReplace" }.to_owned(),
                        unit_ids: Some(vec![unit.id().to_owned()]),
                        ..TacticalControlRequest::default()
                    }
                } else {
                    TacticalControlRequest {
                        kind: "clearSelection".to_owned(),
                        ..TacticalControlRequest::default()
                    }
                };
                self.apply_control(request)?;
            }
            2 if selected => {
                let enemy = picked.as_deref().and_then(|unit_id| {
                    self.battle.units().iter().find(|unit| {
                        unit.id() == unit_id
                            && unit.side() == BattleSide::Defender
                            && !unit.is_routed()
                            && !unit.is_destroyed()
                    })
                });
                let request = if let Some(unit) = enemy {
                    TacticalControlRequest {
                        kind: "engageSelected".to_owned(),
                        target_unit_id: Some(unit.id().to_owned()),
                        ..TacticalControlRequest::default()
                    }
                } else if let Some(point) = battlefield_point(
                    &snapshot,
                    x_px,
                    y_px,
                    width_px,
                    height_px,
                ) {
                    TacticalControlRequest {
                        kind: "moveSelected".to_owned(),
                        x_mm: Some(point.x_mm),
                        y_mm: Some(point.y_mm),
                        ..TacticalControlRequest::default()
                    }
                } else {
                    return Ok(());
                };
                self.apply_control(request)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.last_frame_ms = None;
    }

    fn snapshot(&self) -> BattleRenderSnapshot {
        BattleRenderSnapshot::project(&self.battle, &self.controls.render_view(&self.battle))
    }

    fn outcome(&self) -> Option<&'static str> {
        let attacker_live = self.battle.units().iter().any(|unit| {
            unit.side() == BattleSide::Attacker && !unit.is_routed() && !unit.is_destroyed()
        });
        let defender_live = self.battle.units().iter().any(|unit| {
            unit.side() == BattleSide::Defender && !unit.is_routed() && !unit.is_destroyed()
        });
        match (attacker_live, defender_live) {
            (true, true) => None,
            (true, false) => Some("playerVictory"),
            (false, true) => Some("playerDefeat"),
            (false, false) => Some("draw"),
        }
    }

    fn status_json(&self) -> Result<String, String> {
        let snapshot = self.snapshot();
        let selected = snapshot
            .units
            .iter()
            .filter(|unit| unit.selected)
            .map(|unit| unit.unit_id.as_str())
            .collect::<BTreeSet<_>>();
        let units = self
            .battle
            .units()
            .iter()
            .map(|unit| UnitStatus {
                id: unit.id().to_owned(),
                side: side_name(unit.side()),
                soldiers: unit.soldiers(),
                morale: unit.morale(),
                fatigue: unit.fatigue(),
                routed: unit.is_routed(),
                destroyed: unit.is_destroyed(),
                selected: selected.contains(unit.id()),
                engagement_target: unit.engagement_target().map(str::to_owned),
                x_mm: unit.position().x_mm,
                y_mm: unit.position().y_mm,
            })
            .collect();
        serde_json::to_string(&SandboxStatus {
            tick: self.battle.tick(),
            paused: self.paused,
            outcome: self.outcome(),
            selected_units: selected.into_iter().map(str::to_owned).collect(),
            units,
        })
        .map_err(|error| error.to_string())
    }

    fn render(&mut self) -> Result<(), String> {
        let width = self.canvas.width().max(1);
        let height = self.canvas.height().max(1);
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
        }

        let snapshot = self.snapshot();
        self.renderer
            .upload_snapshot(&self.device, &self.queue, &snapshot);
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => {
                self.present(frame);
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.present(frame);
                self.surface.configure(&self.device, &self.config);
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                Err("WebGPU rejected the tactical canvas frame as invalid".to_owned())
            }
        }
    }

    fn present(&self, frame: wgpu::SurfaceTexture) {
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Medieval browser tactical frame encoder"),
            });
        self.renderer.render(&mut encoder, &view, CLEAR_COLOR);
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }
}

fn sample_battle() -> Result<TacticalBattle, String> {
    let battlefield = FlatBattlefield::new(100_000, 100_000);
    TacticalBattle::new(
        battlefield,
        vec![
            unit(
                "attacker-spears",
                BattleSide::Attacker,
                110,
                22_000,
                24_000,
                Formation::Line { files: 28 },
                450,
            ),
            unit(
                "attacker-archers",
                BattleSide::Attacker,
                80,
                18_000,
                50_000,
                Formation::Line { files: 24 },
                420,
            ),
            unit(
                "attacker-knights",
                BattleSide::Attacker,
                44,
                22_000,
                76_000,
                Formation::Column { files: 12 },
                850,
            ),
            unit(
                "defender-spears",
                BattleSide::Defender,
                110,
                78_000,
                24_000,
                Formation::Line { files: 28 },
                430,
            ),
            unit(
                "defender-archers",
                BattleSide::Defender,
                80,
                82_000,
                50_000,
                Formation::Line { files: 24 },
                400,
            ),
            unit(
                "defender-knights",
                BattleSide::Defender,
                44,
                78_000,
                76_000,
                Formation::Column { files: 12 },
                800,
            ),
        ],
    )
    .map_err(|error| error.to_string())
}

fn unit(
    id: &str,
    side: BattleSide,
    soldiers: u16,
    x_mm: u32,
    y_mm: u32,
    formation: Formation,
    speed_mm_per_tick: u32,
) -> TacticalUnit {
    TacticalUnit::new(
        id,
        side,
        soldiers,
        BattlePoint::new(x_mm, y_mm),
        formation,
        speed_mm_per_tick,
    )
}

fn drive_opponent(battle: &mut TacticalBattle) -> Result<(), String> {
    let attackers = battle
        .units()
        .iter()
        .filter(|unit| {
            unit.side() == BattleSide::Attacker && !unit.is_routed() && !unit.is_destroyed()
        })
        .map(|unit| (unit.id().to_owned(), unit.position()))
        .collect::<Vec<_>>();
    if attackers.is_empty() {
        return Ok(());
    }

    let defenders = battle
        .units()
        .iter()
        .filter(|unit| {
            unit.side() == BattleSide::Defender && !unit.is_routed() && !unit.is_destroyed()
        })
        .map(|unit| (unit.id().to_owned(), unit.position(), unit.engagement_target().map(str::to_owned)))
        .collect::<Vec<_>>();

    let mut assignments = Vec::with_capacity(defenders.len());
    for (defender_id, defender_position, current_target) in defenders {
        let target = attackers
            .iter()
            .min_by(|left, right| {
                let left_distance = distance_squared(defender_position, left.1);
                let right_distance = distance_squared(defender_position, right.1);
                left_distance
                    .cmp(&right_distance)
                    .then_with(|| left.0.cmp(&right.0))
            })
            .map(|candidate| candidate.0.clone())
            .expect("non-empty attacker set has a nearest unit");
        if current_target.as_deref() != Some(target.as_str()) {
            assignments.push((defender_id, target));
        }
    }

    for (defender_id, target_id) in assignments {
        battle
            .issue_engagement_order(&defender_id, &target_id)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn distance_squared(left: BattlePoint, right: BattlePoint) -> u64 {
    let dx = i64::from(left.x_mm) - i64::from(right.x_mm);
    let dy = i64::from(left.y_mm) - i64::from(right.y_mm);
    u64::try_from(dx * dx + dy * dy).expect("battlefield distance is non-negative")
}

fn validate_viewport(x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    if [x, y, width, height].iter().any(|value| !value.is_finite())
        || width <= 0.0
        || height <= 0.0
    {
        return Err("pointer viewport must contain finite positive dimensions".to_owned());
    }
    Ok(())
}

fn sanitized_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() && zoom > 0.0 {
        zoom.max(MIN_CAMERA_ZOOM)
    } else {
        1.0
    }
}

fn projected_pixel(
    snapshot: &BattleRenderSnapshot,
    position: BattlePoint,
    width_px: f64,
    height_px: f64,
) -> (f64, f64) {
    let half_width = f64::from(snapshot.battlefield.width_mm) / 2.0;
    let half_depth = f64::from(snapshot.battlefield.depth_mm) / 2.0;
    let zoom = f64::from(sanitized_zoom(snapshot.camera.zoom));
    let clip_x = (f64::from(position.x_mm) - f64::from(snapshot.camera.center_x_mm))
        / half_width
        * zoom;
    let clip_y = (f64::from(snapshot.camera.center_y_mm) - f64::from(position.y_mm))
        / half_depth
        * zoom;
    ((clip_x + 1.0) * width_px / 2.0, (1.0 - clip_y) * height_px / 2.0)
}

fn pick_unit(
    snapshot: &BattleRenderSnapshot,
    x_px: f64,
    y_px: f64,
    width_px: f64,
    height_px: f64,
) -> Option<String> {
    snapshot
        .units
        .iter()
        .filter(|unit| !unit.routed)
        .filter_map(|unit| {
            let (unit_x, unit_y) = projected_pixel(snapshot, unit.position, width_px, height_px);
            let dx = unit_x - x_px;
            let dy = unit_y - y_px;
            let distance = dx * dx + dy * dy;
            (distance <= PICK_RADIUS_PX * PICK_RADIUS_PX)
                .then_some((distance, unit.unit_id.as_str()))
        })
        .min_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.1.cmp(right.1))
        })
        .map(|(_, unit_id)| unit_id.to_owned())
}

fn battlefield_point(
    snapshot: &BattleRenderSnapshot,
    x_px: f64,
    y_px: f64,
    width_px: f64,
    height_px: f64,
) -> Option<BattlePoint> {
    let clip_x = x_px / width_px * 2.0 - 1.0;
    let clip_y = 1.0 - y_px / height_px * 2.0;
    let zoom = f64::from(sanitized_zoom(snapshot.camera.zoom));
    let half_width = f64::from(snapshot.battlefield.width_mm) / 2.0;
    let half_depth = f64::from(snapshot.battlefield.depth_mm) / 2.0;
    let x = f64::from(snapshot.camera.center_x_mm) + clip_x * half_width / zoom;
    let y = f64::from(snapshot.camera.center_y_mm) - clip_y * half_depth / zoom;
    if x < 0.0
        || y < 0.0
        || x > f64::from(snapshot.battlefield.width_mm)
        || y > f64::from(snapshot.battlefield.depth_mm)
    {
        return None;
    }
    Some(BattlePoint::new(x.round() as u32, y.round() as u32))
}

const fn side_name(side: BattleSide) -> &'static str {
    match side {
        BattleSide::Attacker => "player",
        BattleSide::Defender => "opponent",
    }
}

fn status_result(sandbox: &BrowserSandbox) -> Result<String, JsValue> {
    sandbox.status_json().map_err(js_error)
}

fn with_sandbox<T>(
    operation: impl FnOnce(&mut BrowserSandbox) -> Result<T, String>,
) -> Result<T, JsValue> {
    SANDBOX.with(|slot| {
        let mut slot = slot.borrow_mut();
        let sandbox = slot
            .as_mut()
            .ok_or_else(|| js_error("battle sandbox has not been initialized"))?;
        operation(sandbox).map_err(js_error)
    })
}

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[wasm_bindgen]
pub async fn battle_sandbox_start(canvas_id: String) -> Result<String, JsValue> {
    let window = web_sys::window().ok_or_else(|| js_error("browser window is unavailable"))?;
    let document = window
        .document()
        .ok_or_else(|| js_error("browser document is unavailable"))?;
    let canvas = document
        .get_element_by_id(&canvas_id)
        .ok_or_else(|| js_error(format!("battle canvas #{canvas_id} does not exist")))?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| js_error(format!("element #{canvas_id} is not a canvas")))?;

    let sandbox = BrowserSandbox::new(canvas).await.map_err(js_error)?;
    let status = sandbox.status_json().map_err(js_error)?;
    SANDBOX.with(|slot| *slot.borrow_mut() = Some(sandbox));
    Ok(status)
}

#[wasm_bindgen]
pub fn battle_sandbox_frame(timestamp_ms: f64) -> Result<(), JsValue> {
    with_sandbox(|sandbox| sandbox.frame(timestamp_ms))
}

#[wasm_bindgen]
pub fn battle_sandbox_pointer(
    button: u16,
    x_px: f64,
    y_px: f64,
    width_px: f64,
    height_px: f64,
    shift: bool,
) -> Result<String, JsValue> {
    with_sandbox(|sandbox| {
        sandbox.pointer(button, x_px, y_px, width_px, height_px, shift)?;
        sandbox.status_json()
    })
}

#[wasm_bindgen]
pub fn battle_sandbox_control(request_json: &str) -> Result<String, JsValue> {
    let request: TacticalControlRequest = serde_json::from_str(request_json)
        .map_err(|error| js_error(format!("invalid tactical control request: {error}")))?;
    with_sandbox(|sandbox| {
        sandbox.apply_control(request)?;
        sandbox.status_json()
    })
}

#[wasm_bindgen]
pub fn battle_sandbox_status() -> Result<String, JsValue> {
    SANDBOX.with(|slot| {
        let slot = slot.borrow();
        let sandbox = slot
            .as_ref()
            .ok_or_else(|| js_error("battle sandbox has not been initialized"))?;
        status_result(sandbox)
    })
}

#[wasm_bindgen]
pub fn battle_sandbox_reset() -> Result<String, JsValue> {
    with_sandbox(|sandbox| {
        sandbox.reset()?;
        sandbox.status_json()
    })
}

#[wasm_bindgen]
pub fn battle_sandbox_set_paused(paused: bool) -> Result<String, JsValue> {
    with_sandbox(|sandbox| {
        sandbox.set_paused(paused);
        sandbox.status_json()
    })
}

#[wasm_bindgen]
pub fn battle_sandbox_pan(direction: &str) -> Result<String, JsValue> {
    let (delta_x_mm, delta_y_mm) = match direction {
        "left" => (-CAMERA_PAN_MM, 0.0),
        "right" => (CAMERA_PAN_MM, 0.0),
        "up" => (0.0, -CAMERA_PAN_MM),
        "down" => (0.0, CAMERA_PAN_MM),
        _ => return Err(js_error(format!("unknown camera direction {direction}"))),
    };
    battle_sandbox_control(
        &serde_json::json!({
            "kind": "panCamera",
            "deltaXMm": delta_x_mm,
            "deltaYMm": delta_y_mm,
        })
        .to_string(),
    )
}

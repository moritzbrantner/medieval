use std::{cell::RefCell, cmp::Ordering, collections::BTreeSet};

use medieval_core::{
    BattlePoint, BattleSide, DeploymentZone, FlatBattlefield, Formation,
    TACTICAL_TICKS_PER_SECOND, TacticalBattle, TacticalGroundCover, TacticalTerrainCell,
    TacticalTerrainProfile, TacticalUnit, UnitKind,
};
use medieval_renderer::{BattleRenderSnapshot, GpuBattleRenderer};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::HtmlCanvasElement;

mod controls;
use controls::{TacticalControlRequest, TacticalControls};

const MAX_FRAME_DELTA_MS: f64 = 250.0;
const MAX_TICKS_PER_FRAME: u32 = 5;
const OPPONENT_REPLAN_TICKS: u64 = TACTICAL_TICKS_PER_SECOND as u64;
const PICK_PADDING_PX: f64 = 14.0;
const PICK_FALLBACK_RADIUS_PX: f64 = 44.0;
const ARCHER_RANGE_MM: u32 = 25_000;
const CAMERA_PAN_MM: f32 = 5_000.0;
const SANDBOX_ARMY_BUDGET: u32 = 1_500;
const MAX_SANDBOX_BATTALIONS: u32 = 12;
const PLAYER_DEPLOYMENT_X_MM: u32 = 22_000;
const PLAYER_DEPLOYMENT_FIRST_Y_MM: u32 = 10_000;
const PLAYER_DEPLOYMENT_ROW_SPACING_MM: u32 = 7_000;
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.055,
    g: 0.047,
    b: 0.035,
    a: 1.0,
};

thread_local! {
    static SANDBOX: RefCell<Option<BrowserSandbox>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SandboxArmySelection {
    levy: u16,
    spearmen: u16,
    archers: u16,
    knights: u16,
}

impl SandboxArmySelection {
    fn entries(self) -> [(UnitKind, u16); 4] {
        [
            (UnitKind::Levy, self.levy),
            (UnitKind::Spearmen, self.spearmen),
            (UnitKind::Archers, self.archers),
            (UnitKind::Knights, self.knights),
        ]
    }

    fn battalion_count(self) -> u32 {
        self.entries()
            .into_iter()
            .map(|(_, count)| u32::from(count))
            .sum()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArmySetupUnit {
    unit: UnitKind,
    label: &'static str,
    cost: u32,
    selected: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArmySetupQuote {
    budget: u32,
    spent: u32,
    remaining: u32,
    battalions: u32,
    max_battalions: u32,
    can_start: bool,
    reason: Option<&'static str>,
    units: Vec<ArmySetupUnit>,
}

fn army_setup_quote(selection: SandboxArmySelection) -> ArmySetupQuote {
    let battalions = selection.battalion_count();
    let mut spent = 0_u32;
    let mut units = Vec::with_capacity(4);
    for (unit, selected) in selection.entries() {
        let cost = unit.recruitment_cost();
        spent = spent.saturating_add(cost.saturating_mul(u32::from(selected)));
        units.push(ArmySetupUnit {
            unit,
            label: unit.label(),
            cost,
            selected,
        });
    }
    let reason = if battalions == 0 {
        Some("Choose at least one battalion.")
    } else if battalions > MAX_SANDBOX_BATTALIONS {
        Some("The field command can coordinate at most 12 battalions.")
    } else if spent > SANDBOX_ARMY_BUDGET {
        Some("This army exceeds the available muster budget.")
    } else {
        None
    };
    ArmySetupQuote {
        budget: SANDBOX_ARMY_BUDGET,
        spent,
        remaining: SANDBOX_ARMY_BUDGET.saturating_sub(spent),
        battalions,
        max_battalions: MAX_SANDBOX_BATTALIONS,
        can_start: reason.is_none(),
        reason,
        units,
    }
}

fn validate_army_selection(selection: SandboxArmySelection) -> Result<(), String> {
    let quote = army_setup_quote(selection);
    if quote.can_start {
        Ok(())
    } else {
        Err(quote
            .reason
            .unwrap_or("the selected army is not valid")
            .to_owned())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SandboxStatus {
    tick: u64,
    paused: bool,
    outcome: Option<&'static str>,
    selected_units: Vec<String>,
    deployment_zones: [DeploymentZone; 2],
    terrain_profile: TacticalTerrainProfile,
    forest_cells: Vec<TacticalTerrainCell>,
    river_cells: Vec<TacticalTerrainCell>,
    river_crossing_cells: Vec<TacticalTerrainCell>,
    siege: Option<serde_json::Value>,
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
struct UnitStatus {
    id: String,
    side: &'static str,
    soldiers: u16,
    morale: u16,
    fatigue: u16,
    formation: &'static str,
    formation_files: u16,
    attack_range_mm: u32,
    routed: bool,
    destroyed: bool,
    selected: bool,
    engagement_target: Option<String>,
    destination: Option<BattlePoint>,
    x_mm: u32,
    y_mm: u32,
    terrain_elevation_mm: u32,
    ground_cover: TacticalGroundCover,
    ranged_target_damage_factor_milli: u32,
    engagement_elevation_damage_factor_milli: Option<u32>,
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
    initial_army: SandboxArmySelection,
}

impl BrowserSandbox {
    async fn new(
        canvas: HtmlCanvasElement,
        initial_army: SandboxArmySelection,
    ) -> Result<Self, String> {
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

        let mut battle = sample_battle(initial_army)?;
        drive_opponent(&mut battle)?;
        let controls = TacticalControls::new(&battle, BattleSide::Attacker);
        let snapshot = BattleRenderSnapshot::capture(&battle, &controls.render_view(&battle));
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
            initial_army,
        };
        sandbox.render()?;
        Ok(sandbox)
    }

    fn reset(&mut self) -> Result<(), String> {
        let mut battle = sample_battle(self.initial_army)?;
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

    fn set_selected_formation(&mut self, kind: &str) -> Result<(), String> {
        let line = match kind {
            "line" => true,
            "column" => false,
            _ => return Err(format!("unknown formation {kind}")),
        };
        let selected = self
            .snapshot()
            .units
            .into_iter()
            .filter(|unit| unit.selected)
            .map(|unit| unit.unit_id)
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Err("no tactical units are selected".to_owned());
        }

        let mut next = self.battle.clone();
        for unit_id in selected {
            let files = next
                .units()
                .iter()
                .find(|unit| unit.id() == unit_id)
                .map(|unit| match unit.formation() {
                    Formation::Line { files } | Formation::Column { files } => files,
                })
                .ok_or_else(|| format!("selected tactical unit {unit_id} no longer exists"))?;
            let formation = if line {
                Formation::Line { files }
            } else {
                Formation::Column { files }
            };
            next.issue_formation_order(&unit_id, formation)
                .map_err(|error| error.to_string())?;
        }
        self.battle = next;
        self.render()
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.last_frame_ms = None;
    }

    fn snapshot(&self) -> BattleRenderSnapshot {
        BattleRenderSnapshot::capture(&self.battle, &self.controls.render_view(&self.battle))
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
        let terrain = self.battle.terrain();
        let battlefield = self.battle.battlefield();
        let units = self
            .battle
            .units()
            .iter()
            .map(|unit| {
                let (formation, formation_files) = formation_status(unit.formation());
                let position = unit.position();
                let engagement_elevation_damage_factor_milli = unit
                    .engagement_target()
                    .and_then(|target_id| {
                        self.battle
                            .units()
                            .iter()
                            .find(|target| target.id() == target_id)
                    })
                    .map(|target| {
                        terrain.elevation_damage_factor_milli(
                            battlefield,
                            position,
                            target.position(),
                        )
                    });
                UnitStatus {
                    id: unit.id().to_owned(),
                    side: side_name(unit.side()),
                    soldiers: unit.soldiers(),
                    morale: unit.morale(),
                    fatigue: unit.fatigue(),
                    formation,
                    formation_files,
                    attack_range_mm: unit.attack_range_mm(),
                    routed: unit.is_routed(),
                    destroyed: unit.is_destroyed(),
                    selected: selected.contains(unit.id()),
                    engagement_target: unit.engagement_target().map(str::to_owned),
                    destination: unit.destination(),
                    x_mm: position.x_mm,
                    y_mm: position.y_mm,
                    terrain_elevation_mm: terrain.height_mm(battlefield, position),
                    ground_cover: terrain.ground_cover_at(battlefield, position),
                    ranged_target_damage_factor_milli: terrain
                        .ranged_target_damage_factor_milli(battlefield, position),
                    engagement_elevation_damage_factor_milli,
                }
            })
            .collect();
        let siege = self
            .battle
            .siege_snapshot()
            .map(|siege| serde_json::to_value(siege).expect("core siege snapshot is serializable"));
        serde_json::to_string(&SandboxStatus {
            tick: self.battle.tick(),
            paused: self.paused,
            outcome: self.outcome(),
            selected_units: selected.into_iter().map(str::to_owned).collect(),
            deployment_zones: self.battle.deployment_zones(),
            terrain_profile: terrain.profile(),
            forest_cells: terrain.forest_cells().to_vec(),
            river_cells: terrain.river_cells().to_vec(),
            river_crossing_cells: terrain.river_crossing_cells().to_vec(),
            siege,
            camera: CameraStatus {
                target_x_mm: snapshot.camera.target_x_mm(),
                target_z_mm: snapshot.camera.target_z_mm(),
                distance_mm: snapshot.camera.distance_mm(),
            },
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

fn sample_battle(selection: SandboxArmySelection) -> Result<TacticalBattle, String> {
    validate_army_selection(selection)?;
    let mut units = player_units(selection);
    units.extend(opponent_units());
    TacticalBattle::deploy_siege(FlatBattlefield::new(100_000, 100_000), units)
        .map_err(|error| error.to_string())
}

fn player_units(selection: SandboxArmySelection) -> Vec<TacticalUnit> {
    let mut units = Vec::new();
    let mut deployment_slot = 0_u16;
    for (kind, count) in selection.entries() {
        for index in 0..count {
            let y_mm = PLAYER_DEPLOYMENT_FIRST_Y_MM
                + u32::from(deployment_slot) * PLAYER_DEPLOYMENT_ROW_SPACING_MM;
            if index == 0 {
                if let Some(unit) = canonical_player_unit(kind, y_mm) {
                    units.push(unit);
                    deployment_slot = deployment_slot.saturating_add(1);
                    continue;
                }
            }
            units.push(extra_player_unit(kind, index, y_mm));
            deployment_slot = deployment_slot.saturating_add(1);
        }
    }
    units
}

fn canonical_player_unit(kind: UnitKind, y_mm: u32) -> Option<TacticalUnit> {
    match kind {
        UnitKind::Levy => None,
        UnitKind::Spearmen => Some(unit(
            "attacker-spears",
            UnitKind::Spearmen,
            BattleSide::Attacker,
            110,
            PLAYER_DEPLOYMENT_X_MM,
            y_mm,
            Formation::Line { files: 28 },
            450,
        )),
        UnitKind::Archers => Some(
            unit(
                "attacker-archers",
                UnitKind::Archers,
                BattleSide::Attacker,
                80,
                PLAYER_DEPLOYMENT_X_MM,
                y_mm,
                Formation::Line { files: 24 },
                420,
            )
            .with_attack_range_mm(ARCHER_RANGE_MM),
        ),
        UnitKind::Knights => Some(unit(
            "attacker-knights",
            UnitKind::Knights,
            BattleSide::Attacker,
            44,
            PLAYER_DEPLOYMENT_X_MM,
            y_mm,
            Formation::Column { files: 12 },
            850,
        )),
    }
}

fn extra_player_unit(kind: UnitKind, index: u16, y_mm: u32) -> TacticalUnit {
    let slug = match kind {
        UnitKind::Levy => "levy",
        UnitKind::Spearmen => "spears",
        UnitKind::Archers => "archers",
        UnitKind::Knights => "knights",
    };
    let id = if index == 0 {
        format!("attacker-{slug}")
    } else {
        format!("attacker-{slug}-{}", index + 1)
    };
    let (soldiers, formation, speed_mm_per_tick) = match kind {
        UnitKind::Levy => (120, Formation::Line { files: 30 }, 380),
        UnitKind::Spearmen => (110, Formation::Line { files: 28 }, 450),
        UnitKind::Archers => (80, Formation::Line { files: 24 }, 420),
        UnitKind::Knights => (44, Formation::Column { files: 12 }, 850),
    };
    let unit = unit(
        &id,
        kind,
        BattleSide::Attacker,
        soldiers,
        PLAYER_DEPLOYMENT_X_MM,
        y_mm,
        formation,
        speed_mm_per_tick,
    );
    if kind == UnitKind::Archers {
        unit.with_attack_range_mm(ARCHER_RANGE_MM)
    } else {
        unit
    }
}

fn opponent_units() -> Vec<TacticalUnit> {
    vec![
        unit(
            "defender-spears",
            UnitKind::Spearmen,
            BattleSide::Defender,
            110,
            78_000,
            24_000,
            Formation::Line { files: 28 },
            430,
        ),
        unit(
            "defender-archers",
            UnitKind::Archers,
            BattleSide::Defender,
            80,
            82_000,
            50_000,
            Formation::Line { files: 24 },
            400,
        )
        .with_attack_range_mm(ARCHER_RANGE_MM),
        unit(
            "defender-knights",
            UnitKind::Knights,
            BattleSide::Defender,
            44,
            78_000,
            76_000,
            Formation::Column { files: 12 },
            800,
        ),
    ]
}

fn unit(
    id: &str,
    unit_kind: UnitKind,
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
    .with_unit_kind(unit_kind)
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
        .map(|unit| {
            (
                unit.id().to_owned(),
                unit.position(),
                unit.engagement_target().map(str::to_owned),
            )
        })
        .collect::<Vec<_>>();
    let mut assignments = Vec::with_capacity(defenders.len());
    for (defender_id, defender_position, current_target) in defenders {
        let target = attackers
            .iter()
            .min_by(|left, right| {
                distance_squared(defender_position, left.1)
                    .cmp(&distance_squared(defender_position, right.1))
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

fn projected_pixel(
    snapshot: &BattleRenderSnapshot,
    world_position_mm: [f32; 3],
    width_px: f64,
    height_px: f64,
) -> Option<(f64, f64)> {
    let [x, y] = snapshot.camera.project_world_point(
        snapshot.battlefield,
        world_position_mm,
        width_px as f32,
        height_px as f32,
    )?;
    Some((f64::from(x), f64::from(y)))
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
            let (anchor_x, anchor_y) = projected_pixel(
                snapshot,
                unit.interaction_anchor_mm(),
                width_px,
                height_px,
            )?;
            let anchor_dx = anchor_x - x_px;
            let anchor_dy = anchor_y - y_px;
            let anchor_distance = anchor_dx * anchor_dx + anchor_dy * anchor_dy;

            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            let mut nearest_soldier_distance = f64::INFINITY;
            let mut projected_soldiers = 0_u16;
            for center in &unit.soldier_centers_mm {
                let Some((soldier_x, soldier_y)) =
                    projected_pixel(snapshot, *center, width_px, height_px)
                else {
                    continue;
                };
                projected_soldiers = projected_soldiers.saturating_add(1);
                min_x = min_x.min(soldier_x);
                max_x = max_x.max(soldier_x);
                min_y = min_y.min(soldier_y);
                max_y = max_y.max(soldier_y);
                let dx = soldier_x - x_px;
                let dy = soldier_y - y_px;
                nearest_soldier_distance = nearest_soldier_distance.min(dx * dx + dy * dy);
            }

            let inside_formation = projected_soldiers > 0
                && x_px >= min_x - PICK_PADDING_PX
                && x_px <= max_x + PICK_PADDING_PX
                && y_px >= min_y - PICK_PADDING_PX
                && y_px <= max_y + PICK_PADDING_PX;
            let anchor_hit =
                anchor_distance <= PICK_FALLBACK_RADIUS_PX * PICK_FALLBACK_RADIUS_PX;
            if !inside_formation && !anchor_hit {
                return None;
            }
            let distance = if inside_formation {
                nearest_soldier_distance.min(anchor_distance)
            } else {
                anchor_distance
            };
            Some((distance, unit.unit_id.as_str()))
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
    snapshot.camera.ground_point_from_viewport(
        snapshot.battlefield,
        x_px as f32,
        y_px as f32,
        width_px as f32,
        height_px as f32,
    )
}

const fn side_name(side: BattleSide) -> &'static str {
    match side {
        BattleSide::Attacker => "player",
        BattleSide::Defender => "opponent",
    }
}

const fn formation_status(formation: Formation) -> (&'static str, u16) {
    match formation {
        Formation::Line { files } => ("line", files),
        Formation::Column { files } => ("column", files),
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

fn parse_army_selection(selection_json: &str) -> Result<SandboxArmySelection, JsValue> {
    serde_json::from_str(selection_json)
        .map_err(|error| js_error(format!("invalid army selection: {error}")))
}

#[wasm_bindgen]
pub fn battle_sandbox_quote_army(selection_json: &str) -> Result<String, JsValue> {
    let selection = parse_army_selection(selection_json)?;
    serde_json::to_string(&army_setup_quote(selection)).map_err(js_error)
}

#[wasm_bindgen]
pub async fn battle_sandbox_start(
    canvas_id: String,
    selection_json: String,
) -> Result<String, JsValue> {
    let window = web_sys::window().ok_or_else(|| js_error("browser window is unavailable"))?;
    let document = window
        .document()
        .ok_or_else(|| js_error("browser document is unavailable"))?;
    let canvas = document
        .get_element_by_id(&canvas_id)
        .ok_or_else(|| js_error(format!("battle canvas #{canvas_id} does not exist")))?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| js_error(format!("element #{canvas_id} is not a canvas")))?;
    let selection = parse_army_selection(&selection_json)?;
    let sandbox = BrowserSandbox::new(canvas, selection)
        .await
        .map_err(js_error)?;
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
pub fn battle_sandbox_control(request_json: &str) -> Result<String, JsValue> {
    let request: TacticalControlRequest = serde_json::from_str(request_json)
        .map_err(|error| js_error(format!("invalid tactical control request: {error}")))?;
    with_sandbox(|sandbox| {
        sandbox.apply_control(request)?;
        sandbox.status_json()
    })
}

#[wasm_bindgen]
pub fn battle_sandbox_set_formation(kind: &str) -> Result<String, JsValue> {
    with_sandbox(|sandbox| {
        sandbox.set_selected_formation(kind)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maximum_wide_battalions_get_distinct_in_bounds_rows() {
        let units = player_units(SandboxArmySelection {
            levy: MAX_SANDBOX_BATTALIONS as u16,
            spearmen: 0,
            archers: 0,
            knights: 0,
        });

        assert_eq!(units.len(), MAX_SANDBOX_BATTALIONS as usize);
        for (slot, unit) in units.iter().enumerate() {
            assert_eq!(unit.position().x_mm, PLAYER_DEPLOYMENT_X_MM);
            assert_eq!(
                unit.position().y_mm,
                PLAYER_DEPLOYMENT_FIRST_Y_MM
                    + slot as u32 * PLAYER_DEPLOYMENT_ROW_SPACING_MM
            );
            assert!(unit.position().y_mm < 100_000);
        }
        for pair in units.windows(2) {
            assert!(
                pair[1].position().y_mm - pair[0].position().y_mm
                    >= PLAYER_DEPLOYMENT_ROW_SPACING_MM
            );
        }
    }
}

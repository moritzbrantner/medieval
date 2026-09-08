use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use medieval_core::{BattlePoint, BattleSide, MovementOrder, TacticalBattle};
use medieval_renderer::{Camera2d, RenderViewState};
use serde::Deserialize;

const MIN_CAMERA_ZOOM: f32 = 0.05;
const MAX_CAMERA_ZOOM: f32 = 20.0;
const CONTROL_GROUP_COUNT: u8 = 10;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Add,
    Toggle,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TacticalControlRequest {
    pub kind: String,
    pub unit_ids: Option<Vec<String>>,
    pub group: Option<u8>,
    pub additive: Option<bool>,
    pub active: Option<bool>,
    pub delta_x_mm: Option<f32>,
    pub delta_y_mm: Option<f32>,
    pub factor: Option<f32>,
    pub x_mm: Option<u32>,
    pub y_mm: Option<u32>,
    pub target_unit_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum TacticalControlIntent {
    FitCamera,
    PanCamera {
        delta_x_mm: f32,
        delta_y_mm: f32,
    },
    ZoomCamera {
        factor: f32,
    },
    SelectUnits {
        unit_ids: Vec<String>,
        mode: SelectionMode,
    },
    ClearSelection,
    AssignControlGroup {
        group: u8,
    },
    RecallControlGroup {
        group: u8,
        additive: bool,
    },
    SetOrderPreview {
        active: bool,
    },
    MoveSelected {
        destination: BattlePoint,
    },
    EngageSelected {
        target_unit_id: String,
    },
    StopSelected,
}

impl TacticalControlRequest {
    fn into_intent(self) -> Result<TacticalControlIntent, TacticalControlError> {
        let kind = self.kind;
        match kind.as_str() {
            "fitCamera" => Ok(TacticalControlIntent::FitCamera),
            "panCamera" => Ok(TacticalControlIntent::PanCamera {
                delta_x_mm: required(self.delta_x_mm, &kind, "deltaXMm")?,
                delta_y_mm: required(self.delta_y_mm, &kind, "deltaYMm")?,
            }),
            "zoomCamera" => Ok(TacticalControlIntent::ZoomCamera {
                factor: required(self.factor, &kind, "factor")?,
            }),
            "selectReplace" => Ok(TacticalControlIntent::SelectUnits {
                unit_ids: required(self.unit_ids, &kind, "unitIds")?,
                mode: SelectionMode::Replace,
            }),
            "selectAdd" => Ok(TacticalControlIntent::SelectUnits {
                unit_ids: required(self.unit_ids, &kind, "unitIds")?,
                mode: SelectionMode::Add,
            }),
            "selectToggle" => Ok(TacticalControlIntent::SelectUnits {
                unit_ids: required(self.unit_ids, &kind, "unitIds")?,
                mode: SelectionMode::Toggle,
            }),
            "clearSelection" => Ok(TacticalControlIntent::ClearSelection),
            "assignControlGroup" => Ok(TacticalControlIntent::AssignControlGroup {
                group: required(self.group, &kind, "group")?,
            }),
            "recallControlGroup" => Ok(TacticalControlIntent::RecallControlGroup {
                group: required(self.group, &kind, "group")?,
                additive: self.additive.unwrap_or(false),
            }),
            "setOrderPreview" => Ok(TacticalControlIntent::SetOrderPreview {
                active: required(self.active, &kind, "active")?,
            }),
            "moveSelected" => Ok(TacticalControlIntent::MoveSelected {
                destination: BattlePoint::new(
                    required(self.x_mm, &kind, "xMm")?,
                    required(self.y_mm, &kind, "yMm")?,
                ),
            }),
            "engageSelected" => Ok(TacticalControlIntent::EngageSelected {
                target_unit_id: required(self.target_unit_id, &kind, "targetUnitId")?,
            }),
            "stopSelected" => Ok(TacticalControlIntent::StopSelected),
            _ => Err(TacticalControlError::UnknownIntent(kind)),
        }
    }
}

fn required<T>(
    value: Option<T>,
    intent: &str,
    argument: &'static str,
) -> Result<T, TacticalControlError> {
    value.ok_or_else(|| TacticalControlError::MissingArgument {
        intent: intent.to_owned(),
        argument,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TacticalControlError {
    InvalidCameraDelta,
    InvalidZoomFactor,
    InvalidControlGroup(u8),
    UnknownUnit(String),
    UncontrollableUnit(String),
    NoUnitsSelected,
    RuleRejected(String),
    MissingArgument {
        intent: String,
        argument: &'static str,
    },
    UnknownIntent(String),
}

impl fmt::Display for TacticalControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCameraDelta => write!(formatter, "camera pan must be finite"),
            Self::InvalidZoomFactor => {
                write!(formatter, "camera zoom factor must be finite and positive")
            }
            Self::InvalidControlGroup(group) => {
                write!(formatter, "control group {group} must be between 0 and 9")
            }
            Self::UnknownUnit(unit_id) => write!(formatter, "unknown tactical unit {unit_id}"),
            Self::UncontrollableUnit(unit_id) => {
                write!(
                    formatter,
                    "tactical unit {unit_id} is not controllable by this player"
                )
            }
            Self::NoUnitsSelected => write!(formatter, "no tactical units are selected"),
            Self::RuleRejected(error) => {
                write!(formatter, "tactical rules rejected the command: {error}")
            }
            Self::MissingArgument { intent, argument } => {
                write!(formatter, "tactical control {intent} requires {argument}")
            }
            Self::UnknownIntent(intent) => write!(formatter, "unknown tactical control {intent}"),
        }
    }
}

impl std::error::Error for TacticalControlError {}

#[derive(Clone, Debug, PartialEq)]
pub struct TacticalControls {
    player_side: BattleSide,
    camera: Camera2d,
    selected_units: BTreeSet<String>,
    control_groups: BTreeMap<u8, BTreeSet<String>>,
    order_preview: bool,
}

impl TacticalControls {
    #[must_use]
    pub fn new(battle: &TacticalBattle, player_side: BattleSide) -> Self {
        Self {
            player_side,
            camera: Camera2d::fit(battle.battlefield()),
            selected_units: BTreeSet::new(),
            control_groups: BTreeMap::new(),
            order_preview: false,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub const fn camera(&self) -> Camera2d {
        self.camera
    }

    pub fn apply_request(
        &mut self,
        battle: &mut TacticalBattle,
        request: TacticalControlRequest,
    ) -> Result<(), TacticalControlError> {
        self.apply_intent(battle, request.into_intent()?)
    }

    fn apply_intent(
        &mut self,
        battle: &mut TacticalBattle,
        intent: TacticalControlIntent,
    ) -> Result<(), TacticalControlError> {
        match intent {
            TacticalControlIntent::FitCamera => {
                self.fit_camera(battle);
                Ok(())
            }
            TacticalControlIntent::PanCamera {
                delta_x_mm,
                delta_y_mm,
            } => self.pan_camera(battle, delta_x_mm, delta_y_mm),
            TacticalControlIntent::ZoomCamera { factor } => self.zoom_camera(factor),
            TacticalControlIntent::SelectUnits { unit_ids, mode } => {
                self.select_units(battle, unit_ids, mode)
            }
            TacticalControlIntent::ClearSelection => {
                self.clear_selection();
                Ok(())
            }
            TacticalControlIntent::AssignControlGroup { group } => {
                self.assign_control_group(group)
            }
            TacticalControlIntent::RecallControlGroup { group, additive } => {
                self.recall_control_group(battle, group, additive)
            }
            TacticalControlIntent::SetOrderPreview { active } => {
                self.set_order_preview(active);
                Ok(())
            }
            TacticalControlIntent::MoveSelected { destination } => {
                self.move_selected(battle, destination)
            }
            TacticalControlIntent::EngageSelected { target_unit_id } => {
                self.engage_selected(battle, &target_unit_id)
            }
            TacticalControlIntent::StopSelected => self.stop_selected(battle),
        }
    }

    fn fit_camera(&mut self, battle: &TacticalBattle) {
        self.camera = Camera2d::fit(battle.battlefield());
    }

    fn pan_camera(
        &mut self,
        battle: &TacticalBattle,
        delta_x_mm: f32,
        delta_y_mm: f32,
    ) -> Result<(), TacticalControlError> {
        if !delta_x_mm.is_finite() || !delta_y_mm.is_finite() {
            return Err(TacticalControlError::InvalidCameraDelta);
        }

        let battlefield = battle.battlefield();
        self.camera.center_x_mm =
            (self.camera.center_x_mm + delta_x_mm).clamp(0.0, battlefield.width_mm as f32);
        self.camera.center_y_mm =
            (self.camera.center_y_mm + delta_y_mm).clamp(0.0, battlefield.depth_mm as f32);
        Ok(())
    }

    fn zoom_camera(&mut self, factor: f32) -> Result<(), TacticalControlError> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(TacticalControlError::InvalidZoomFactor);
        }

        self.camera.zoom = (self.camera.zoom * factor).clamp(MIN_CAMERA_ZOOM, MAX_CAMERA_ZOOM);
        Ok(())
    }

    fn select_units<I, S>(
        &mut self,
        battle: &TacticalBattle,
        unit_ids: I,
        mode: SelectionMode,
    ) -> Result<(), TacticalControlError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let requested = self.validate_selection(battle, unit_ids)?;
        match mode {
            SelectionMode::Replace => self.selected_units = requested,
            SelectionMode::Add => self.selected_units.extend(requested),
            SelectionMode::Toggle => {
                for unit_id in requested {
                    if !self.selected_units.remove(&unit_id) {
                        self.selected_units.insert(unit_id);
                    }
                }
            }
        }
        if self.selected_units.is_empty() {
            self.order_preview = false;
        }
        Ok(())
    }

    fn clear_selection(&mut self) {
        self.selected_units.clear();
        self.order_preview = false;
    }

    fn assign_control_group(&mut self, group: u8) -> Result<(), TacticalControlError> {
        Self::validate_control_group(group)?;
        self.control_groups
            .insert(group, self.selected_units.clone());
        Ok(())
    }

    fn recall_control_group(
        &mut self,
        battle: &TacticalBattle,
        group: u8,
        additive: bool,
    ) -> Result<(), TacticalControlError> {
        Self::validate_control_group(group)?;
        let recalled = self
            .control_groups
            .get(&group)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|unit_id| Self::is_controllable(battle, self.player_side, unit_id))
            .collect::<BTreeSet<_>>();
        self.control_groups.insert(group, recalled.clone());
        if additive {
            self.selected_units.extend(recalled);
        } else {
            self.selected_units = recalled;
        }
        if self.selected_units.is_empty() {
            self.order_preview = false;
        }
        Ok(())
    }

    fn set_order_preview(&mut self, active: bool) {
        self.order_preview = active && !self.selected_units.is_empty();
    }

    fn move_selected(
        &mut self,
        battle: &mut TacticalBattle,
        destination: BattlePoint,
    ) -> Result<(), TacticalControlError> {
        self.sync_with_battle(battle);
        self.ensure_selection()?;

        let mut next = battle.clone();
        for unit_id in &self.selected_units {
            next.issue_move_order(MovementOrder {
                unit_id: unit_id.clone(),
                destination,
            })
            .map_err(|error| TacticalControlError::RuleRejected(error.to_string()))?;
        }
        *battle = next;
        self.order_preview = false;
        Ok(())
    }

    fn engage_selected(
        &mut self,
        battle: &mut TacticalBattle,
        target_unit_id: &str,
    ) -> Result<(), TacticalControlError> {
        self.sync_with_battle(battle);
        self.ensure_selection()?;

        let mut next = battle.clone();
        for unit_id in &self.selected_units {
            next.issue_engagement_order(unit_id, target_unit_id)
                .map_err(|error| TacticalControlError::RuleRejected(error.to_string()))?;
        }
        *battle = next;
        self.order_preview = false;
        Ok(())
    }

    fn stop_selected(
        &mut self,
        battle: &mut TacticalBattle,
    ) -> Result<(), TacticalControlError> {
        self.sync_with_battle(battle);
        self.ensure_selection()?;

        let mut next = battle.clone();
        for unit_id in &self.selected_units {
            let position = next
                .units()
                .iter()
                .find(|unit| unit.id() == unit_id)
                .expect("selected units are synchronized before issuing stop orders")
                .position();
            next.issue_move_order(MovementOrder {
                unit_id: unit_id.clone(),
                destination: position,
            })
            .map_err(|error| TacticalControlError::RuleRejected(error.to_string()))?;
        }
        *battle = next;
        self.order_preview = false;
        Ok(())
    }

    fn sync_with_battle(&mut self, battle: &TacticalBattle) {
        let player_side = self.player_side;
        self.selected_units
            .retain(|unit_id| Self::is_controllable(battle, player_side, unit_id));
        for group in self.control_groups.values_mut() {
            group.retain(|unit_id| Self::is_controllable(battle, player_side, unit_id));
        }
        if self.selected_units.is_empty() {
            self.order_preview = false;
        }
    }

    #[must_use]
    pub fn render_view(&self, battle: &TacticalBattle) -> RenderViewState {
        let mut view = RenderViewState::fit(battle.battlefield());
        view.camera = self.camera;
        for unit_id in &self.selected_units {
            view = view.with_selected(unit_id.clone());
            if self.order_preview {
                view = view.with_order_preview(unit_id.clone());
            }
        }
        view
    }

    #[cfg(test)]
    pub fn selected_unit_ids(&self) -> impl Iterator<Item = &str> {
        self.selected_units.iter().map(String::as_str)
    }

    fn validate_selection<I, S>(
        &self,
        battle: &TacticalBattle,
        unit_ids: I,
    ) -> Result<BTreeSet<String>, TacticalControlError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut requested = BTreeSet::new();
        for unit_id in unit_ids {
            let unit_id = unit_id.as_ref();
            let unit = battle
                .units()
                .iter()
                .find(|unit| unit.id() == unit_id)
                .ok_or_else(|| TacticalControlError::UnknownUnit(unit_id.to_owned()))?;
            if unit.side() != self.player_side || unit.is_destroyed() {
                return Err(TacticalControlError::UncontrollableUnit(unit_id.to_owned()));
            }
            requested.insert(unit_id.to_owned());
        }
        Ok(requested)
    }

    const fn validate_control_group(group: u8) -> Result<(), TacticalControlError> {
        if group < CONTROL_GROUP_COUNT {
            Ok(())
        } else {
            Err(TacticalControlError::InvalidControlGroup(group))
        }
    }

    fn ensure_selection(&self) -> Result<(), TacticalControlError> {
        if self.selected_units.is_empty() {
            Err(TacticalControlError::NoUnitsSelected)
        } else {
            Ok(())
        }
    }

    fn is_controllable(battle: &TacticalBattle, player_side: BattleSide, unit_id: &str) -> bool {
        battle
            .units()
            .iter()
            .any(|unit| unit.id() == unit_id && unit.side() == player_side && !unit.is_destroyed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use medieval_core::{FlatBattlefield, Formation, TacticalUnit};

    fn unit(id: &str, side: BattleSide, x_mm: u32) -> TacticalUnit {
        TacticalUnit::new(
            id,
            side,
            80,
            BattlePoint::new(x_mm, 50_000),
            Formation::Line { files: 20 },
            1_000,
        )
    }

    fn sample_battle() -> TacticalBattle {
        TacticalBattle::new(
            FlatBattlefield::new(100_000, 100_000),
            vec![
                unit("attacker-a", BattleSide::Attacker, 20_000),
                unit("attacker-b", BattleSide::Attacker, 30_000),
                unit("defender", BattleSide::Defender, 70_000),
            ],
        )
        .unwrap()
    }

    #[test]
    fn request_dispatch_supports_selection_and_camera_intents() {
        let mut battle = sample_battle();
        let mut controls = TacticalControls::new(&battle, BattleSide::Attacker);

        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "selectReplace".to_owned(),
                    unit_ids: Some(vec!["attacker-a".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "panCamera".to_owned(),
                    delta_x_mm: Some(5_000.0),
                    delta_y_mm: Some(0.0),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();

        assert_eq!(controls.selected_unit_ids().collect::<Vec<_>>(), vec!["attacker-a"]);
        assert_eq!(controls.camera().center_x_mm, 55_000.0);
    }

    #[test]
    fn request_dispatch_rejects_missing_and_unknown_intents() {
        let mut battle = sample_battle();
        let mut controls = TacticalControls::new(&battle, BattleSide::Attacker);

        assert_eq!(
            controls
                .apply_request(
                    &mut battle,
                    TacticalControlRequest {
                        kind: "zoomCamera".to_owned(),
                        ..TacticalControlRequest::default()
                    },
                )
                .unwrap_err(),
            TacticalControlError::MissingArgument {
                intent: "zoomCamera".to_owned(),
                argument: "factor",
            }
        );
        assert_eq!(
            controls
                .apply_request(
                    &mut battle,
                    TacticalControlRequest {
                        kind: "teleport".to_owned(),
                        ..TacticalControlRequest::default()
                    },
                )
                .unwrap_err(),
            TacticalControlError::UnknownIntent("teleport".to_owned())
        );
    }

    #[test]
    fn selection_modes_are_deterministic_and_invalid_selection_is_atomic() {
        let mut battle = sample_battle();
        let mut controls = TacticalControls::new(&battle, BattleSide::Attacker);

        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "selectReplace".to_owned(),
                    unit_ids: Some(vec!["attacker-a".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "selectAdd".to_owned(),
                    unit_ids: Some(vec!["attacker-b".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        assert_eq!(
            controls.selected_unit_ids().collect::<Vec<_>>(),
            vec!["attacker-a", "attacker-b"]
        );

        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "selectToggle".to_owned(),
                    unit_ids: Some(vec!["attacker-a".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        assert_eq!(
            controls.selected_unit_ids().collect::<Vec<_>>(),
            vec!["attacker-b"]
        );

        let before = controls.clone();
        assert_eq!(
            controls
                .apply_request(
                    &mut battle,
                    TacticalControlRequest {
                        kind: "selectAdd".to_owned(),
                        unit_ids: Some(vec!["defender".to_owned()]),
                        ..TacticalControlRequest::default()
                    },
                )
                .unwrap_err(),
            TacticalControlError::UncontrollableUnit("defender".to_owned())
        );
        assert_eq!(controls, before);
    }

    #[test]
    fn camera_pan_and_zoom_are_bounded_and_reject_non_finite_input() {
        let mut battle = sample_battle();
        let mut controls = TacticalControls::new(&battle, BattleSide::Attacker);

        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "panCamera".to_owned(),
                    delta_x_mm: Some(1_000_000.0),
                    delta_y_mm: Some(-1_000_000.0),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        assert_eq!(controls.camera().center_x_mm, 100_000.0);
        assert_eq!(controls.camera().center_y_mm, 0.0);

        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "zoomCamera".to_owned(),
                    factor: Some(100.0),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        assert_eq!(controls.camera().zoom, MAX_CAMERA_ZOOM);
        let before = controls.camera();
        assert_eq!(
            controls
                .apply_request(
                    &mut battle,
                    TacticalControlRequest {
                        kind: "zoomCamera".to_owned(),
                        factor: Some(f32::NAN),
                        ..TacticalControlRequest::default()
                    },
                )
                .unwrap_err(),
            TacticalControlError::InvalidZoomFactor
        );
        assert_eq!(controls.camera(), before);
    }

    #[test]
    fn selected_move_orders_are_atomic_and_core_validated() {
        let mut battle = sample_battle();
        let mut controls = TacticalControls::new(&battle, BattleSide::Attacker);
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "selectReplace".to_owned(),
                    unit_ids: Some(vec!["attacker-a".to_owned(), "attacker-b".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();

        let destination = BattlePoint::new(60_000, 60_000);
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "moveSelected".to_owned(),
                    x_mm: Some(destination.x_mm),
                    y_mm: Some(destination.y_mm),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        for unit_id in ["attacker-a", "attacker-b"] {
            let unit = battle
                .units()
                .iter()
                .find(|unit| unit.id() == unit_id)
                .unwrap();
            assert_eq!(unit.destination(), Some(destination));
        }

        let before = battle.clone();
        assert!(
            controls
                .apply_request(
                    &mut battle,
                    TacticalControlRequest {
                        kind: "moveSelected".to_owned(),
                        x_mm: Some(100_001),
                        y_mm: Some(50_000),
                        ..TacticalControlRequest::default()
                    },
                )
                .is_err()
        );
        assert_eq!(battle, before);
    }

    #[test]
    fn engagement_and_stop_commands_apply_to_the_whole_selection() {
        let mut battle = sample_battle();
        let mut controls = TacticalControls::new(&battle, BattleSide::Attacker);
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "selectReplace".to_owned(),
                    unit_ids: Some(vec!["attacker-a".to_owned(), "attacker-b".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();

        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "engageSelected".to_owned(),
                    target_unit_id: Some("defender".to_owned()),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        for unit_id in ["attacker-a", "attacker-b"] {
            let unit = battle
                .units()
                .iter()
                .find(|unit| unit.id() == unit_id)
                .unwrap();
            assert_eq!(unit.engagement_target(), Some("defender"));
        }

        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "stopSelected".to_owned(),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        for unit_id in ["attacker-a", "attacker-b"] {
            let unit = battle
                .units()
                .iter()
                .find(|unit| unit.id() == unit_id)
                .unwrap();
            assert_eq!(unit.engagement_target(), None);
            assert_eq!(unit.destination(), None);
        }
    }

    #[test]
    fn control_groups_recall_sorted_live_units() {
        let mut battle = sample_battle();
        let mut controls = TacticalControls::new(&battle, BattleSide::Attacker);
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "selectReplace".to_owned(),
                    unit_ids: Some(vec!["attacker-b".to_owned(), "attacker-a".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "assignControlGroup".to_owned(),
                    group: Some(1),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "clearSelection".to_owned(),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();

        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "recallControlGroup".to_owned(),
                    group: Some(1),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        assert_eq!(
            controls.selected_unit_ids().collect::<Vec<_>>(),
            vec!["attacker-a", "attacker-b"]
        );
        assert_eq!(
            controls
                .apply_request(
                    &mut battle,
                    TacticalControlRequest {
                        kind: "assignControlGroup".to_owned(),
                        group: Some(10),
                        ..TacticalControlRequest::default()
                    },
                )
                .unwrap_err(),
            TacticalControlError::InvalidControlGroup(10)
        );
    }

    #[test]
    fn render_view_projects_selection_and_order_preview_without_mutating_battle() {
        let mut battle = sample_battle();
        let before = battle.clone();
        let mut controls = TacticalControls::new(&battle, BattleSide::Attacker);
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "selectReplace".to_owned(),
                    unit_ids: Some(vec!["attacker-a".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        controls
            .apply_request(
                &mut battle,
                TacticalControlRequest {
                    kind: "setOrderPreview".to_owned(),
                    active: Some(true),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();

        let snapshot = medieval_renderer::BattleRenderSnapshot::project(
            &battle,
            &controls.render_view(&battle),
        );
        let selected = snapshot
            .units
            .iter()
            .find(|unit| unit.unit_id == "attacker-a")
            .unwrap();

        assert_eq!(battle, before);
        assert!(selected.selected);
        assert!(selected.order_preview);
    }
}

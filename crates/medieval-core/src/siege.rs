use serde::{Deserialize, Serialize};

use crate::{
    deployment::DeploymentZone,
    tactical::{BattlePoint, BattleSide, FlatBattlefield, TacticalUnit},
};

pub const SIEGE_CAPTURE_MAX_PROGRESS: u16 = 1_000;
pub const SIEGE_CAPTURE_PROGRESS_PER_PULSE: u16 = 100;
pub const SIEGE_TOWER_COUNT: usize = 4;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SiegeGateState {
    Closed,
    Open,
    Destroyed,
}

impl SiegeGateState {
    #[must_use]
    pub const fn is_traversable(self) -> bool {
        matches!(self, Self::Open | Self::Destroyed)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiegeArea {
    pub min_x_mm: u32,
    pub max_x_mm: u32,
    pub min_y_mm: u32,
    pub max_y_mm: u32,
}

impl SiegeArea {
    #[must_use]
    pub const fn contains(self, point: BattlePoint) -> bool {
        point.x_mm >= self.min_x_mm
            && point.x_mm <= self.max_x_mm
            && point.y_mm >= self.min_y_mm
            && point.y_mm <= self.max_y_mm
    }

    #[must_use]
    pub const fn center(self) -> BattlePoint {
        BattlePoint::new(
            self.min_x_mm + (self.max_x_mm - self.min_x_mm) / 2,
            self.min_y_mm + (self.max_y_mm - self.min_y_mm) / 2,
        )
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiegeTower {
    pub center: BattlePoint,
    pub radius_mm: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiegeCapturePoint {
    pub center: BattlePoint,
    pub radius_mm: u32,
}

impl SiegeCapturePoint {
    #[must_use]
    pub fn contains(self, point: BattlePoint) -> bool {
        let dx = i64::from(point.x_mm) - i64::from(self.center.x_mm);
        let dy = i64::from(point.y_mm) - i64::from(self.center.y_mm);
        let radius = u128::from(self.radius_mm);
        squared_components(dx, dy) <= radius * radius
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiegeLayout {
    pub wall_segments: [SiegeArea; 2],
    pub gate: SiegeArea,
    pub towers: [SiegeTower; SIEGE_TOWER_COUNT],
    pub capture_point: SiegeCapturePoint,
    pub deployment_zones: [DeploymentZone; 2],
}

impl SiegeLayout {
    /// Deterministic first siege layout. The wall forms one vertical barrier
    /// through the battlefield with a centered gate. Towers remain bounded to
    /// the wall footprint; they are presentation/state anchors in this slice,
    /// not independent destruction or collision systems.
    #[must_use]
    pub fn test_siege(battlefield: FlatBattlefield) -> Self {
        let wall_min_x = scale(battlefield.width_mm, 49, 100);
        let wall_max_x = scale(battlefield.width_mm, 51, 100);
        let gate_min_y = scale(battlefield.depth_mm, 45, 100);
        let gate_max_y = scale(battlefield.depth_mm, 55, 100);
        let gate = SiegeArea {
            min_x_mm: wall_min_x,
            max_x_mm: wall_max_x,
            min_y_mm: gate_min_y,
            max_y_mm: gate_max_y,
        };
        let wall_segments = [
            SiegeArea {
                min_x_mm: wall_min_x,
                max_x_mm: wall_max_x,
                min_y_mm: 0,
                max_y_mm: gate_min_y.saturating_sub(1),
            },
            SiegeArea {
                min_x_mm: wall_min_x,
                max_x_mm: wall_max_x,
                min_y_mm: gate_max_y.saturating_add(1),
                max_y_mm: battlefield.depth_mm,
            },
        ];
        let wall_center_x = wall_min_x + (wall_max_x - wall_min_x) / 2;
        let tower_radius = battlefield.width_mm.min(battlefield.depth_mm) / 50;
        let towers = [15_u32, 35, 65, 85].map(|percent_y| SiegeTower {
            center: BattlePoint::new(wall_center_x, scale(battlefield.depth_mm, percent_y, 100)),
            radius_mm: tower_radius,
        });
        let capture_point = SiegeCapturePoint {
            center: BattlePoint::new(
                scale(battlefield.width_mm, 80, 100),
                scale(battlefield.depth_mm, 50, 100),
            ),
            radius_mm: battlefield.width_mm.min(battlefield.depth_mm) / 20,
        };
        let deployment_zones = [
            DeploymentZone {
                side: BattleSide::Attacker,
                min_x_mm: 0,
                max_x_mm: scale(battlefield.width_mm, 35, 100),
                min_y_mm: 0,
                max_y_mm: battlefield.depth_mm,
            },
            DeploymentZone {
                side: BattleSide::Defender,
                min_x_mm: scale(battlefield.width_mm, 65, 100),
                max_x_mm: battlefield.width_mm,
                min_y_mm: 0,
                max_y_mm: battlefield.depth_mm,
            },
        ];
        Self {
            wall_segments,
            gate,
            towers,
            capture_point,
            deployment_zones,
        }
    }

    #[must_use]
    pub fn wall_contains(self, point: BattlePoint) -> bool {
        self.wall_segments
            .iter()
            .any(|segment| segment.contains(point))
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiegeCaptureState {
    pub capturing_side: Option<BattleSide>,
    pub progress: u16,
    pub captured_by: Option<BattleSide>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiegeBattleState {
    pub layout: SiegeLayout,
    pub gate_state: SiegeGateState,
    pub capture: SiegeCaptureState,
}

impl SiegeBattleState {
    #[must_use]
    pub fn test_siege(battlefield: FlatBattlefield) -> Self {
        Self {
            layout: SiegeLayout::test_siege(battlefield),
            gate_state: SiegeGateState::Closed,
            capture: SiegeCaptureState::default(),
        }
    }

    #[must_use]
    pub fn is_passable_at(self, point: BattlePoint) -> bool {
        if self.layout.wall_contains(point) {
            return false;
        }
        if self.layout.gate.contains(point) {
            return self.gate_state.is_traversable();
        }
        true
    }

    /// Returns a deterministic intermediate target that keeps a crossing path
    /// inside the centered gate corridor. A closed gate still allows a unit to
    /// approach the near gate edge but never returns a waypoint inside it.
    #[must_use]
    pub fn movement_waypoint(self, from: BattlePoint, destination: BattlePoint) -> BattlePoint {
        let gate = self.layout.gate;
        let gate_center = gate.center();
        let from_side = wall_side(from.x_mm, gate.min_x_mm, gate.max_x_mm);
        let destination_side = wall_side(destination.x_mm, gate.min_x_mm, gate.max_x_mm);

        if from_side == WallSide::Inside {
            if !self.gate_state.is_traversable() {
                return from;
            }
            return match destination_side {
                WallSide::Left => BattlePoint::new(gate.min_x_mm.saturating_sub(1), from.y_mm),
                WallSide::Right => BattlePoint::new(gate.max_x_mm.saturating_add(1), from.y_mm),
                WallSide::Inside => destination,
            };
        }

        let crossing_required = matches!(
            (from_side, destination_side),
            (WallSide::Left, WallSide::Right) | (WallSide::Right, WallSide::Left)
        ) || !self.is_passable_at(destination);
        if !crossing_required {
            return destination;
        }

        if !(gate.min_y_mm..=gate.max_y_mm).contains(&from.y_mm) {
            return BattlePoint::new(from.x_mm, gate_center.y_mm);
        }

        let near_gate_edge = match from_side {
            WallSide::Left => gate.min_x_mm.saturating_sub(1),
            WallSide::Right => gate.max_x_mm.saturating_add(1),
            WallSide::Inside => from.x_mm,
        };
        if !self.gate_state.is_traversable() {
            return BattlePoint::new(near_gate_edge, gate_center.y_mm);
        }

        BattlePoint::new(gate_center.x_mm, gate_center.y_mm)
    }

    /// Advances capture once per authoritative tactical combat pulse. Only
    /// formed units count. If both sides are present, progress is left exactly
    /// unchanged; contested capture therefore fails closed.
    pub fn advance_capture(&mut self, units: &[TacticalUnit]) {
        if self.capture.captured_by.is_some() {
            return;
        }
        let mut attacker_present = false;
        let mut defender_present = false;
        for unit in units.iter().filter(|unit| {
            !unit.is_routed()
                && !unit.is_destroyed()
                && self.layout.capture_point.contains(unit.position())
        }) {
            match unit.side() {
                BattleSide::Attacker => attacker_present = true,
                BattleSide::Defender => defender_present = true,
            }
        }

        let side = match (attacker_present, defender_present) {
            (true, false) => Some(BattleSide::Attacker),
            (false, true) => Some(BattleSide::Defender),
            (false, false) | (true, true) => None,
        };
        let Some(side) = side else {
            return;
        };

        if self.capture.capturing_side != Some(side) {
            self.capture.capturing_side = Some(side);
            self.capture.progress = 0;
        }
        self.capture.progress = self
            .capture
            .progress
            .saturating_add(SIEGE_CAPTURE_PROGRESS_PER_PULSE)
            .min(SIEGE_CAPTURE_MAX_PROGRESS);
        if self.capture.progress == SIEGE_CAPTURE_MAX_PROGRESS {
            self.capture.captured_by = Some(side);
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum WallSide {
    Left,
    Inside,
    Right,
}

fn wall_side(x_mm: u32, min_x_mm: u32, max_x_mm: u32) -> WallSide {
    if x_mm < min_x_mm {
        WallSide::Left
    } else if x_mm > max_x_mm {
        WallSide::Right
    } else {
        WallSide::Inside
    }
}

fn scale(span_mm: u32, numerator: u32, denominator: u32) -> u32 {
    ((u64::from(span_mm) * u64::from(numerator)) / u64::from(denominator)) as u32
}

fn squared_components(dx: i64, dy: i64) -> u128 {
    let x = u128::from(dx.unsigned_abs());
    let y = u128::from(dy.unsigned_abs());
    x * x + y * y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tactical::Formation;

    fn unit(id: &str, side: BattleSide, point: BattlePoint) -> TacticalUnit {
        TacticalUnit::new(id, side, 40, point, Formation::Line { files: 10 }, 1_000)
    }

    #[test]
    fn test_siege_layout_is_deterministic_bounded_and_serializable() {
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        let first = SiegeBattleState::test_siege(battlefield);
        let second = SiegeBattleState::test_siege(battlefield);
        assert_eq!(first, second);
        assert_eq!(first.layout.gate.min_x_mm, 49_000);
        assert_eq!(first.layout.gate.max_x_mm, 51_000);
        assert_eq!(first.layout.gate.min_y_mm, 36_000);
        assert_eq!(first.layout.gate.max_y_mm, 44_000);
        assert_eq!(first.layout.towers.len(), SIEGE_TOWER_COUNT);
        assert!(first.layout.towers.iter().all(|tower| {
            tower.center.x_mm <= battlefield.width_mm
                && tower.center.y_mm <= battlefield.depth_mm
                && tower.radius_mm > 0
        }));
        assert_eq!(
            first.layout.capture_point.center,
            BattlePoint::new(80_000, 40_000)
        );

        let encoded = serde_json::to_string(&first).unwrap();
        let decoded: SiegeBattleState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, first);
    }

    #[test]
    fn closed_gate_blocks_the_wall_while_open_and_destroyed_gates_are_traversable() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let closed = SiegeBattleState::test_siege(battlefield);
        let gate = closed.layout.gate.center();
        assert!(!closed.is_passable_at(BattlePoint::new(50_000, 20_000)));
        assert!(!closed.is_passable_at(gate));
        assert!(closed.is_passable_at(BattlePoint::new(40_000, 20_000)));

        let mut open = closed;
        open.gate_state = SiegeGateState::Open;
        assert!(open.is_passable_at(gate));
        let mut destroyed = closed;
        destroyed.gate_state = SiegeGateState::Destroyed;
        assert!(destroyed.is_passable_at(gate));
    }

    #[test]
    fn crossing_path_aligns_enters_and_exits_the_open_gate() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let mut siege = SiegeBattleState::test_siege(battlefield);
        siege.gate_state = SiegeGateState::Open;
        let destination = BattlePoint::new(80_000, 10_000);
        let align = siege.movement_waypoint(BattlePoint::new(20_000, 10_000), destination);
        assert_eq!(align, BattlePoint::new(20_000, 50_000));
        let enter = siege.movement_waypoint(align, destination);
        assert_eq!(enter, BattlePoint::new(50_000, 50_000));
        assert!(siege.is_passable_at(enter));
        let exit = siege.movement_waypoint(enter, destination);
        assert_eq!(exit, BattlePoint::new(51_001, 50_000));
        assert!(siege.is_passable_at(exit));
        assert_eq!(siege.movement_waypoint(exit, destination), destination);
    }

    #[test]
    fn closed_gate_routes_to_near_edge_but_never_into_the_gate() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let siege = SiegeBattleState::test_siege(battlefield);
        let destination = BattlePoint::new(80_000, 50_000);
        let from = BattlePoint::new(20_000, 50_000);
        let waypoint = siege.movement_waypoint(from, destination);
        assert_eq!(waypoint, BattlePoint::new(48_999, 50_000));
        assert!(siege.is_passable_at(waypoint));
        assert!(!siege.is_passable_at(siege.layout.gate.center()));
    }

    #[test]
    fn formed_uncontested_units_capture_deterministically() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let mut siege = SiegeBattleState::test_siege(battlefield);
        let attacker = unit(
            "attacker",
            BattleSide::Attacker,
            siege.layout.capture_point.center,
        );
        for _ in 0..10 {
            siege.advance_capture(std::slice::from_ref(&attacker));
        }
        assert_eq!(siege.capture.progress, SIEGE_CAPTURE_MAX_PROGRESS);
        assert_eq!(siege.capture.captured_by, Some(BattleSide::Attacker));
    }

    #[test]
    fn contested_capture_fails_closed_without_changing_progress() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let mut siege = SiegeBattleState::test_siege(battlefield);
        let point = siege.layout.capture_point.center;
        let attacker = unit("attacker", BattleSide::Attacker, point);
        let defender = unit("defender", BattleSide::Defender, point);
        siege.advance_capture(std::slice::from_ref(&attacker));
        let before = siege.capture;
        siege.advance_capture(&[attacker, defender]);
        assert_eq!(siege.capture, before);
    }
}

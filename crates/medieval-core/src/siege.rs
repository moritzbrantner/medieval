use serde::{Deserialize, Serialize};

use crate::{
    deployment::DeploymentZone,
    tactical::{BattlePoint, BattleSide, FlatBattlefield, TacticalUnit},
};

pub const SIEGE_CAPTURE_MAX_PROGRESS: u16 = 1_000;
pub const SIEGE_CAPTURE_PROGRESS_PER_PULSE: u16 = 100;
pub const SIEGE_TOWER_COUNT: usize = 4;
/// Integrity of a standing gate. Integrity uses the same thousandths scale as
/// capture progress; the gate is destroyed on the pulse it reaches zero.
pub const SIEGE_GATE_MAX_INTEGRITY: u16 = 1_000;
/// Distance from the gate footprint within which an ordered unit strikes it.
pub const SIEGE_GATE_ATTACK_DISTANCE_MM: u32 = crate::tactical::COMBAT_CONTACT_DISTANCE_MM;

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

/// Versioned, core-owned fortification profiles. Each profile is the single
/// source of its wall, gate, tower and capture geometry; renderers only draw
/// the resulting [`SiegeLayout`]. Versions are never changed in place: a new
/// geometry gets a new variant so retained battles and seeds stay exact.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SiegeProfile {
    /// Timber palisade: a thin wall line, a wide gate, and small watchtowers.
    PalisadeV1,
    /// Stone curtain wall: thicker wall, narrower gate, and full towers. This is
    /// the original deterministic siege layout, so battles and seeds recorded
    /// before profiles existed load as stone walls.
    #[default]
    StoneWallsV1,
}

/// Geometry of one profile, as fractions of the battlefield.
struct SiegeProfileGeometry {
    /// Wall band across the battlefield width, in thousandths.
    wall_permille: (u32, u32),
    /// Gate span along the battlefield depth, in thousandths.
    gate_permille: (u32, u32),
    /// Tower radius as a divisor of the battlefield's shorter side.
    tower_radius_divisor: u32,
}

impl SiegeProfile {
    pub const ALL: [Self; 2] = [Self::PalisadeV1, Self::StoneWallsV1];

    /// The fortification level this profile represents (1 = palisade, 2 = stone).
    #[must_use]
    pub const fn fortification_level(self) -> u8 {
        match self {
            Self::PalisadeV1 => 1,
            Self::StoneWallsV1 => 2,
        }
    }

    /// Gate damage one formed attacking unit deals per combat pulse. Like
    /// capture progress it counts units, not soldiers: one unit breaches a
    /// palisade gate in 10 pulses (10 s) and a stone gate in 20 pulses (20 s).
    #[must_use]
    pub const fn gate_damage_per_pulse(self) -> u16 {
        match self {
            Self::PalisadeV1 => 100,
            Self::StoneWallsV1 => 50,
        }
    }

    /// The profile for a campaign fortification level; `None` below a palisade.
    #[must_use]
    pub const fn for_fortification_level(level: u8) -> Option<Self> {
        match level {
            0 => None,
            1 => Some(Self::PalisadeV1),
            _ => Some(Self::StoneWallsV1),
        }
    }

    const fn geometry(self) -> SiegeProfileGeometry {
        match self {
            Self::PalisadeV1 => SiegeProfileGeometry {
                wall_permille: (495, 505),
                gate_permille: (430, 570),
                tower_radius_divisor: 80,
            },
            Self::StoneWallsV1 => SiegeProfileGeometry {
                wall_permille: (490, 510),
                gate_permille: (450, 550),
                tower_radius_divisor: 50,
            },
        }
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
    /// The original deterministic siege layout, kept as the stone-walls profile.
    #[must_use]
    pub fn test_siege(battlefield: FlatBattlefield) -> Self {
        Self::for_profile(battlefield, SiegeProfile::StoneWallsV1)
    }

    /// Deterministic layout of a fortification profile. The wall forms one
    /// vertical barrier through the battlefield with a centered gate. Towers
    /// remain bounded to the wall footprint; they are presentation/state
    /// anchors in this slice, not independent destruction or collision systems.
    #[must_use]
    pub fn for_profile(battlefield: FlatBattlefield, profile: SiegeProfile) -> Self {
        let geometry = profile.geometry();
        let wall_min_x = scale(battlefield.width_mm, geometry.wall_permille.0, 1_000);
        let wall_max_x = scale(battlefield.width_mm, geometry.wall_permille.1, 1_000);
        let gate_min_y = scale(battlefield.depth_mm, geometry.gate_permille.0, 1_000);
        let gate_max_y = scale(battlefield.depth_mm, geometry.gate_permille.1, 1_000);
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
        let tower_radius =
            battlefield.width_mm.min(battlefield.depth_mm) / geometry.tower_radius_divisor;
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
    /// Battles recorded before profiles existed used the stone-walls layout.
    #[serde(default)]
    pub profile: SiegeProfile,
    pub layout: SiegeLayout,
    pub gate_state: SiegeGateState,
    /// Remaining gate integrity out of [`SIEGE_GATE_MAX_INTEGRITY`]. Sieges
    /// recorded before gate assaults existed load with an intact gate.
    #[serde(default = "full_gate_integrity")]
    pub gate_integrity: u16,
    pub capture: SiegeCaptureState,
}

const fn full_gate_integrity() -> u16 {
    SIEGE_GATE_MAX_INTEGRITY
}

impl SiegeBattleState {
    #[must_use]
    pub fn test_siege(battlefield: FlatBattlefield) -> Self {
        Self::for_profile(battlefield, SiegeProfile::StoneWallsV1)
    }

    /// A fresh siege of `profile`: closed gate, no capture progress.
    #[must_use]
    pub fn for_profile(battlefield: FlatBattlefield, profile: SiegeProfile) -> Self {
        Self {
            profile,
            layout: SiegeLayout::for_profile(battlefield, profile),
            gate_state: SiegeGateState::Closed,
            gate_integrity: SIEGE_GATE_MAX_INTEGRITY,
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

    /// Whether `point` is close enough to the gate footprint to strike it.
    #[must_use]
    pub fn within_gate_attack_distance(self, point: BattlePoint) -> bool {
        let gate = self.layout.gate;
        let dx = u128::from(
            gate.min_x_mm
                .saturating_sub(point.x_mm)
                .max(point.x_mm.saturating_sub(gate.max_x_mm)),
        );
        let dy = u128::from(
            gate.min_y_mm
                .saturating_sub(point.y_mm)
                .max(point.y_mm.saturating_sub(gate.max_y_mm)),
        );
        let reach = u128::from(SIEGE_GATE_ATTACK_DISTANCE_MM);
        dx * dx + dy * dy <= reach * reach
    }

    /// Where a unit at `from` stands to strike the gate: just outside the gate
    /// footprint on its own side of the wall, on the gate's center line. The
    /// point is passable whatever the gate state, so the existing siege path
    /// reaches it without crossing the wall.
    #[must_use]
    pub fn gate_attack_position(self, from: BattlePoint) -> BattlePoint {
        let gate = self.layout.gate;
        let center = gate.center();
        let x_mm = if from.x_mm > center.x_mm {
            gate.max_x_mm.saturating_add(1)
        } else {
            gate.min_x_mm.saturating_sub(1)
        };
        BattlePoint::new(x_mm, center.y_mm)
    }

    /// Whether a closed gate is all that separates `from` and `to` across the
    /// wall line, so reaching `to` needs the gate breached or opened.
    #[must_use]
    pub fn closed_gate_separates(self, from: BattlePoint, to: BattlePoint) -> bool {
        let gate = self.layout.gate;
        self.gate_state == SiegeGateState::Closed
            && matches!(
                (
                    wall_side(from.x_mm, gate.min_x_mm, gate.max_x_mm),
                    wall_side(to.x_mm, gate.min_x_mm, gate.max_x_mm)
                ),
                (WallSide::Left, WallSide::Right) | (WallSide::Right, WallSide::Left)
            )
    }

    /// Applies one combat pulse of gate damage. Only formed attacker units
    /// under an explicit gate-attack order and within reach of the gate count;
    /// defenders never damage their own gate. Integrity only falls, and the
    /// gate becomes [`SiegeGateState::Destroyed`] on the pulse it reaches zero.
    /// Returns whether this pulse destroyed the gate.
    pub fn advance_gate_assault(&mut self, units: &[TacticalUnit]) -> bool {
        if self.gate_state != SiegeGateState::Closed {
            return false;
        }
        let attackers = units
            .iter()
            .filter(|unit| {
                unit.side() == BattleSide::Attacker
                    && unit.can_receive_orders()
                    && unit.is_attacking_gate()
                    && self.within_gate_attack_distance(unit.position())
            })
            .count();
        if attackers == 0 {
            return false;
        }
        let damage = u16::try_from(attackers)
            .unwrap_or(u16::MAX)
            .saturating_mul(self.profile.gate_damage_per_pulse());
        self.gate_integrity = self.gate_integrity.saturating_sub(damage);
        if self.gate_integrity == 0 {
            self.gate_state = SiegeGateState::Destroyed;
            return true;
        }
        false
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
            unit.can_receive_orders() && self.layout.capture_point.contains(unit.position())
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
    fn fortification_profiles_have_distinct_documented_geometry() {
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        let palisade = SiegeBattleState::for_profile(battlefield, SiegeProfile::PalisadeV1);
        let stone = SiegeBattleState::for_profile(battlefield, SiegeProfile::StoneWallsV1);
        assert_eq!(stone, SiegeBattleState::test_siege(battlefield));
        assert_eq!(
            (palisade.layout.gate.min_x_mm, palisade.layout.gate.max_x_mm),
            (49_500, 50_500)
        );
        assert_eq!(
            (palisade.layout.gate.min_y_mm, palisade.layout.gate.max_y_mm),
            (34_400, 45_600)
        );
        assert_eq!(palisade.layout.towers[0].radius_mm, 1_000);
        assert_eq!(stone.layout.towers[0].radius_mm, 1_600);
        assert!(
            palisade.layout.gate.max_y_mm - palisade.layout.gate.min_y_mm
                > stone.layout.gate.max_y_mm - stone.layout.gate.min_y_mm
        );
        assert_eq!(palisade.layout.capture_point, stone.layout.capture_point);
        assert_eq!(palisade.gate_state, SiegeGateState::Closed);
        for profile in SiegeProfile::ALL {
            assert_eq!(
                SiegeProfile::for_fortification_level(profile.fortification_level()),
                Some(profile)
            );
        }
        assert_eq!(SiegeProfile::for_fortification_level(0), None);
    }

    #[test]
    fn battles_recorded_before_profiles_load_as_stone_walls() {
        let battlefield = FlatBattlefield::new(100_000, 80_000);
        let mut document = serde_json::to_value(SiegeBattleState::test_siege(battlefield)).unwrap();
        document.as_object_mut().unwrap().remove("profile");
        let legacy: SiegeBattleState = serde_json::from_value(document).unwrap();
        assert_eq!(legacy, SiegeBattleState::test_siege(battlefield));
        assert_eq!(legacy.profile, SiegeProfile::StoneWallsV1);
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
    fn gate_assault_counts_only_ordered_attackers_in_reach_and_uses_profile_damage() {
        let battlefield = FlatBattlefield::new(100_000, 100_000);
        let mut palisade = SiegeBattleState::for_profile(battlefield, SiegeProfile::PalisadeV1);
        let reach = palisade.gate_attack_position(BattlePoint::new(20_000, 10_000));
        assert_eq!(reach, BattlePoint::new(49_499, 50_000));
        assert!(palisade.within_gate_attack_distance(reach));
        assert!(!palisade.within_gate_attack_distance(BattlePoint::new(40_000, 50_000)));

        let ordered = unit("ram", BattleSide::Attacker, reach).with_gate_attack_order();
        let unordered = unit("idle", BattleSide::Attacker, reach);
        let defender = unit("defender", BattleSide::Defender, reach).with_gate_attack_order();
        assert!(!palisade.advance_gate_assault(&[unordered, defender]));
        assert_eq!(palisade.gate_integrity, SIEGE_GATE_MAX_INTEGRITY);

        for pulse in 1..=10 {
            let destroyed = palisade.advance_gate_assault(std::slice::from_ref(&ordered));
            assert_eq!(destroyed, pulse == 10);
        }
        assert_eq!(palisade.gate_integrity, 0);
        assert_eq!(palisade.gate_state, SiegeGateState::Destroyed);
        assert!(!palisade.advance_gate_assault(std::slice::from_ref(&ordered)));

        let mut stone = SiegeBattleState::test_siege(battlefield);
        stone.advance_gate_assault(&[ordered.clone(), ordered]);
        assert_eq!(
            stone.gate_integrity,
            SIEGE_GATE_MAX_INTEGRITY - 2 * SiegeProfile::StoneWallsV1.gate_damage_per_pulse()
        );
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

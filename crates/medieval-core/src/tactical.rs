use std::{collections::HashSet, fmt};

use serde::{Deserialize, Serialize};

pub const TACTICAL_TICKS_PER_SECOND: u32 = 20;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattlePoint {
    pub x_mm: u32,
    pub y_mm: u32,
}

impl BattlePoint {
    #[must_use]
    pub const fn new(x_mm: u32, y_mm: u32) -> Self {
        Self { x_mm, y_mm }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatBattlefield {
    pub width_mm: u32,
    pub depth_mm: u32,
}

impl FlatBattlefield {
    #[must_use]
    pub const fn new(width_mm: u32, depth_mm: u32) -> Self {
        Self { width_mm, depth_mm }
    }

    const fn contains(self, point: BattlePoint) -> bool {
        point.x_mm <= self.width_mm && point.y_mm <= self.depth_mm
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BattleSide {
    Attacker,
    Defender,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Formation {
    Line { files: u16 },
    Column { files: u16 },
}

impl Formation {
    const fn files(self) -> u16 {
        match self {
            Self::Line { files } | Self::Column { files } => files,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovementOrder {
    pub unit_id: String,
    pub destination: BattlePoint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalUnit {
    id: String,
    side: BattleSide,
    soldiers: u16,
    position: BattlePoint,
    formation: Formation,
    speed_mm_per_tick: u32,
    destination: Option<BattlePoint>,
}

impl TacticalUnit {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        side: BattleSide,
        soldiers: u16,
        position: BattlePoint,
        formation: Formation,
        speed_mm_per_tick: u32,
    ) -> Self {
        Self {
            id: id.into(),
            side,
            soldiers,
            position,
            formation,
            speed_mm_per_tick,
            destination: None,
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn side(&self) -> BattleSide {
        self.side
    }

    #[must_use]
    pub const fn soldiers(&self) -> u16 {
        self.soldiers
    }

    #[must_use]
    pub const fn position(&self) -> BattlePoint {
        self.position
    }

    #[must_use]
    pub const fn formation(&self) -> Formation {
        self.formation
    }

    #[must_use]
    pub const fn speed_mm_per_tick(&self) -> u32 {
        self.speed_mm_per_tick
    }

    #[must_use]
    pub const fn destination(&self) -> Option<BattlePoint> {
        self.destination
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalBattle {
    tick: u64,
    battlefield: FlatBattlefield,
    units: Vec<TacticalUnit>,
}

impl TacticalBattle {
    pub fn new(
        battlefield: FlatBattlefield,
        units: Vec<TacticalUnit>,
    ) -> Result<Self, TacticalError> {
        if battlefield.width_mm == 0 || battlefield.depth_mm == 0 {
            return Err(TacticalError::InvalidBattlefield {
                width_mm: battlefield.width_mm,
                depth_mm: battlefield.depth_mm,
            });
        }

        let mut unit_ids = HashSet::with_capacity(units.len());
        for unit in &units {
            if unit.id.trim().is_empty() {
                return Err(TacticalError::EmptyUnitId);
            }
            if !unit_ids.insert(unit.id.clone()) {
                return Err(TacticalError::DuplicateUnitId(unit.id.clone()));
            }
            if unit.formation.files() == 0 {
                return Err(TacticalError::InvalidFormation(unit.id.clone()));
            }
            if unit.speed_mm_per_tick == 0 {
                return Err(TacticalError::ZeroMovementSpeed(unit.id.clone()));
            }
            if !battlefield.contains(unit.position) {
                return Err(TacticalError::UnitOutOfBounds {
                    unit_id: unit.id.clone(),
                    position: unit.position,
                });
            }
        }

        Ok(Self {
            tick: 0,
            battlefield,
            units,
        })
    }

    #[must_use]
    pub const fn tick(&self) -> u64 {
        self.tick
    }

    #[must_use]
    pub const fn battlefield(&self) -> FlatBattlefield {
        self.battlefield
    }

    #[must_use]
    pub fn units(&self) -> &[TacticalUnit] {
        &self.units
    }

    pub fn issue_move_order(&mut self, order: MovementOrder) -> Result<(), TacticalError> {
        let unit_index = self
            .units
            .iter()
            .position(|unit| unit.id == order.unit_id)
            .ok_or_else(|| TacticalError::UnitNotFound(order.unit_id.clone()))?;

        if !self.battlefield.contains(order.destination) {
            return Err(TacticalError::DestinationOutOfBounds {
                unit_id: order.unit_id,
                destination: order.destination,
            });
        }

        let unit = &mut self.units[unit_index];
        unit.destination = (unit.position != order.destination).then_some(order.destination);
        Ok(())
    }

    pub fn advance_ticks(&mut self, ticks: u32) {
        for _ in 0..ticks {
            for unit in &mut self.units {
                advance_unit(unit);
            }
            self.tick = self.tick.saturating_add(1);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TacticalError {
    InvalidBattlefield {
        width_mm: u32,
        depth_mm: u32,
    },
    EmptyUnitId,
    DuplicateUnitId(String),
    InvalidFormation(String),
    ZeroMovementSpeed(String),
    UnitOutOfBounds {
        unit_id: String,
        position: BattlePoint,
    },
    UnitNotFound(String),
    DestinationOutOfBounds {
        unit_id: String,
        destination: BattlePoint,
    },
}

impl fmt::Display for TacticalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBattlefield { width_mm, depth_mm } => write!(
                formatter,
                "battlefield dimensions must be positive, got {width_mm}x{depth_mm} mm"
            ),
            Self::EmptyUnitId => write!(formatter, "tactical unit IDs must not be empty"),
            Self::DuplicateUnitId(unit_id) => {
                write!(formatter, "tactical unit ID {unit_id} is duplicated")
            }
            Self::InvalidFormation(unit_id) => {
                write!(formatter, "tactical unit {unit_id} has an empty formation")
            }
            Self::ZeroMovementSpeed(unit_id) => {
                write!(
                    formatter,
                    "tactical unit {unit_id} must have a positive movement speed"
                )
            }
            Self::UnitOutOfBounds { unit_id, position } => write!(
                formatter,
                "tactical unit {unit_id} starts outside the battlefield at ({}, {}) mm",
                position.x_mm, position.y_mm
            ),
            Self::UnitNotFound(unit_id) => {
                write!(formatter, "tactical unit {unit_id} does not exist")
            }
            Self::DestinationOutOfBounds {
                unit_id,
                destination,
            } => write!(
                formatter,
                "movement order for {unit_id} leaves the battlefield at ({}, {}) mm",
                destination.x_mm, destination.y_mm
            ),
        }
    }
}

impl std::error::Error for TacticalError {}

fn advance_unit(unit: &mut TacticalUnit) {
    let Some(destination) = unit.destination else {
        return;
    };

    if unit.position == destination {
        unit.destination = None;
        return;
    }

    let dx = i64::from(destination.x_mm) - i64::from(unit.position.x_mm);
    let dy = i64::from(destination.y_mm) - i64::from(unit.position.y_mm);
    let distance_squared = squared_components(dx, dy);
    let speed = u128::from(unit.speed_mm_per_tick);

    if distance_squared <= speed * speed {
        unit.position = destination;
        unit.destination = None;
        return;
    }

    let distance = integer_sqrt_ceil(distance_squared);
    let divisor = i128::try_from(distance).expect("movement distance must fit in i128");
    let speed = i128::from(unit.speed_mm_per_tick);
    let mut step_x =
        i64::try_from(i128::from(dx) * speed / divisor).expect("movement x step must fit in i64");
    let mut step_y =
        i64::try_from(i128::from(dy) * speed / divisor).expect("movement y step must fit in i64");

    if step_x == 0 && step_y == 0 {
        if dx.unsigned_abs() >= dy.unsigned_abs() {
            step_x = dx.signum();
        } else {
            step_y = dy.signum();
        }
    }

    let next_x = i64::from(unit.position.x_mm) + step_x;
    let next_y = i64::from(unit.position.y_mm) + step_y;
    unit.position = BattlePoint::new(
        u32::try_from(next_x).expect("movement x remains between current position and target"),
        u32::try_from(next_y).expect("movement y remains between current position and target"),
    );
}

fn squared_components(dx: i64, dy: i64) -> u128 {
    let x = u128::from(dx.unsigned_abs());
    let y = u128::from(dy.unsigned_abs());
    x * x + y * y
}

fn integer_sqrt_ceil(value: u128) -> u128 {
    if value < 2 {
        return value;
    }

    let mut low = 1_u128;
    let mut high = value;
    let mut floor = 1_u128;

    while low <= high {
        let mid = low + (high - low) / 2;
        if mid <= value / mid {
            floor = mid;
            low = mid.saturating_add(1);
        } else {
            high = mid - 1;
        }
    }

    if floor * floor == value {
        floor
    } else {
        floor.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_unit(
        id: &str,
        side: BattleSide,
        position: BattlePoint,
        formation: Formation,
    ) -> TacticalUnit {
        TacticalUnit::new(id, side, 80, position, formation, 1_200)
    }

    fn sample_battle() -> TacticalBattle {
        TacticalBattle::new(
            FlatBattlefield::new(300_000, 200_000),
            vec![
                sample_unit(
                    "attacker-spears",
                    BattleSide::Attacker,
                    BattlePoint::new(20_000, 100_000),
                    Formation::Line { files: 20 },
                ),
                sample_unit(
                    "defender-spears",
                    BattleSide::Defender,
                    BattlePoint::new(280_000, 100_000),
                    Formation::Line { files: 20 },
                ),
            ],
        )
        .unwrap()
    }

    fn unit<'a>(battle: &'a TacticalBattle, id: &str) -> &'a TacticalUnit {
        battle.units().iter().find(|unit| unit.id() == id).unwrap()
    }

    fn point_distance_squared(left: BattlePoint, right: BattlePoint) -> u128 {
        squared_components(
            i64::from(right.x_mm) - i64::from(left.x_mm),
            i64::from(right.y_mm) - i64::from(left.y_mm),
        )
    }

    #[test]
    fn setup_validation_rejects_duplicate_ids_and_invalid_units() {
        let duplicate = TacticalBattle::new(
            FlatBattlefield::new(10_000, 10_000),
            vec![
                sample_unit(
                    "spears",
                    BattleSide::Attacker,
                    BattlePoint::new(1_000, 1_000),
                    Formation::Line { files: 10 },
                ),
                sample_unit(
                    "spears",
                    BattleSide::Defender,
                    BattlePoint::new(9_000, 9_000),
                    Formation::Line { files: 10 },
                ),
            ],
        );
        assert!(matches!(duplicate, Err(TacticalError::DuplicateUnitId(_))));

        let invalid_formation = TacticalBattle::new(
            FlatBattlefield::new(10_000, 10_000),
            vec![sample_unit(
                "spears",
                BattleSide::Attacker,
                BattlePoint::new(1_000, 1_000),
                Formation::Column { files: 0 },
            )],
        );
        assert!(matches!(
            invalid_formation,
            Err(TacticalError::InvalidFormation(_))
        ));
    }

    #[test]
    fn rejected_orders_do_not_mutate_authoritative_state() {
        let mut battle = sample_battle();
        let before = battle.clone();

        assert!(matches!(
            battle.issue_move_order(MovementOrder {
                unit_id: "missing".into(),
                destination: BattlePoint::new(50_000, 50_000),
            }),
            Err(TacticalError::UnitNotFound(_))
        ));
        assert_eq!(battle, before);

        assert!(matches!(
            battle.issue_move_order(MovementOrder {
                unit_id: "attacker-spears".into(),
                destination: BattlePoint::new(300_001, 50_000),
            }),
            Err(TacticalError::DestinationOutOfBounds { .. })
        ));
        assert_eq!(battle, before);
    }

    #[test]
    fn fixed_tick_replay_is_deterministic_and_serializable() {
        let mut first = sample_battle();
        let mut second = sample_battle();
        let order = MovementOrder {
            unit_id: "attacker-spears".into(),
            destination: BattlePoint::new(120_000, 140_000),
        };

        first.issue_move_order(order.clone()).unwrap();
        second.issue_move_order(order).unwrap();
        first.advance_ticks(TACTICAL_TICKS_PER_SECOND * 7);
        second.advance_ticks(TACTICAL_TICKS_PER_SECOND * 7);

        assert_eq!(first.tick(), 140);
        assert_eq!(first, second);

        let encoded = serde_json::to_string(&first).unwrap();
        let decoded: TacticalBattle = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, first);
    }

    #[test]
    fn movement_never_exceeds_speed_and_arrives_exactly() {
        let mut battle = sample_battle();
        let destination = BattlePoint::new(90_000, 150_000);
        battle
            .issue_move_order(MovementOrder {
                unit_id: "attacker-spears".into(),
                destination,
            })
            .unwrap();

        for _ in 0..200 {
            let before = unit(&battle, "attacker-spears").position();
            let speed = unit(&battle, "attacker-spears").speed_mm_per_tick();
            battle.advance_ticks(1);
            let after = unit(&battle, "attacker-spears").position();
            assert!(
                point_distance_squared(before, after)
                    <= u128::from(speed) * u128::from(speed)
            );
            if unit(&battle, "attacker-spears").destination().is_none() {
                break;
            }
        }

        let attacker = unit(&battle, "attacker-spears");
        assert_eq!(attacker.position(), destination);
        assert_eq!(attacker.destination(), None);
    }

    #[test]
    fn extreme_diagonal_movement_still_respects_speed_bound() {
        let speed = 1_000_000;
        let mut battle = TacticalBattle::new(
            FlatBattlefield::new(u32::MAX, u32::MAX),
            vec![TacticalUnit::new(
                "scouts",
                BattleSide::Attacker,
                20,
                BattlePoint::new(0, 0),
                Formation::Column { files: 4 },
                speed,
            )],
        )
        .unwrap();
        battle
            .issue_move_order(MovementOrder {
                unit_id: "scouts".into(),
                destination: BattlePoint::new(u32::MAX, u32::MAX),
            })
            .unwrap();

        let before = unit(&battle, "scouts").position();
        battle.advance_ticks(1);
        let after = unit(&battle, "scouts").position();

        assert!(
            point_distance_squared(before, after) <= u128::from(speed) * u128::from(speed)
        );
    }

    #[test]
    fn movement_preserves_identity_side_formation_and_soldier_count() {
        let mut battle = sample_battle();
        let before = unit(&battle, "attacker-spears").clone();
        battle
            .issue_move_order(MovementOrder {
                unit_id: "attacker-spears".into(),
                destination: BattlePoint::new(80_000, 120_000),
            })
            .unwrap();
        battle.advance_ticks(10);

        let after = unit(&battle, "attacker-spears");
        assert_eq!(after.id(), before.id());
        assert_eq!(after.side(), before.side());
        assert_eq!(after.formation(), before.formation());
        assert_eq!(after.soldiers(), before.soldiers());
        assert_ne!(after.position(), before.position());
    }
}

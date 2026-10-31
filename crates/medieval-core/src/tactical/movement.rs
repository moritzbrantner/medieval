//! The movement phase: unit steps, charges and copy-on-write phase-start state.

use super::geometry::{
    integer_sqrt_ceil, measured_points_within_distance, point_distance_squared, squared_components,
};
use super::{
    BattlePoint, BattleSide, COMBAT_CONTACT_DISTANCE_MM, FlatBattlefield,
    IDLE_FATIGUE_RECOVERY_PER_TICK, MAX_TACTICAL_FATIGUE, MOVEMENT_FATIGUE_PER_TICK,
    PURSUIT_DISTANCE_MM, ROUT_FATIGUE_PER_TICK, TacticalBattle, TacticalUnit, TacticalUnitState,
    find_unit,
};
use crate::TacticalWorkCounters;
use crate::deployment::siege::{SiegeBattleState, SiegeLadderPlacement};

impl TacticalBattle {
    pub(super) fn is_passable_at(&self, point: BattlePoint) -> bool {
        self.terrain.is_passable_at(self.battlefield, point)
            && self.siege.is_none_or(|siege| siege.is_passable_at(point))
    }

    fn movement_waypoint(&self, from: BattlePoint, destination: BattlePoint) -> BattlePoint {
        let terrain_waypoint = self
            .terrain
            .movement_waypoint(self.battlefield, from, destination);
        self.siege.map_or(terrain_waypoint, |siege| {
            siege.movement_waypoint(from, terrain_waypoint)
        })
    }

    fn measured_movement_waypoint(
        &self,
        counters: &mut TacticalWorkCounters,
        from: BattlePoint,
        destination: BattlePoint,
    ) -> BattlePoint {
        counters.path_requests += 1;
        self.movement_waypoint(from, destination)
    }

    /// Whether `unit` may stand at `point`: passable ground, or a placed
    /// ladder cell for a unit that can climb ladders.
    fn unit_may_stand_at(&self, unit: &TacticalUnit, point: BattlePoint) -> bool {
        self.is_passable_at(point)
            || (self.climbs_ladders(unit)
                && self.terrain.is_passable_at(self.battlefield, point)
                && self
                    .siege_ladders
                    .iter()
                    .any(|ladder| ladder.area.contains(point)))
    }

    /// One movement step of `unit` toward `destination` at `speed`. Without
    /// ladders this is the shared siege and terrain path. With ladders, a
    /// destination on a ladder is approached at its foot on the unit's side,
    /// and units that can climb route a closed-wall crossing over a ladder:
    /// they step from the foot onto the ladder's edge, cross the wall
    /// thickness at the profile's climb speed, and stay at the top while a
    /// defender holds it (`held`, per ladder in placement order).
    fn unit_step(
        &self,
        counters: &mut TacticalWorkCounters,
        unit: &TacticalUnit,
        destination: BattlePoint,
        speed: u32,
        held: &[bool],
    ) -> BattlePoint {
        let from = unit.position;
        let Some(siege) = self.siege.filter(|_| !self.siege_ladders.is_empty()) else {
            let waypoint = self.measured_movement_waypoint(counters, from, destination);
            return move_point_toward(from, waypoint, speed);
        };
        let destination = match self
            .siege_ladders
            .iter()
            .find(|ladder| ladder.area.contains(destination))
        {
            Some(ladder) if !siege.in_wall_band(from) => BattlePoint::new(
                crate::deployment::siege::ladder_foot(ladder.area, from).x_mm,
                destination.y_mm,
            ),
            _ => destination,
        };
        if !self.climbs_ladders(unit) {
            let waypoint = self.measured_movement_waypoint(counters, from, destination);
            return move_point_toward(from, waypoint, speed);
        }
        counters.path_requests += 1;
        let terrain_waypoint = self
            .terrain
            .movement_waypoint(self.battlefield, from, destination);
        let Some(waypoint) = siege.ladder_waypoint(&self.siege_ladders, from, terrain_waypoint)
        else {
            return move_point_toward(from, siege.movement_waypoint(from, terrain_waypoint), speed);
        };
        if let Some(index) = self
            .siege_ladders
            .iter()
            .position(|ladder| ladder.area.contains(from))
        {
            // Climbing: the wall thickness takes the profile's climb time, and
            // nobody leaves onto the defenders' (east) side of a held ladder.
            let area = self.siege_ladders[index].area;
            let next = move_point_toward(from, waypoint, siege.ladder_climb_speed_mm_per_tick());
            if next.x_mm > area.max_x_mm && held.get(index).copied().unwrap_or(false) {
                return BattlePoint::new(area.max_x_mm, next.y_mm);
            }
            return next;
        }
        let next = move_point_toward(from, waypoint, speed);
        let gate = siege.layout.gate;
        let entry_x = if from.x_mm < gate.min_x_mm && next.x_mm >= gate.min_x_mm {
            gate.min_x_mm
        } else if from.x_mm > gate.max_x_mm && next.x_mm <= gate.max_x_mm {
            gate.max_x_mm
        } else {
            return next;
        };
        // Stepping off the foot onto the ladder's near edge.
        let entry = BattlePoint::new(entry_x, from.y_mm);
        if self
            .siege_ladders
            .iter()
            .any(|ladder| ladder.area.contains(entry))
        {
            entry
        } else {
            from
        }
    }

    /// Places the ladder of every formed unit that has reached the foot of
    /// its ordered ladder this tick, in unit order.
    fn place_arrived_ladders(&mut self, counters: &mut TacticalWorkCounters) {
        let Some(siege) = self.siege else {
            return;
        };
        for index in 0..self.units.len() {
            let unit = &self.units[index];
            let Some(at) = unit.ladder_order else {
                continue;
            };
            counters.unit_scan_visits += 1;
            let Some(area) = siege.ladder_area(at) else {
                self.units[index].ladder_order = None;
                continue;
            };
            if unit.state != TacticalUnitState::Formed
                || unit.position != crate::deployment::siege::ladder_foot(area, unit.position)
            {
                continue;
            }
            let placed_by = unit.id.clone();
            self.units[index].ladder_order = None;
            self.siege_ladders
                .push(SiegeLadderPlacement { area, placed_by });
        }
    }

    fn advance_movement_orders(&mut self, counters: &mut TacticalWorkCounters) {
        // Queued-waypoint promotion and attack-move acquisition each visit
        // every unit once.
        counters.unit_scan_visits += 2 * self.units.len() as u64;
        for unit in &mut self.units {
            if unit.state == TacticalUnitState::Formed
                && unit.destination.is_none()
                && unit.engagement_target.is_none()
                && !unit.queued_movements.is_empty()
            {
                let waypoint = unit.queued_movements.remove(0);
                unit.destination =
                    (unit.position != waypoint.destination).then_some(waypoint.destination);
                unit.movement_mode = waypoint.mode;
                unit.arrival_facing = None;
                unit.interrupt_charge();
            }
        }
        // Acquisition reads only enemies' side, state and position, which this
        // loop never changes, so it reads live units instead of a snapshot.
        for index in 0..self.units.len() {
            let unit = &self.units[index];
            if unit.state != TacticalUnitState::Formed
                || unit.movement_mode != crate::MovementMode::AttackMove
            {
                continue;
            }
            if unit.destination.is_none() {
                self.units[index].engagement_target = None;
                self.units[index].movement_mode = crate::MovementMode::March;
                continue;
            }
            let units = &self.units;
            let radius = unit.attack_range_mm().max(5_000);
            let mut eligible = |enemy: &&TacticalUnit| {
                counters.target_candidate_visits += 1;
                enemy.side != unit.side
                    && enemy.state == TacticalUnitState::Formed
                    && measured_points_within_distance(
                        counters,
                        unit.position,
                        enemy.position,
                        radius,
                    )
            };
            let retained = units
                .iter()
                .filter(&mut eligible)
                .find(|enemy| Some(enemy.id.as_str()) == unit.engagement_target.as_deref());
            let target = retained.or_else(|| {
                units.iter().filter(&mut eligible).min_by(|left, right| {
                    point_distance_squared(unit.position, left.position)
                        .cmp(&point_distance_squared(unit.position, right.position))
                        .then_with(|| left.id.cmp(&right.id))
                })
            });
            let target = target.map(|enemy| enemy.id.clone());
            if self.units[index].engagement_target != target {
                self.units[index].interrupt_charge();
                self.units[index].engagement_target = target;
            }
        }
    }

    /// Every unit moves against the state all units had when the phase began.
    /// Instead of cloning the whole unit vector, a unit is copied only right
    /// before its own step may change it; reads of other units go through
    /// [`MovementBefore`], so idle units cost a visit but no copy.
    pub(super) fn advance_movement_phase(
        &mut self,
        before: &mut MovementBefore,
        counters: &mut TacticalWorkCounters,
    ) {
        before.clear();
        self.advance_movement_orders(counters);
        // Whether each ladder's top is held, from the state all units had when
        // the phase began, so the result never depends on unit order.
        let held: Vec<bool> = self
            .siege_ladders
            .iter()
            .map(|ladder| {
                counters.unit_scan_visits += self.units.len() as u64;
                SiegeBattleState::ladder_held(ladder, &self.units)
            })
            .collect();
        for index in 0..self.units.len() {
            counters.movement_unit_visits += 1;
            if !movement_may_change(&self.units[index]) {
                continue;
            }
            counters.snapshot_unit_copies += 1;
            before.save(index, self.units[index].clone());
            let unit = before.saved_unit(index);
            match unit.state {
                TacticalUnitState::Formed => {
                    let target = unit
                        .engagement_target
                        .as_deref()
                        .and_then(|target_id| {
                            before
                                .find(&self.units, target_id, counters)
                                .map(TargetView::of)
                        })
                        .filter(|target| {
                            !matches!(
                                target.state,
                                TacticalUnitState::Destroyed | TacticalUnitState::Escaped { .. }
                            )
                        });
                    self.advance_formed_unit(counters, index, unit, target, &held);
                }
                TacticalUnitState::Routed => {
                    let nearest_enemy = before
                        .units(&self.units)
                        .filter(|candidate| {
                            counters.target_candidate_visits += 1;
                            candidate.side != unit.side
                                && candidate.state == TacticalUnitState::Formed
                        })
                        .min_by(|left, right| {
                            point_distance_squared(unit.position, left.position)
                                .cmp(&point_distance_squared(unit.position, right.position))
                                .then_with(|| left.id.cmp(&right.id))
                        })
                        .map(|enemy| enemy.position);
                    self.advance_routed_unit(counters, index, unit, nearest_enemy, &held);
                }
                TacticalUnitState::Withdrawing { routed } => {
                    self.advance_withdrawing_unit(counters, index, unit, routed, &held);
                }
                TacticalUnitState::Escaped { .. } | TacticalUnitState::Destroyed => {}
            }
        }
        self.place_arrived_ladders(counters);
        counters.unit_scan_visits += self.units.len() as u64;
        // Charges read only their own previous charge and positions, never
        // another unit's charge, so all are computed before any is written.
        let charges: Vec<(usize, crate::CavalryChargeState)> = before
            .units(&self.units)
            .enumerate()
            .filter(|(_, unit)| unit.charge.is_some())
            .map(|(index, unit)| (index, self.next_charge(counters, index, unit, before)))
            .collect();
        for (index, charge) in charges {
            self.units[index].charge = Some(charge);
        }
    }

    /// The charge state of a unit that carried one when movement began.
    fn next_charge(
        &self,
        counters: &mut TacticalWorkCounters,
        index: usize,
        before: &TacticalUnit,
        before_units: &MovementBefore,
    ) -> crate::CavalryChargeState {
        use crate::CavalryChargeState as Charge;
        let previous = before
            .charge
            .expect("only units with a charge state advance it");
        if before.state != TacticalUnitState::Formed {
            return Charge::Ready;
        }
        if let Charge::Recovering { ticks_remaining } = previous {
            return if ticks_remaining <= 1 {
                Charge::Ready
            } else {
                Charge::Recovering {
                    ticks_remaining: ticks_remaining - 1,
                }
            };
        }
        let target = before
            .engagement_target
            .as_deref()
            .and_then(|id| before_units.find(&self.units, id, counters))
            .filter(|target| target.state == TacticalUnitState::Formed);
        let Some(target) = target else {
            return previous.interrupted();
        };
        let next = self.units[index].position;
        let target_position = find_unit(&self.units, &target.id, counters)
            .expect("movement preserves units")
            .position;
        let intercepted = self.units.iter().any(|other| {
            counters.target_candidate_visits += 1;
            other.side != before.side
                && other.id != target.id
                && other.state == TacticalUnitState::Formed
                && measured_points_within_distance(
                    counters,
                    next,
                    other.position,
                    COMBAT_CONTACT_DISTANCE_MM,
                )
        });
        let straight =
            before.facing.is_some_and(|facing| {
                facing.classify(before.position, target.position) == crate::CombatArc::Front
            }) && self.measured_movement_waypoint(counters, before.position, target.position)
                == target.position
                && self
                    .terrain
                    .ground_cover_at(self.battlefield, before.position)
                    == crate::TacticalGroundCover::Open
                && self.terrain.ground_cover_at(self.battlefield, next)
                    == crate::TacticalGroundCover::Open;
        if intercepted || !straight {
            return previous.interrupted();
        }
        let displacement = next
            .x_mm
            .abs_diff(before.position.x_mm)
            .max(next.y_mm.abs_diff(before.position.y_mm));
        let run_up_mm = previous
            .run_up_mm()
            .saturating_add(displacement)
            .min(crate::charge::CHARGE_RUN_UP_MM);
        if measured_points_within_distance(
            counters,
            next,
            target_position,
            COMBAT_CONTACT_DISTANCE_MM,
        ) {
            return Charge::Contact { run_up_mm };
        }
        if next == before.position
            && !measured_points_within_distance(
                counters,
                before.position,
                target.position,
                COMBAT_CONTACT_DISTANCE_MM,
            )
        {
            return previous.interrupted();
        }
        if run_up_mm >= crate::charge::CHARGE_RUN_UP_MM {
            Charge::Charging { run_up_mm }
        } else {
            Charge::Approaching { run_up_mm }
        }
    }

    fn advance_formed_unit(
        &mut self,
        counters: &mut TacticalWorkCounters,
        index: usize,
        unit: &TacticalUnit,
        target: Option<TargetView>,
        held: &[bool],
    ) {
        if let Some(target) = target
            && target.is_pursuit_target()
            && !measured_points_within_distance(
                counters,
                unit.position,
                target.position,
                PURSUIT_DISTANCE_MM,
            )
        {
            self.units[index].engagement_target = None;
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_sub(IDLE_FATIGUE_RECOVERY_PER_TICK);
            return;
        }

        let gate_attack_position = self.siege.filter(|_| unit.gate_attack).map(|siege| {
            counters.target_candidate_visits += 1;
            if siege.within_gate_attack_distance(unit.position) {
                unit.position
            } else {
                siege.gate_attack_position(unit.position)
            }
        });
        let ladder_position = self
            .siege
            .zip(unit.ladder_order)
            .and_then(|(siege, at)| siege.ladder_area(at))
            .map(|area| crate::deployment::siege::ladder_foot(area, unit.position));
        let destination = if unit.movement_mode == crate::MovementMode::AttackMove {
            target.map(|target| target.position).or(unit.destination)
        } else {
            unit.destination
                .or_else(|| target.map(|target| target.position))
                .or(gate_attack_position)
                .or(ladder_position)
        };
        let Some(destination) = destination else {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_sub(IDLE_FATIGUE_RECOVERY_PER_TICK);
            return;
        };

        let engagement_stop_distance = target
            .filter(|target| target.state == TacticalUnitState::Formed)
            .map_or(COMBAT_CONTACT_DISTANCE_MM, |_| unit.attack_range_mm());
        let holding_at_gate = unit.destination.is_none()
            && unit.engagement_target.is_none()
            && (gate_attack_position == Some(unit.position)
                || ladder_position == Some(unit.position));
        // A melee unit next to its target across a wall is not in contact; it
        // keeps following the siege path toward the gate instead of holding.
        let blocked_by_wall = engagement_stop_distance <= COMBAT_CONTACT_DISTANCE_MM
            && self.wall_separates(unit.position, destination);
        if holding_at_gate
            || (unit.engagement_target.is_some()
                && !blocked_by_wall
                && measured_points_within_distance(
                    counters,
                    unit.position,
                    destination,
                    engagement_stop_distance,
                ))
        {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_sub(IDLE_FATIGUE_RECOVERY_PER_TICK);
            return;
        }

        let base_movement_speed = if target.is_some_and(|target| target.is_pursuit_target()) {
            unit.speed_mm_per_tick.saturating_mul(2)
        } else {
            unit.speed_mm_per_tick
        };
        counters.terrain_speed_queries += 1;
        let movement_speed = self.terrain().movement_speed_mm_per_tick(
            self.battlefield,
            unit.position,
            base_movement_speed,
        );
        let next = self.unit_step(counters, unit, destination, movement_speed, held);
        debug_assert!(self.unit_may_stand_at(unit, next));
        if unit.facing.is_some() && next != unit.position {
            self.units[index].facing = crate::Facing::toward(unit.position, next);
        }
        self.units[index].position = next;
        if self.units[index].destination == Some(destination) && next == destination {
            self.units[index].destination = None;
            if let Some(facing) = self.units[index].arrival_facing.take() {
                self.units[index].facing = Some(facing);
            }
        }
        if next != unit.position {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_add(MOVEMENT_FATIGUE_PER_TICK)
                .min(MAX_TACTICAL_FATIGUE);
        }
    }

    fn advance_withdrawing_unit(
        &mut self,
        counters: &mut TacticalWorkCounters,
        index: usize,
        unit: &TacticalUnit,
        routed: bool,
        held: &[bool],
    ) {
        let edge = match unit.side {
            BattleSide::Attacker => 0,
            BattleSide::Defender => self.battlefield.width_mm,
        };
        let destination = BattlePoint::new(edge, unit.position.y_mm);
        let base_speed = if routed {
            unit.speed_mm_per_tick.saturating_mul(2)
        } else {
            unit.speed_mm_per_tick
        };
        counters.terrain_speed_queries += 1;
        let speed =
            self.terrain
                .movement_speed_mm_per_tick(self.battlefield, unit.position, base_speed);
        let next = self.unit_step(counters, unit, destination, speed, held);
        debug_assert!(self.unit_may_stand_at(unit, next));
        let before_position = unit.position;
        let unit = &mut self.units[index];
        unit.position = next;
        if next.x_mm == edge {
            unit.state = TacticalUnitState::Escaped { routed };
        } else if next != before_position {
            unit.fatigue = unit
                .fatigue
                .saturating_add(if routed {
                    ROUT_FATIGUE_PER_TICK
                } else {
                    MOVEMENT_FATIGUE_PER_TICK
                })
                .min(MAX_TACTICAL_FATIGUE);
        }
    }

    fn advance_routed_unit(
        &mut self,
        counters: &mut TacticalWorkCounters,
        index: usize,
        unit: &TacticalUnit,
        nearest_enemy: Option<BattlePoint>,
        held: &[bool],
    ) {
        let Some(enemy_position) = nearest_enemy else {
            return;
        };
        counters.terrain_speed_queries += 1;
        let route_speed = self.terrain().movement_speed_mm_per_tick(
            self.battlefield,
            unit.position,
            unit.speed_mm_per_tick.saturating_mul(2),
        );
        let desired = move_point_away(
            unit.position,
            enemy_position,
            route_speed,
            self.battlefield,
            unit.side,
        );
        let next = self.unit_step(counters, unit, desired, route_speed, held);
        debug_assert!(self.unit_may_stand_at(unit, next));
        self.units[index].position = next;
        if next != unit.position {
            self.units[index].fatigue = self.units[index]
                .fatigue
                .saturating_add(ROUT_FATIGUE_PER_TICK)
                .min(MAX_TACTICAL_FATIGUE);
        }
    }
}

fn move_point_toward(current: BattlePoint, destination: BattlePoint, speed_mm: u32) -> BattlePoint {
    if current == destination {
        return destination;
    }
    let dx = i64::from(destination.x_mm) - i64::from(current.x_mm);
    let dy = i64::from(destination.y_mm) - i64::from(current.y_mm);
    let distance_squared = squared_components(dx, dy);
    let speed = u128::from(speed_mm);
    if distance_squared <= speed * speed {
        return destination;
    }

    let (step_x, step_y) = step_vector(dx, dy, speed_mm);
    BattlePoint::new(
        u32::try_from(i64::from(current.x_mm) + step_x)
            .expect("movement x remains between current position and target"),
        u32::try_from(i64::from(current.y_mm) + step_y)
            .expect("movement y remains between current position and target"),
    )
}

fn move_point_away(
    current: BattlePoint,
    enemy: BattlePoint,
    speed_mm: u32,
    battlefield: FlatBattlefield,
    side: BattleSide,
) -> BattlePoint {
    let mut dx = i64::from(current.x_mm) - i64::from(enemy.x_mm);
    let dy = i64::from(current.y_mm) - i64::from(enemy.y_mm);
    if dx == 0 && dy == 0 {
        dx = match side {
            BattleSide::Attacker => -1,
            BattleSide::Defender => 1,
        };
    }
    let (step_x, step_y) = step_vector(dx, dy, speed_mm);
    let x = (i64::from(current.x_mm) + step_x).clamp(0, i64::from(battlefield.width_mm));
    let y = (i64::from(current.y_mm) + step_y).clamp(0, i64::from(battlefield.depth_mm));
    BattlePoint::new(u32::try_from(x).unwrap(), u32::try_from(y).unwrap())
}

fn step_vector(dx: i64, dy: i64, speed_mm: u32) -> (i64, i64) {
    let distance = integer_sqrt_ceil(squared_components(dx, dy));
    let divisor = i128::try_from(distance).expect("movement distance must fit in i128");
    let speed = i128::from(speed_mm);
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
    (step_x, step_y)
}

/// Whether a unit's movement step can change it. Idle formed units without
/// fatigue to recover, and escaped or destroyed units, are left untouched.
fn movement_may_change(unit: &TacticalUnit) -> bool {
    match unit.state {
        TacticalUnitState::Formed => {
            unit.destination.is_some()
                || unit.engagement_target.is_some()
                || unit.gate_attack
                || unit.ladder_order.is_some()
                || unit.fatigue > 0
        }
        TacticalUnitState::Routed | TacticalUnitState::Withdrawing { .. } => true,
        TacticalUnitState::Escaped { .. } | TacticalUnitState::Destroyed => false,
    }
}

/// Copy-on-write view of the unit vector as it was when a movement phase
/// began. Units are saved in increasing index order right before their own
/// step; every other unit is still unchanged in the live vector.
#[derive(Default)]
pub(super) struct MovementBefore {
    saved: Vec<(usize, TacticalUnit)>,
}

impl MovementBefore {
    fn clear(&mut self) {
        self.saved.clear();
    }

    fn save(&mut self, index: usize, unit: TacticalUnit) {
        debug_assert!(self.saved.last().is_none_or(|(last, _)| *last < index));
        self.saved.push((index, unit));
    }

    fn saved_unit(&self, index: usize) -> &TacticalUnit {
        let (saved_index, unit) = self.saved.last().expect("unit was just saved");
        debug_assert_eq!(*saved_index, index);
        unit
    }

    /// Every unit's phase-start state in index order, merging the saved copies
    /// with the live vector in one linear pass (no per-unit search).
    fn units<'a>(&'a self, live: &'a [TacticalUnit]) -> impl Iterator<Item = &'a TacticalUnit> {
        let mut saved = self.saved.iter().peekable();
        live.iter().enumerate().map(move |(index, unit)| {
            match saved.next_if(|(saved_index, _)| *saved_index == index) {
                Some((_, copy)) => copy,
                None => unit,
            }
        })
    }

    /// One unit's phase-start state; a binary search over the saved copies,
    /// used for single lookups only.
    fn unit<'a>(&'a self, live: &'a [TacticalUnit], index: usize) -> &'a TacticalUnit {
        self.saved
            .binary_search_by_key(&index, |(saved, _)| *saved)
            .map_or(&live[index], |position| &self.saved[position].1)
    }

    fn find<'a>(
        &'a self,
        live: &'a [TacticalUnit],
        id: &str,
        counters: &mut TacticalWorkCounters,
    ) -> Option<&'a TacticalUnit> {
        counters.indexed_unit_lookups += 1;
        live.binary_search_by(|unit| unit.id.as_str().cmp(id))
            .ok()
            .map(|index| self.unit(live, index))
    }
}

/// The parts of an engagement target a formed unit's movement reads.
#[derive(Copy, Clone)]
struct TargetView {
    position: BattlePoint,
    state: TacticalUnitState,
}

impl TargetView {
    fn of(unit: &TacticalUnit) -> Self {
        Self {
            position: unit.position,
            state: unit.state,
        }
    }

    pub(super) const fn is_pursuit_target(self) -> bool {
        matches!(
            self.state,
            TacticalUnitState::Routed | TacticalUnitState::Withdrawing { .. }
        )
    }
}

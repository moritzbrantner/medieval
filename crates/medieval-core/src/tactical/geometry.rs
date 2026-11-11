//! Exact integer distance checks between battle points.

use super::BattlePoint;
use crate::TacticalWorkCounters;

/// Exact deterministic tactical proximity: inclusive squared-distance comparison.
///
/// Tactical positions use the full `u32` battlefield range, so deltas are widened to `i64`.
/// The cheap axis rejection bounds each delta by the radius; squares and their sum then fit
/// `u128` without overflow.
#[cfg(test)]
pub(super) fn points_within_distance(
    left: BattlePoint,
    right: BattlePoint,
    distance_mm: u32,
) -> bool {
    measured_points_within_distance(
        &mut TacticalWorkCounters::default(),
        left,
        right,
        distance_mm,
    )
}

pub(super) fn measured_points_within_distance(
    counters: &mut TacticalWorkCounters,
    left: BattlePoint,
    right: BattlePoint,
    distance_mm: u32,
) -> bool {
    counters.proximity_queries += 1;
    let dx = i64::from(right.x_mm) - i64::from(left.x_mm);
    let dy = i64::from(right.y_mm) - i64::from(left.y_mm);
    let distance = u64::from(distance_mm);
    if dx.unsigned_abs() > distance || dy.unsigned_abs() > distance {
        return false;
    }

    counters.physics_contact_queries += 1;
    let dx = u128::from(dx.unsigned_abs());
    let dy = u128::from(dy.unsigned_abs());
    let distance = u128::from(distance);
    dx * dx + dy * dy <= distance * distance
}

pub(super) fn point_distance_squared(left: BattlePoint, right: BattlePoint) -> u128 {
    squared_components(
        i64::from(right.x_mm) - i64::from(left.x_mm),
        i64::from(right.y_mm) - i64::from(left.y_mm),
    )
}

pub(super) const fn square_u32(value: u32) -> u128 {
    let value = value as u128;
    value * value
}

pub(super) fn squared_components(dx: i64, dy: i64) -> u128 {
    let x = u128::from(dx.unsigned_abs());
    let y = u128::from(dy.unsigned_abs());
    x * x + y * y
}

pub(super) fn integer_sqrt_ceil(value: u128) -> u128 {
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

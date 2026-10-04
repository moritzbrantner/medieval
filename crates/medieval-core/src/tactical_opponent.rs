use crate::{BattlePoint, BattleSide, TacticalBattle, TacticalBattleState, TacticalError};

impl TacticalBattle {
    /// Assigns formed units to the nearest active enemy, with stable-ID ties.
    /// Finished battles and withdrawing forces retain their authoritative state.
    pub fn plan_opponent_orders(&mut self, side: BattleSide) -> Result<(), TacticalError> {
        if !matches!(self.state(), TacticalBattleState::Running) {
            return Ok(());
        }
        let enemies: Vec<_> = self
            .units()
            .iter()
            .filter(|unit| {
                unit.side() != side && (unit.can_receive_orders() || unit.is_withdrawing())
            })
            .map(|unit| (unit.id().to_owned(), unit.position()))
            .collect();
        let assignments: Vec<_> = self
            .units()
            .iter()
            .filter(|unit| unit.side() == side && unit.can_receive_orders())
            .filter_map(|unit| {
                let target = enemies.iter().min_by(|left, right| {
                    distance_squared(unit.position(), left.1)
                        .cmp(&distance_squared(unit.position(), right.1))
                        .then_with(|| left.0.cmp(&right.0))
                })?;
                (unit.engagement_target() != Some(target.0.as_str()))
                    .then(|| (unit.id().to_owned(), target.0.clone()))
            })
            .collect();
        for (unit, target) in assignments {
            self.issue_engagement_order(&unit, &target)?;
        }
        Ok(())
    }
}

fn distance_squared(left: BattlePoint, right: BattlePoint) -> u128 {
    let dx = u128::from(left.x_mm.abs_diff(right.x_mm));
    let dy = u128::from(left.y_mm.abs_diff(right.y_mm));
    dx * dx + dy * dy
}

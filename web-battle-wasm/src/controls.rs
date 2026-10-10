use std::cell::RefCell;

use medieval_core::{BattleSide, TacticalBattle};
use medieval_renderer::RenderViewState;

mod shared {
    include!("../../shared/tactical_controls.rs");

    impl TacticalControls {
        pub(super) fn reconcile_with_battle(&mut self, battle: &TacticalBattle) {
            self.sync_with_battle(battle);
        }

        /// Non-empty control groups in ascending group order.
        pub(super) fn assigned_control_groups(&self) -> Vec<(u8, Vec<String>)> {
            self.control_groups
                .iter()
                .filter(|(_, unit_ids)| !unit_ids.is_empty())
                .map(|(group, unit_ids)| (*group, unit_ids.iter().cloned().collect()))
                .collect()
        }
    }
}

pub use shared::TacticalControlRequest;

pub struct TacticalControls(RefCell<shared::TacticalControls>);

impl TacticalControls {
    pub fn new(battle: &TacticalBattle, player_side: BattleSide) -> Self {
        Self(RefCell::new(shared::TacticalControls::new(
            battle,
            player_side,
        )))
    }

    pub fn apply_request(
        &mut self,
        battle: &mut TacticalBattle,
        request: TacticalControlRequest,
    ) -> Result<(), shared::TacticalControlError> {
        self.0.get_mut().apply_request(battle, request)
    }

    pub fn attack_move_armed(&self) -> bool {
        self.0.borrow().attack_move_armed()
    }

    /// Control groups after reconciling with the battle, so routed or lost
    /// units never appear in a group the HUD can recall.
    pub fn control_groups(&self, battle: &TacticalBattle) -> Vec<(u8, Vec<String>)> {
        let mut controls = self.0.borrow_mut();
        controls.reconcile_with_battle(battle);
        controls.assigned_control_groups()
    }

    pub fn render_view(&self, battle: &TacticalBattle) -> RenderViewState {
        let mut controls = self.0.borrow_mut();
        controls.reconcile_with_battle(battle);
        controls.render_view(battle)
    }
}

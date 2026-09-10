use std::cell::RefCell;

use medieval_core::{BattleSide, TacticalBattle};
use medieval_renderer::RenderViewState;

mod shared {
    include!("../../shared/tactical_controls.rs");

    impl TacticalControls {
        pub(super) fn reconcile_with_battle(&mut self, battle: &TacticalBattle) {
            self.sync_with_battle(battle);
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

    pub fn render_view(&self, battle: &TacticalBattle) -> RenderViewState {
        let mut controls = self.0.borrow_mut();
        controls.reconcile_with_battle(battle);
        controls.render_view(battle)
    }
}

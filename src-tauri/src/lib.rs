use std::sync::Mutex;

use medieval_core::{CampaignState, new_campaign as fresh_campaign};
use tauri::State;

struct GameState(Mutex<CampaignState>);

impl Default for GameState {
    fn default() -> Self {
        Self(Mutex::new(fresh_campaign()))
    }
}

fn lock_campaign(
    state: &State<'_, GameState>,
) -> Result<std::sync::MutexGuard<'_, CampaignState>, String> {
    state
        .0
        .lock()
        .map_err(|_| "campaign state lock was poisoned".to_owned())
}

#[tauri::command]
fn campaign_state(state: State<'_, GameState>) -> Result<CampaignState, String> {
    Ok(lock_campaign(&state)?.clone())
}

#[tauri::command]
fn start_new_campaign(state: State<'_, GameState>) -> Result<CampaignState, String> {
    let mut campaign = lock_campaign(&state)?;
    *campaign = fresh_campaign();
    Ok(campaign.clone())
}

#[tauri::command]
fn end_turn(state: State<'_, GameState>) -> Result<CampaignState, String> {
    let mut campaign = lock_campaign(&state)?;
    campaign.end_turn();
    Ok(campaign.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(GameState::default())
        .invoke_handler(tauri::generate_handler![
            campaign_state,
            start_new_campaign,
            end_turn
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Medieval");
}

use std::sync::Mutex;

use medieval_core::{CampaignState, RecruitmentOption, UnitKind, new_campaign as fresh_campaign};
use tauri::State;

struct GameState(Mutex<CampaignState>);

impl Default for GameState {
    fn default() -> Self {
        Self(Mutex::new(fresh_campaign()))
    }
}

fn lock_campaign<'a>(
    state: &'a State<'_, GameState>,
) -> Result<std::sync::MutexGuard<'a, CampaignState>, String> {
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
fn legal_army_destinations(
    state: State<'_, GameState>,
    army_id: String,
) -> Result<Vec<String>, String> {
    lock_campaign(&state)?
        .legal_destinations(&army_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn move_army(
    state: State<'_, GameState>,
    army_id: String,
    destination: String,
) -> Result<CampaignState, String> {
    let mut campaign = lock_campaign(&state)?;
    campaign
        .move_army(&army_id, &destination)
        .map_err(|error| error.to_string())?;
    Ok(campaign.clone())
}

#[tauri::command]
fn recruitment_options(
    state: State<'_, GameState>,
    province_id: String,
) -> Result<Vec<RecruitmentOption>, String> {
    lock_campaign(&state)?
        .recruitment_options(&province_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn queue_recruitment(
    state: State<'_, GameState>,
    province_id: String,
    unit: UnitKind,
) -> Result<CampaignState, String> {
    let mut campaign = lock_campaign(&state)?;
    campaign
        .queue_recruitment(&province_id, unit)
        .map_err(|error| error.to_string())?;
    Ok(campaign.clone())
}

#[tauri::command]
fn resolve_pending_battle(state: State<'_, GameState>, seed: u64) -> Result<CampaignState, String> {
    let mut campaign = lock_campaign(&state)?;
    campaign
        .resolve_pending_battle(seed)
        .map_err(|error| error.to_string())?;
    Ok(campaign.clone())
}

#[tauri::command]
fn end_turn(state: State<'_, GameState>) -> Result<CampaignState, String> {
    let mut campaign = lock_campaign(&state)?;
    campaign.end_turn().map_err(|error| error.to_string())?;
    Ok(campaign.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(GameState::default())
        .invoke_handler(tauri::generate_handler![
            campaign_state,
            start_new_campaign,
            legal_army_destinations,
            move_army,
            recruitment_options,
            queue_recruitment,
            resolve_pending_battle,
            end_turn
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Medieval");
}

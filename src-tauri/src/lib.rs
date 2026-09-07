use std::sync::{Mutex, MutexGuard};

use medieval_core::{CampaignState, RecruitmentOption, UnitKind, new_campaign as fresh_campaign};
use tauri::State;

struct GameSession {
    campaign: CampaignState,
    player_faction: String,
}

impl Default for GameSession {
    fn default() -> Self {
        Self {
            campaign: fresh_campaign(),
            player_faction: "england".to_owned(),
        }
    }
}

struct GameState(Mutex<GameSession>);

impl Default for GameState {
    fn default() -> Self {
        Self(Mutex::new(GameSession::default()))
    }
}

fn lock_session<'a>(
    state: &'a State<'_, GameState>,
) -> Result<MutexGuard<'a, GameSession>, String> {
    state
        .0
        .lock()
        .map_err(|_| "campaign state lock was poisoned".to_owned())
}

fn ensure_campaign_running(session: &GameSession) -> Result<(), String> {
    if let Some(winner) = session.campaign.winner() {
        return Err(format!("campaign is over; {winner} has already won"));
    }
    Ok(())
}

#[tauri::command]
fn campaign_state(state: State<'_, GameState>) -> Result<CampaignState, String> {
    Ok(lock_session(&state)?.campaign.clone())
}

#[tauri::command]
fn campaign_player_faction(state: State<'_, GameState>) -> Result<String, String> {
    Ok(lock_session(&state)?.player_faction.clone())
}

#[tauri::command]
fn campaign_winner(state: State<'_, GameState>) -> Result<Option<String>, String> {
    Ok(lock_session(&state)?.campaign.winner())
}

#[tauri::command]
fn start_new_campaign(
    state: State<'_, GameState>,
    player_faction: String,
) -> Result<CampaignState, String> {
    let mut session = lock_session(&state)?;
    let mut campaign = fresh_campaign();
    campaign
        .select_player_faction(&player_faction)
        .map_err(|error| error.to_string())?;
    session.campaign = campaign;
    session.player_faction = player_faction;
    Ok(session.campaign.clone())
}

#[tauri::command]
fn legal_army_destinations(
    state: State<'_, GameState>,
    army_id: String,
) -> Result<Vec<String>, String> {
    let session = lock_session(&state)?;
    ensure_campaign_running(&session)?;
    session
        .campaign
        .legal_destinations(&army_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn move_army(
    state: State<'_, GameState>,
    army_id: String,
    destination: String,
) -> Result<CampaignState, String> {
    let mut session = lock_session(&state)?;
    ensure_campaign_running(&session)?;
    session
        .campaign
        .move_army(&army_id, &destination)
        .map_err(|error| error.to_string())?;
    Ok(session.campaign.clone())
}

#[tauri::command]
fn recruitment_options(
    state: State<'_, GameState>,
    province_id: String,
) -> Result<Vec<RecruitmentOption>, String> {
    let session = lock_session(&state)?;
    ensure_campaign_running(&session)?;
    session
        .campaign
        .recruitment_options(&province_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn queue_recruitment(
    state: State<'_, GameState>,
    province_id: String,
    unit: UnitKind,
) -> Result<CampaignState, String> {
    let mut session = lock_session(&state)?;
    ensure_campaign_running(&session)?;
    session
        .campaign
        .queue_recruitment(&province_id, unit)
        .map_err(|error| error.to_string())?;
    Ok(session.campaign.clone())
}

#[tauri::command]
fn resolve_pending_battle(state: State<'_, GameState>, seed: u64) -> Result<CampaignState, String> {
    let mut session = lock_session(&state)?;
    ensure_campaign_running(&session)?;
    session
        .campaign
        .resolve_pending_battle(seed)
        .map_err(|error| error.to_string())?;
    Ok(session.campaign.clone())
}

#[tauri::command]
fn end_player_turn(state: State<'_, GameState>, seed: u64) -> Result<CampaignState, String> {
    let mut session = lock_session(&state)?;
    ensure_campaign_running(&session)?;

    if session.campaign.active_faction != session.player_faction {
        return Err("the deterministic opponent must finish before the player can end another turn"
            .to_owned());
    }

    session
        .campaign
        .end_turn()
        .map_err(|error| error.to_string())?;

    if session.campaign.winner().is_none() {
        let player_faction = session.player_faction.clone();
        session
            .campaign
            .play_ai_turn(&player_faction, seed)
            .map_err(|error| error.to_string())?;
    }

    Ok(session.campaign.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(GameState::default())
        .invoke_handler(tauri::generate_handler![
            campaign_state,
            campaign_player_faction,
            campaign_winner,
            start_new_campaign,
            legal_army_destinations,
            move_army,
            recruitment_options,
            queue_recruitment,
            resolve_pending_battle,
            end_player_turn
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Medieval");
}
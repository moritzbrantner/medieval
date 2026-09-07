use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use medieval_core::{CampaignState, RecruitmentOption, UnitKind, new_campaign as fresh_campaign};
use medieval_save::{decode_campaign_save, encode_campaign_save};
use tauri::{AppHandle, Manager, State};

const SAVE_DIRECTORY: &str = "saves";
const MANUAL_SAVE_FILE: &str = "campaign-manual.json";
const AUTOSAVE_FILE: &str = "campaign-autosave.json";

#[derive(Clone)]
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

fn save_path(app: &AppHandle, file_name: &str) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve Medieval app data directory: {error}"))?
        .join(SAVE_DIRECTORY);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("failed to create Medieval save directory: {error}"))?;
    Ok(directory.join(file_name))
}

fn persist_session(app: &AppHandle, session: &GameSession, file_name: &str) -> Result<(), String> {
    let contents = encode_campaign_save(&session.campaign, &session.player_faction)
        .map_err(|error| error.to_string())?;
    let path = save_path(app, file_name)?;
    let temporary_path = path.with_extension("tmp");

    fs::write(&temporary_path, contents)
        .map_err(|error| format!("failed to write campaign save: {error}"))?;
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|error| format!("failed to replace existing campaign save: {error}"))?;
    }
    fs::rename(&temporary_path, &path)
        .map_err(|error| format!("failed to commit campaign save: {error}"))?;
    Ok(())
}

fn load_session(app: &AppHandle, file_name: &str, label: &str) -> Result<GameSession, String> {
    let path = save_path(app, file_name)?;
    let contents = fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("no {label} campaign save exists yet")
        } else {
            format!("failed to read {label} campaign save: {error}")
        }
    })?;
    let save = decode_campaign_save(&contents).map_err(|error| error.to_string())?;
    Ok(GameSession {
        campaign: save.campaign,
        player_faction: save.player_faction,
    })
}

fn replace_session_from_save(
    app: &AppHandle,
    state: &State<'_, GameState>,
    file_name: &str,
    label: &str,
) -> Result<CampaignState, String> {
    let loaded = load_session(app, file_name, label)?;
    let campaign = loaded.campaign.clone();
    *lock_session(state)? = loaded;
    Ok(campaign)
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
    app: AppHandle,
    state: State<'_, GameState>,
    player_faction: String,
) -> Result<CampaignState, String> {
    let mut campaign = fresh_campaign();
    campaign
        .select_player_faction(&player_faction)
        .map_err(|error| error.to_string())?;
    let next_session = GameSession {
        campaign,
        player_faction,
    };

    let mut session = lock_session(&state)?;
    persist_session(&app, &next_session, AUTOSAVE_FILE)?;
    *session = next_session;
    Ok(session.campaign.clone())
}

#[tauri::command]
fn save_campaign(app: AppHandle, state: State<'_, GameState>) -> Result<(), String> {
    let session = lock_session(&state)?;
    persist_session(&app, &session, MANUAL_SAVE_FILE)
}

#[tauri::command]
fn load_manual_campaign(
    app: AppHandle,
    state: State<'_, GameState>,
) -> Result<CampaignState, String> {
    replace_session_from_save(&app, &state, MANUAL_SAVE_FILE, "manual")
}

#[tauri::command]
fn load_autosave_campaign(
    app: AppHandle,
    state: State<'_, GameState>,
) -> Result<CampaignState, String> {
    replace_session_from_save(&app, &state, AUTOSAVE_FILE, "autosave")
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
fn resolve_pending_battle(
    app: AppHandle,
    state: State<'_, GameState>,
    seed: u64,
) -> Result<CampaignState, String> {
    let mut session = lock_session(&state)?;
    ensure_campaign_running(&session)?;

    let mut next_session = session.clone();
    next_session
        .campaign
        .resolve_pending_battle(seed)
        .map_err(|error| error.to_string())?;
    if next_session.campaign.winner().is_some() {
        persist_session(&app, &next_session, AUTOSAVE_FILE)?;
    }

    *session = next_session;
    Ok(session.campaign.clone())
}

#[tauri::command]
fn end_player_turn(
    app: AppHandle,
    state: State<'_, GameState>,
    seed: u64,
) -> Result<CampaignState, String> {
    let mut session = lock_session(&state)?;
    ensure_campaign_running(&session)?;

    if session.campaign.active_faction != session.player_faction {
        return Err(
            "the deterministic opponent must finish before the player can end another turn"
                .to_owned(),
        );
    }

    let mut next_session = session.clone();
    next_session
        .campaign
        .end_turn()
        .map_err(|error| error.to_string())?;

    if next_session.campaign.winner().is_none() {
        let player_faction = next_session.player_faction.clone();
        next_session
            .campaign
            .play_ai_turn(&player_faction, seed)
            .map_err(|error| error.to_string())?;
    }

    persist_session(&app, &next_session, AUTOSAVE_FILE)?;
    *session = next_session;
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
            save_campaign,
            load_manual_campaign,
            load_autosave_campaign,
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

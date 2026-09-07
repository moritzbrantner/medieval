use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use medieval_core::{
    CampaignSave, CampaignState, RecruitmentOption, UnitKind, new_campaign as fresh_campaign,
};
use tauri::{AppHandle, Manager, State};

const SAVE_FILE_NAME: &str = "campaign-save.json";

#[derive(Clone, Debug, PartialEq, Eq)]
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

fn campaign_save_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join(SAVE_FILE_NAME))
        .map_err(|error| format!("could not resolve the Medieval save directory: {error}"))
}

fn save_session_to_path(session: &GameSession, path: &Path) -> Result<(), String> {
    let document =
        CampaignSave::from_campaign(session.campaign.clone(), session.player_faction.clone())
            .map_err(|error| error.to_string())?
            .to_json()
            .map_err(|error| error.to_string())?;
    write_save_document(path, &document)
}

fn load_session_from_path(path: &Path) -> Result<GameSession, String> {
    let document = fs::read_to_string(path)
        .map_err(|error| format!("could not read campaign save {}: {error}", path.display()))?;
    let save = CampaignSave::from_json(&document).map_err(|error| error.to_string())?;
    Ok(GameSession {
        campaign: save.campaign,
        player_faction: save.player_faction,
    })
}

fn load_into_session(session: &mut GameSession, path: &Path) -> Result<(), String> {
    let loaded = load_session_from_path(path)?;
    *session = loaded;
    Ok(())
}

fn write_save_document(path: &Path, document: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "campaign save path has no parent directory".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "could not create campaign save directory {}: {error}",
            parent.display()
        )
    })?;

    let temporary = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    fs::write(&temporary, document).map_err(|error| {
        format!(
            "could not write temporary campaign save {}: {error}",
            temporary.display()
        )
    })?;

    let had_existing = path.exists();
    if had_existing {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| {
                format!(
                    "could not clear previous campaign save backup {}: {error}",
                    backup.display()
                )
            })?;
        }
        fs::rename(path, &backup).map_err(|error| {
            format!(
                "could not stage previous campaign save {}: {error}",
                path.display()
            )
        })?;
    }

    if let Err(error) = fs::rename(&temporary, path) {
        if had_existing {
            let _ = fs::rename(&backup, path);
        }
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "could not install campaign save {}: {error}",
            path.display()
        ));
    }

    if had_existing {
        let _ = fs::remove_file(&backup);
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
fn save_campaign(app: AppHandle, state: State<'_, GameState>) -> Result<(), String> {
    let path = campaign_save_path(&app)?;
    save_session_to_path(&lock_session(&state)?, &path)
}

#[tauri::command]
fn load_campaign(app: AppHandle, state: State<'_, GameState>) -> Result<CampaignState, String> {
    let path = campaign_save_path(&app)?;
    let mut session = lock_session(&state)?;
    load_into_session(&mut session, &path)?;
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

    let mut next = session.clone();
    next.campaign
        .end_turn()
        .map_err(|error| error.to_string())?;

    if next.campaign.winner().is_none() {
        let player_faction = next.player_faction.clone();
        next.campaign
            .play_ai_turn(&player_faction, seed)
            .map_err(|error| error.to_string())?;
    }

    let path = campaign_save_path(&app)?;
    save_session_to_path(&next, &path)?;
    *session = next;
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
            load_campaign,
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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_save_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("medieval-save-test-{}-{nonce}", std::process::id()))
            .join(name)
    }

    fn remove_test_directory(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn session_save_round_trip_preserves_campaign_and_player_faction() {
        let path = test_save_path("campaign-save.json");
        let mut session = GameSession::default();
        session
            .campaign
            .move_army("england-main", "wessex")
            .unwrap();

        save_session_to_path(&session, &path).unwrap();
        let loaded = load_session_from_path(&path).unwrap();

        assert_eq!(loaded, session);
        remove_test_directory(&path);
    }

    #[test]
    fn save_creates_parent_directories_and_leaves_no_staging_files() {
        let path = test_save_path("nested/campaign-save.json");

        save_session_to_path(&GameSession::default(), &path).unwrap();

        assert!(path.is_file());
        assert!(!path.with_extension("json.tmp").exists());
        assert!(!path.with_extension("json.bak").exists());
        remove_test_directory(&path);
    }

    #[test]
    fn repeated_save_replaces_the_previous_document() {
        let path = test_save_path("campaign-save.json");
        let first = GameSession::default();
        save_session_to_path(&first, &path).unwrap();

        let mut second = first.clone();
        second.campaign.end_turn().unwrap();
        save_session_to_path(&second, &path).unwrap();

        assert_eq!(load_session_from_path(&path).unwrap(), second);
        remove_test_directory(&path);
    }

    #[test]
    fn missing_save_fails_clearly() {
        let path = test_save_path("missing.json");
        let error = load_session_from_path(&path).unwrap_err();

        assert!(error.contains("could not read campaign save"));
        remove_test_directory(&path);
    }

    #[test]
    fn corrupt_save_does_not_replace_the_live_session() {
        let path = test_save_path("campaign-save.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{broken").unwrap();
        let mut session = GameSession::default();
        let before = session.clone();

        assert!(load_into_session(&mut session, &path).is_err());
        assert_eq!(session, before);
        remove_test_directory(&path);
    }

    #[test]
    fn unsupported_save_version_does_not_replace_the_live_session() {
        let path = test_save_path("campaign-save.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"schemaVersion":99,"futureShape":true}"#).unwrap();
        let mut session = GameSession::default();
        let before = session.clone();

        let error = load_into_session(&mut session, &path).unwrap_err();
        assert!(error.contains("unsupported campaign save schema version 99"));
        assert_eq!(session, before);
        remove_test_directory(&path);
    }

    #[test]
    fn app_save_filename_is_fixed_and_not_user_supplied() {
        assert_eq!(SAVE_FILE_NAME, "campaign-save.json");
        assert!(!SAVE_FILE_NAME.contains('/'));
        assert!(!SAVE_FILE_NAME.contains('\\'));
    }
}

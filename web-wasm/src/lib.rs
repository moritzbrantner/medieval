use std::{
    cell::{Cell, RefCell},
    slice, str,
};

use medieval_core::{
    CampaignSave, CampaignState, RecruitmentOption, UnitKind, new_campaign as fresh_campaign,
};
use serde::Serialize;

const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

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

thread_local! {
    static SESSION: RefCell<GameSession> = RefCell::new(GameSession::default());
    static RESPONSE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static RESPONSE_OK: Cell<u32> = const { Cell::new(1) };
}

fn ensure_campaign_running(session: &GameSession) -> Result<(), String> {
    if let Some(winner) = session.campaign.winner() {
        return Err(format!("campaign is over; {winner} has already won"));
    }
    Ok(())
}

fn write_response(ok: bool, bytes: Vec<u8>) {
    RESPONSE.with(|response| *response.borrow_mut() = bytes);
    RESPONSE_OK.with(|response_ok| response_ok.set(u32::from(ok)));
}

fn respond<T: Serialize>(result: Result<T, String>) {
    match result {
        Ok(value) => match serde_json::to_vec(&value) {
            Ok(bytes) => write_response(true, bytes),
            Err(error) => write_response(false, error.to_string().into_bytes()),
        },
        Err(error) => write_response(false, error.into_bytes()),
    }
}

unsafe fn read_input(pointer: *const u8, length: usize) -> Result<String, String> {
    if length == 0 {
        return Ok(String::new());
    }
    if pointer.is_null() {
        return Err("WASM input pointer was null".to_owned());
    }

    let bytes = unsafe { slice::from_raw_parts(pointer, length) };
    str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| format!("WASM input was not UTF-8: {error}"))
}

fn decode_unit(value: &str) -> Result<UnitKind, String> {
    let json = serde_json::to_string(value).map_err(|error| error.to_string())?;
    serde_json::from_str(&json).map_err(|_| format!("unsupported unit kind {value}"))
}

fn decode_seed(seed: f64) -> Result<u64, String> {
    if !seed.is_finite() || seed < 0.0 || seed.fract() != 0.0 || seed > MAX_SAFE_INTEGER {
        return Err("seed must be a non-negative JavaScript safe integer".to_owned());
    }
    Ok(seed as u64)
}

fn campaign_state() -> Result<CampaignState, String> {
    SESSION.with(|session| Ok(session.borrow().campaign.clone()))
}

fn campaign_player_faction() -> Result<String, String> {
    SESSION.with(|session| Ok(session.borrow().player_faction.clone()))
}

fn campaign_winner() -> Result<Option<String>, String> {
    SESSION.with(|session| Ok(session.borrow().campaign.winner()))
}

fn start_new_campaign(player_faction: String) -> Result<CampaignState, String> {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        let mut campaign = fresh_campaign();
        campaign
            .select_player_faction(&player_faction)
            .map_err(|error| error.to_string())?;
        session.campaign = campaign;
        session.player_faction = player_faction;
        Ok(session.campaign.clone())
    })
}

fn legal_army_destinations(army_id: &str) -> Result<Vec<String>, String> {
    SESSION.with(|session| {
        let session = session.borrow();
        ensure_campaign_running(&session)?;
        session
            .campaign
            .legal_destinations(army_id)
            .map_err(|error| error.to_string())
    })
}

fn move_army(army_id: &str, destination: &str) -> Result<CampaignState, String> {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        ensure_campaign_running(&session)?;
        session
            .campaign
            .move_army(army_id, destination)
            .map_err(|error| error.to_string())?;
        Ok(session.campaign.clone())
    })
}

fn recruitment_options(province_id: &str) -> Result<Vec<RecruitmentOption>, String> {
    SESSION.with(|session| {
        let session = session.borrow();
        ensure_campaign_running(&session)?;
        session
            .campaign
            .recruitment_options(province_id)
            .map_err(|error| error.to_string())
    })
}

fn queue_recruitment(province_id: &str, unit: UnitKind) -> Result<CampaignState, String> {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        ensure_campaign_running(&session)?;
        session
            .campaign
            .queue_recruitment(province_id, unit)
            .map_err(|error| error.to_string())?;
        Ok(session.campaign.clone())
    })
}

fn resolve_pending_battle(seed: u64) -> Result<CampaignState, String> {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
        ensure_campaign_running(&session)?;
        session
            .campaign
            .resolve_pending_battle(seed)
            .map_err(|error| error.to_string())?;
        Ok(session.campaign.clone())
    })
}

fn end_player_turn(seed: u64) -> Result<CampaignState, String> {
    SESSION.with(|session| {
        let mut session = session.borrow_mut();
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

        *session = next;
        Ok(session.campaign.clone())
    })
}

fn export_save() -> Result<String, String> {
    SESSION.with(|session| {
        let session = session.borrow();
        CampaignSave::from_campaign(session.campaign.clone(), session.player_faction.clone())
            .map_err(|error| error.to_string())?
            .to_json()
            .map_err(|error| error.to_string())
    })
}

fn import_save(document: &str) -> Result<CampaignState, String> {
    let save = CampaignSave::from_json(document).map_err(|error| error.to_string())?;
    SESSION.with(|session| {
        let campaign = save.campaign.clone();
        *session.borrow_mut() = GameSession {
            campaign: save.campaign,
            player_faction: save.player_faction,
        };
        Ok(campaign)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_alloc(length: usize) -> *mut u8 {
    let buffer = vec![0_u8; length].into_boxed_slice();
    Box::into_raw(buffer).cast::<u8>()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn medieval_dealloc(pointer: *mut u8, length: usize) {
    if pointer.is_null() {
        return;
    }

    let slice_pointer = std::ptr::slice_from_raw_parts_mut(pointer, length);
    unsafe { drop(Box::from_raw(slice_pointer)) };
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_response_ptr() -> *const u8 {
    RESPONSE.with(|response| response.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_response_len() -> usize {
    RESPONSE.with(|response| response.borrow().len())
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_response_ok() -> u32 {
    RESPONSE_OK.with(Cell::get)
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_campaign_state() {
    respond(campaign_state());
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_campaign_player_faction() {
    respond(campaign_player_faction());
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_campaign_winner() {
    respond(campaign_winner());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn medieval_start_new_campaign(pointer: *const u8, length: usize) {
    let player_faction = unsafe { read_input(pointer, length) };
    respond(player_faction.and_then(start_new_campaign));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn medieval_legal_army_destinations(pointer: *const u8, length: usize) {
    let army_id = unsafe { read_input(pointer, length) };
    respond(army_id.and_then(|army_id| legal_army_destinations(&army_id)));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn medieval_move_army(
    army_pointer: *const u8,
    army_length: usize,
    destination_pointer: *const u8,
    destination_length: usize,
) {
    let result = unsafe { read_input(army_pointer, army_length) }.and_then(|army_id| {
        unsafe { read_input(destination_pointer, destination_length) }
            .and_then(|destination| move_army(&army_id, &destination))
    });
    respond(result);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn medieval_recruitment_options(pointer: *const u8, length: usize) {
    let province_id = unsafe { read_input(pointer, length) };
    respond(province_id.and_then(|province_id| recruitment_options(&province_id)));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn medieval_queue_recruitment(
    province_pointer: *const u8,
    province_length: usize,
    unit_pointer: *const u8,
    unit_length: usize,
) {
    let result = unsafe { read_input(province_pointer, province_length) }.and_then(|province_id| {
        unsafe { read_input(unit_pointer, unit_length) }
            .and_then(|unit| decode_unit(&unit))
            .and_then(|unit| queue_recruitment(&province_id, unit))
    });
    respond(result);
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_resolve_pending_battle(seed: f64) {
    respond(decode_seed(seed).and_then(resolve_pending_battle));
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_end_player_turn(seed: f64) {
    respond(decode_seed(seed).and_then(end_player_turn));
}

#[unsafe(no_mangle)]
pub extern "C" fn medieval_export_save() {
    respond(export_save());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn medieval_import_save(pointer: *const u8, length: usize) {
    let document = unsafe { read_input(pointer, length) };
    respond(document.and_then(|document| import_save(&document)));
}

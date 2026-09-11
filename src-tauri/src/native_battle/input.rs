use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use medieval_core::{BattlePoint, BattleSide, TacticalBattle};
use medieval_renderer::BattleRenderSnapshot;
use serde::Deserialize;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use tauri::Listener;
use tauri::Window;

use super::controls::{TacticalControlError, TacticalControlRequest, TacticalControls};
use super::{NativeBattleSession, SharedError, SharedSession};

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub const BROWSER_INPUT_EVENT: &str = "medieval:tactical-input";

const CAMERA_ZOOM_FACTOR: f32 = 1.15;
const DRAG_THRESHOLD_NORMALIZED: f64 = 0.02;
const CLICK_RADIUS_PX: f64 = 34.0;

#[derive(Copy, Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InputModifiers {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub control: bool,
}

#[cfg(any(target_os = "windows", target_os = "macos", test))]
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInputEnvelope {
    pub sequence: u64,
    #[serde(flatten)]
    pub input: DesktopInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DesktopInput {
    KeyDown {
        code: String,
        #[serde(flatten)]
        modifiers: InputModifiers,
    },
    PointerDown {
        button: PointerButton,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        #[serde(flatten)]
        modifiers: InputModifiers,
    },
    PointerUp {
        button: PointerButton,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        #[serde(flatten)]
        modifiers: InputModifiers,
    },
    Wheel {
        delta_y: f64,
    },
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PointerButton {
    Primary,
    Secondary,
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct PointerSample {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl PointerSample {
    fn new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, String> {
        if !x.is_finite()
            || !y.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return Err(
                "desktop tactical pointer coordinates must be finite with a positive viewport"
                    .to_owned(),
            );
        }
        Ok(Self {
            x: x.clamp(0.0, width),
            y: y.clamp(0.0, height),
            width,
            height,
        })
    }

    fn normalized(self) -> (f64, f64) {
        (self.x / self.width, self.y / self.height)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
struct PointerGesture {
    button: PointerButton,
    start: PointerSample,
    modifiers: InputModifiers,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesktopInputState {
    pointer_gesture: Option<PointerGesture>,
    #[cfg(any(target_os = "windows", target_os = "macos", test))]
    next_browser_sequence: u64,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct InputOutcome {
    pub close_requested: bool,
}

impl DesktopInputState {
    pub fn reset_browser_sequence(&mut self) {
        self.pointer_gesture = None;
        #[cfg(any(target_os = "windows", target_os = "macos", test))]
        {
            self.next_browser_sequence = 1;
        }
    }

    #[cfg(any(target_os = "windows", target_os = "macos", test))]
    fn apply_browser(
        &mut self,
        battle: &mut TacticalBattle,
        controls: &mut TacticalControls,
        player_side: BattleSide,
        envelope: BrowserInputEnvelope,
    ) -> Result<InputOutcome, String> {
        if envelope.sequence != self.next_browser_sequence {
            return Err(format!(
                "desktop tactical input sequence mismatch: expected {}, received {}",
                self.next_browser_sequence, envelope.sequence
            ));
        }
        self.next_browser_sequence = self.next_browser_sequence.saturating_add(1);
        self.apply(battle, controls, player_side, envelope.input)
    }

    pub fn apply(
        &mut self,
        battle: &mut TacticalBattle,
        controls: &mut TacticalControls,
        player_side: BattleSide,
        input: DesktopInput,
    ) -> Result<InputOutcome, String> {
        match input {
            DesktopInput::KeyDown { code, modifiers } => {
                self.apply_key(battle, controls, code.as_str(), modifiers)
            }
            DesktopInput::PointerDown {
                button,
                x,
                y,
                width,
                height,
                modifiers,
            } => self.pointer_down(
                battle,
                controls,
                button,
                PointerSample::new(x, y, width, height)?,
                modifiers,
            ),
            DesktopInput::PointerUp {
                button,
                x,
                y,
                width,
                height,
                modifiers,
            } => self.pointer_up(
                battle,
                controls,
                player_side,
                button,
                PointerSample::new(x, y, width, height)?,
                modifiers,
            ),
            DesktopInput::Wheel { delta_y } => {
                if !delta_y.is_finite() {
                    return Err("desktop tactical wheel delta must be finite".to_owned());
                }
                if delta_y != 0.0 {
                    apply_control(
                        battle,
                        controls,
                        TacticalControlRequest {
                            kind: "zoomCamera".to_owned(),
                            factor: Some(if delta_y < 0.0 {
                                CAMERA_ZOOM_FACTOR
                            } else {
                                1.0 / CAMERA_ZOOM_FACTOR
                            }),
                            ..TacticalControlRequest::default()
                        },
                    )?;
                }
                Ok(InputOutcome::default())
            }
        }
    }

    fn apply_key(
        &mut self,
        battle: &mut TacticalBattle,
        controls: &mut TacticalControls,
        code: &str,
        modifiers: InputModifiers,
    ) -> Result<InputOutcome, String> {
        if code == "Escape" {
            self.pointer_gesture = None;
            return Ok(InputOutcome {
                close_requested: true,
            });
        }

        let battlefield = battle.battlefield();
        let camera = controls.render_view(battle).camera;
        let step = camera.pan_step_mm(battlefield);
        let request = match code {
            "ArrowLeft" => Some(pan_request(-step, 0.0)),
            "ArrowRight" => Some(pan_request(step, 0.0)),
            "ArrowUp" => Some(pan_request(0.0, -step)),
            "ArrowDown" => Some(pan_request(0.0, step)),
            "Equal" | "NumpadAdd" => Some(TacticalControlRequest {
                kind: "zoomCamera".to_owned(),
                factor: Some(CAMERA_ZOOM_FACTOR),
                ..TacticalControlRequest::default()
            }),
            "Minus" | "NumpadSubtract" => Some(TacticalControlRequest {
                kind: "zoomCamera".to_owned(),
                factor: Some(1.0 / CAMERA_ZOOM_FACTOR),
                ..TacticalControlRequest::default()
            }),
            "Space" => Some(TacticalControlRequest {
                kind: "fitCamera".to_owned(),
                ..TacticalControlRequest::default()
            }),
            "KeyS" => Some(TacticalControlRequest {
                kind: "stopSelected".to_owned(),
                ..TacticalControlRequest::default()
            }),
            _ => digit_from_code(code).map(|group| TacticalControlRequest {
                kind: if modifiers.control {
                    "assignControlGroup"
                } else {
                    "recallControlGroup"
                }
                .to_owned(),
                group: Some(group),
                additive: (!modifiers.control).then_some(modifiers.shift),
                ..TacticalControlRequest::default()
            }),
        };
        if let Some(request) = request {
            apply_control_ignoring_empty_selection(battle, controls, request)?;
        }
        Ok(InputOutcome::default())
    }

    fn pointer_down(
        &mut self,
        battle: &mut TacticalBattle,
        controls: &mut TacticalControls,
        button: PointerButton,
        sample: PointerSample,
        modifiers: InputModifiers,
    ) -> Result<InputOutcome, String> {
        self.pointer_gesture = Some(PointerGesture {
            button,
            start: sample,
            modifiers,
        });
        if button == PointerButton::Secondary {
            apply_control(
                battle,
                controls,
                TacticalControlRequest {
                    kind: "setOrderPreview".to_owned(),
                    active: Some(true),
                    ..TacticalControlRequest::default()
                },
            )?;
        }
        Ok(InputOutcome::default())
    }

    fn pointer_up(
        &mut self,
        battle: &mut TacticalBattle,
        controls: &mut TacticalControls,
        player_side: BattleSide,
        button: PointerButton,
        sample: PointerSample,
        modifiers: InputModifiers,
    ) -> Result<InputOutcome, String> {
        let gesture = self.pointer_gesture.take();
        if button == PointerButton::Secondary {
            apply_control(
                battle,
                controls,
                TacticalControlRequest {
                    kind: "setOrderPreview".to_owned(),
                    active: Some(false),
                    ..TacticalControlRequest::default()
                },
            )?;
        }
        let Some(gesture) = gesture.filter(|gesture| gesture.button == button) else {
            return Ok(InputOutcome::default());
        };
        let effective_modifiers = if gesture.modifiers == modifiers {
            modifiers
        } else {
            gesture.modifiers
        };
        match button {
            PointerButton::Primary => self.finish_primary_pointer(
                battle,
                controls,
                player_side,
                gesture.start,
                sample,
                effective_modifiers,
            )?,
            PointerButton::Secondary => {
                self.finish_secondary_pointer(battle, controls, player_side, sample)?;
            }
        }
        Ok(InputOutcome::default())
    }

    fn finish_primary_pointer(
        &mut self,
        battle: &mut TacticalBattle,
        controls: &mut TacticalControls,
        player_side: BattleSide,
        start: PointerSample,
        end: PointerSample,
        modifiers: InputModifiers,
    ) -> Result<(), String> {
        let (start_x, start_y) = start.normalized();
        let (end_x, end_y) = end.normalized();
        let dragged =
            (end_x - start_x).abs().max((end_y - start_y).abs()) >= DRAG_THRESHOLD_NORMALIZED;
        let kind = selection_kind(modifiers);
        let unit_ids = if dragged {
            units_in_viewport_rect(battle, controls, player_side, start, end)
        } else {
            nearest_unit_at_pointer(battle, controls, end, |unit| {
                unit.side() == player_side && !unit.is_routed() && !unit.is_destroyed()
            })
            .into_iter()
            .collect()
        };
        if unit_ids.is_empty() {
            if kind == "selectReplace" {
                apply_control(
                    battle,
                    controls,
                    TacticalControlRequest {
                        kind: "clearSelection".to_owned(),
                        ..TacticalControlRequest::default()
                    },
                )?;
            }
            return Ok(());
        }
        apply_control(
            battle,
            controls,
            TacticalControlRequest {
                kind: kind.to_owned(),
                unit_ids: Some(unit_ids),
                ..TacticalControlRequest::default()
            },
        )
    }

    fn finish_secondary_pointer(
        &mut self,
        battle: &mut TacticalBattle,
        controls: &mut TacticalControls,
        player_side: BattleSide,
        sample: PointerSample,
    ) -> Result<(), String> {
        if let Some(target_unit_id) = nearest_unit_at_pointer(battle, controls, sample, |unit| {
            unit.side() != player_side && !unit.is_destroyed()
        }) {
            let result = apply_control(
                battle,
                controls,
                TacticalControlRequest {
                    kind: "engageSelected".to_owned(),
                    target_unit_id: Some(target_unit_id),
                    ..TacticalControlRequest::default()
                },
            );
            return match result {
                Err(error) if error == TacticalControlError::NoUnitsSelected.to_string() => Ok(()),
                other => other,
            };
        }
        let destination = viewport_to_world(battle, controls, sample)?;
        let result = apply_control(
            battle,
            controls,
            TacticalControlRequest {
                kind: "moveSelected".to_owned(),
                x_mm: Some(destination.x_mm),
                y_mm: Some(destination.y_mm),
                ..TacticalControlRequest::default()
            },
        );
        match result {
            Err(error) if error == TacticalControlError::NoUnitsSelected.to_string() => Ok(()),
            other => other,
        }
    }
}

fn pan_request(delta_x_mm: f32, delta_z_mm: f32) -> TacticalControlRequest {
    TacticalControlRequest {
        kind: "panCamera".to_owned(),
        delta_x_mm: Some(delta_x_mm),
        delta_y_mm: Some(delta_z_mm),
        ..TacticalControlRequest::default()
    }
}

fn selection_kind(modifiers: InputModifiers) -> &'static str {
    if modifiers.control {
        "selectToggle"
    } else if modifiers.shift {
        "selectAdd"
    } else {
        "selectReplace"
    }
}

fn digit_from_code(code: &str) -> Option<u8> {
    let suffix = code.strip_prefix("Digit")?;
    if suffix.len() != 1 {
        return None;
    }
    suffix.as_bytes()[0]
        .checked_sub(b'0')
        .filter(|digit| *digit <= 9)
}

fn viewport_to_world(
    battle: &TacticalBattle,
    controls: &TacticalControls,
    sample: PointerSample,
) -> Result<BattlePoint, String> {
    controls
        .render_view(battle)
        .camera
        .ground_point_from_viewport(
            battle.battlefield(),
            sample.x as f32,
            sample.y as f32,
            sample.width as f32,
            sample.height as f32,
        )
        .ok_or_else(|| "desktop tactical pointer ray does not intersect the battlefield".to_owned())
}

fn render_snapshot(battle: &TacticalBattle, controls: &TacticalControls) -> BattleRenderSnapshot {
    BattleRenderSnapshot::capture(battle, &controls.render_view(battle))
}

fn nearest_unit_at_pointer<F>(
    battle: &TacticalBattle,
    controls: &TacticalControls,
    sample: PointerSample,
    mut predicate: F,
) -> Option<String>
where
    F: FnMut(&medieval_core::TacticalUnit) -> bool,
{
    let snapshot = render_snapshot(battle, controls);
    snapshot
        .units
        .iter()
        .filter_map(|rendered| {
            let unit = battle.units().iter().find(|unit| unit.id() == rendered.unit_id)?;
            if !predicate(unit) {
                return None;
            }
            let [x, y] = snapshot.camera.project_world_point(
                snapshot.battlefield,
                rendered.interaction_anchor_mm(),
                sample.width as f32,
                sample.height as f32,
            )?;
            let dx = f64::from(x) - sample.x;
            let dy = f64::from(y) - sample.y;
            let distance_squared = dx * dx + dy * dy;
            (distance_squared <= CLICK_RADIUS_PX * CLICK_RADIUS_PX)
                .then_some((distance_squared, rendered.unit_id.as_str()))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0).then_with(|| left.1.cmp(right.1)))
        .map(|(_, unit_id)| unit_id.to_owned())
}

fn units_in_viewport_rect(
    battle: &TacticalBattle,
    controls: &TacticalControls,
    player_side: BattleSide,
    first: PointerSample,
    second: PointerSample,
) -> Vec<String> {
    let snapshot = render_snapshot(battle, controls);
    let min_x = first.x.min(second.x);
    let max_x = first.x.max(second.x);
    let min_y = first.y.min(second.y);
    let max_y = first.y.max(second.y);
    snapshot
        .units
        .iter()
        .filter(|rendered| {
            let Some(unit) = battle.units().iter().find(|unit| unit.id() == rendered.unit_id) else {
                return false;
            };
            if unit.side() != player_side || unit.is_routed() || unit.is_destroyed() {
                return false;
            }
            let Some([x, y]) = snapshot.camera.project_world_point(
                snapshot.battlefield,
                rendered.interaction_anchor_mm(),
                first.width as f32,
                first.height as f32,
            ) else {
                return false;
            };
            (min_x..=max_x).contains(&f64::from(x)) && (min_y..=max_y).contains(&f64::from(y))
        })
        .map(|unit| unit.unit_id.clone())
        .collect()
}

fn apply_control(
    battle: &mut TacticalBattle,
    controls: &mut TacticalControls,
    request: TacticalControlRequest,
) -> Result<(), String> {
    controls
        .apply_request(battle, request)
        .map_err(|error| error.to_string())
}

fn apply_control_ignoring_empty_selection(
    battle: &mut TacticalBattle,
    controls: &mut TacticalControls,
    request: TacticalControlRequest,
) -> Result<(), String> {
    match controls.apply_request(battle, request) {
        Ok(()) | Err(TacticalControlError::NoUnitsSelected) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn install_browser_input_listener(
    app: &tauri::App,
    window: Window,
    session: SharedSession,
    running: Arc<AtomicBool>,
    last_error: SharedError,
) {
    app.listen(BROWSER_INPUT_EVENT, move |event| {
        if !running.load(Ordering::Acquire) {
            return;
        }
        let result = (|| -> Result<InputOutcome, String> {
            let envelope = serde_json::from_str::<BrowserInputEnvelope>(event.payload())
                .map_err(|error| format!("invalid desktop tactical input payload: {error}"))?;
            let mut session = session
                .lock()
                .map_err(|_| "native tactical session lock was poisoned".to_owned())?;
            let NativeBattleSession {
                battle,
                controls,
                player_side,
                input,
            } = &mut *session;
            input.apply_browser(battle, controls, *player_side, envelope)
        })();
        match result {
            Ok(InputOutcome {
                close_requested: true,
            }) => {
                running.store(false, Ordering::Release);
                let _ = window.hide();
            }
            Ok(InputOutcome {
                close_requested: false,
            }) => {}
            Err(error) => {
                if let Ok(mut last_error) = last_error.lock() {
                    *last_error = Some(error);
                }
            }
        }
    });
}

#[cfg(target_os = "linux")]
pub fn install_linux_input(
    window: &Window,
    session: SharedSession,
    running: Arc<AtomicBool>,
    last_error: SharedError,
) -> Result<(), String> {
    use gdk::{EventMask, ModifierType, ScrollDirection, keys::constants};
    use gtk::{glib::Propagation, prelude::*};

    let gtk_window = window
        .gtk_window()
        .map_err(|error| format!("could not access tactical GTK window for input: {error}"))?;
    gtk_window.add_events(
        EventMask::KEY_PRESS_MASK
            | EventMask::BUTTON_PRESS_MASK
            | EventMask::BUTTON_RELEASE_MASK
            | EventMask::SCROLL_MASK,
    );

    let key_session = Arc::clone(&session);
    let key_window = window.clone();
    let key_running = Arc::clone(&running);
    let key_error = Arc::clone(&last_error);
    gtk_window.connect_key_press_event(move |_, event| {
        let key = event.keyval();
        let code = if key == constants::Left {
            Some("ArrowLeft")
        } else if key == constants::Right {
            Some("ArrowRight")
        } else if key == constants::Up {
            Some("ArrowUp")
        } else if key == constants::Down {
            Some("ArrowDown")
        } else if key == constants::space {
            Some("Space")
        } else if key == constants::Escape {
            Some("Escape")
        } else if key == constants::s || key == constants::S {
            Some("KeyS")
        } else if key == constants::plus || key == constants::equal || key == constants::KP_Add {
            Some("Equal")
        } else if key == constants::minus || key == constants::KP_Subtract {
            Some("Minus")
        } else if key == constants::_0 || key == constants::KP_0 {
            Some("Digit0")
        } else if key == constants::_1 || key == constants::KP_1 {
            Some("Digit1")
        } else if key == constants::_2 || key == constants::KP_2 {
            Some("Digit2")
        } else if key == constants::_3 || key == constants::KP_3 {
            Some("Digit3")
        } else if key == constants::_4 || key == constants::KP_4 {
            Some("Digit4")
        } else if key == constants::_5 || key == constants::KP_5 {
            Some("Digit5")
        } else if key == constants::_6 || key == constants::KP_6 {
            Some("Digit6")
        } else if key == constants::_7 || key == constants::KP_7 {
            Some("Digit7")
        } else if key == constants::_8 || key == constants::KP_8 {
            Some("Digit8")
        } else if key == constants::_9 || key == constants::KP_9 {
            Some("Digit9")
        } else {
            None
        };
        if let Some(code) = code {
            let outcome = apply_shared_input(
                &key_session,
                DesktopInput::KeyDown {
                    code: code.to_owned(),
                    modifiers: modifiers_from_gdk(event.state()),
                },
            );
            finish_native_input(outcome, &key_window, &key_running, &key_error);
            return Propagation::Stop;
        }
        Propagation::Proceed
    });

    let press_session = Arc::clone(&session);
    let press_window = window.clone();
    let press_running = Arc::clone(&running);
    let press_error = Arc::clone(&last_error);
    gtk_window.connect_button_press_event(move |widget, event| {
        let button = match event.button() {
            gdk::BUTTON_PRIMARY => Some(PointerButton::Primary),
            gdk::BUTTON_SECONDARY => Some(PointerButton::Secondary),
            _ => None,
        };
        let Some(button) = button else {
            return Propagation::Proceed;
        };
        let (x, y) = event.position();
        let outcome = apply_shared_input(
            &press_session,
            DesktopInput::PointerDown {
                button,
                x,
                y,
                width: f64::from(widget.allocated_width().max(1)),
                height: f64::from(widget.allocated_height().max(1)),
                modifiers: modifiers_from_gdk(event.state()),
            },
        );
        finish_native_input(outcome, &press_window, &press_running, &press_error);
        Propagation::Stop
    });

    let release_session = Arc::clone(&session);
    let release_window = window.clone();
    let release_running = Arc::clone(&running);
    let release_error = Arc::clone(&last_error);
    gtk_window.connect_button_release_event(move |widget, event| {
        let button = match event.button() {
            gdk::BUTTON_PRIMARY => Some(PointerButton::Primary),
            gdk::BUTTON_SECONDARY => Some(PointerButton::Secondary),
            _ => None,
        };
        let Some(button) = button else {
            return Propagation::Proceed;
        };
        let (x, y) = event.position();
        let outcome = apply_shared_input(
            &release_session,
            DesktopInput::PointerUp {
                button,
                x,
                y,
                width: f64::from(widget.allocated_width().max(1)),
                height: f64::from(widget.allocated_height().max(1)),
                modifiers: modifiers_from_gdk(event.state()),
            },
        );
        finish_native_input(outcome, &release_window, &release_running, &release_error);
        Propagation::Stop
    });

    let scroll_session = session;
    let scroll_window = window.clone();
    let scroll_running = running;
    let scroll_error = last_error;
    gtk_window.connect_scroll_event(move |_, event| {
        let delta_y = event
            .scroll_deltas()
            .map(|(_, delta_y)| delta_y)
            .or_else(|| {
                event.scroll_direction().map(|direction| match direction {
                    ScrollDirection::Up => -1.0,
                    ScrollDirection::Down => 1.0,
                    _ => 0.0,
                })
            })
            .unwrap_or(0.0);
        let outcome = apply_shared_input(&scroll_session, DesktopInput::Wheel { delta_y });
        finish_native_input(outcome, &scroll_window, &scroll_running, &scroll_error);
        Propagation::Stop
    });

    fn modifiers_from_gdk(state: ModifierType) -> InputModifiers {
        InputModifiers {
            shift: state.contains(ModifierType::SHIFT_MASK),
            control: state.contains(ModifierType::CONTROL_MASK),
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_shared_input(
    session: &SharedSession,
    input: DesktopInput,
) -> Result<InputOutcome, String> {
    let mut session = session
        .lock()
        .map_err(|_| "native tactical session lock was poisoned".to_owned())?;
    let NativeBattleSession {
        battle,
        controls,
        player_side,
        input: state,
    } = &mut *session;
    state.apply(battle, controls, *player_side, input)
}

#[cfg(target_os = "linux")]
fn finish_native_input(
    result: Result<InputOutcome, String>,
    window: &Window,
    running: &AtomicBool,
    last_error: &SharedError,
) {
    match result {
        Ok(InputOutcome {
            close_requested: true,
        }) => {
            running.store(false, Ordering::Release);
            let _ = window.hide();
        }
        Ok(InputOutcome {
            close_requested: false,
        }) => {}
        Err(error) => {
            if let Ok(mut last_error) = last_error.lock() {
                *last_error = Some(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use medieval_core::{FlatBattlefield, Formation, TacticalUnit};

    use super::*;

    fn unit(id: &str, side: BattleSide, x_mm: u32, y_mm: u32) -> TacticalUnit {
        TacticalUnit::new(
            id,
            side,
            80,
            BattlePoint::new(x_mm, y_mm),
            Formation::Line { files: 20 },
            1_000,
        )
    }

    fn session() -> NativeBattleSession {
        let battle = TacticalBattle::new(
            FlatBattlefield::new(100_000, 100_000),
            vec![
                unit("attacker-a", BattleSide::Attacker, 30_000, 50_000),
                unit("attacker-b", BattleSide::Attacker, 45_000, 60_000),
                unit("defender", BattleSide::Defender, 70_000, 50_000),
            ],
        )
        .unwrap();
        NativeBattleSession {
            controls: TacticalControls::new(&battle, BattleSide::Attacker),
            battle,
            player_side: BattleSide::Attacker,
            input: DesktopInputState::default(),
        }
    }

    fn pointer(x: f64, y: f64) -> PointerSample {
        PointerSample::new(x, y, 1_000.0, 1_000.0).unwrap()
    }

    fn pointer_for_unit(session: &NativeBattleSession, unit_id: &str) -> PointerSample {
        let snapshot = render_snapshot(&session.battle, &session.controls);
        let rendered = snapshot
            .units
            .iter()
            .find(|unit| unit.unit_id == unit_id)
            .unwrap();
        let [x, y] = snapshot
            .camera
            .project_world_point(
                snapshot.battlefield,
                rendered.interaction_anchor_mm(),
                1_000.0,
                1_000.0,
            )
            .unwrap();
        pointer(f64::from(x), f64::from(y))
    }

    fn pointer_for_ground(session: &NativeBattleSession, point: BattlePoint) -> PointerSample {
        let camera = session.controls.render_view(&session.battle).camera;
        let [x, y] = camera
            .project_ground_point(session.battle.battlefield(), point, 1_000.0, 1_000.0)
            .unwrap();
        pointer(f64::from(x), f64::from(y))
    }

    fn click(
        input: &mut DesktopInputState,
        session: &mut NativeBattleSession,
        button: PointerButton,
        sample: PointerSample,
    ) {
        for down in [true, false] {
            let event = if down {
                DesktopInput::PointerDown {
                    button,
                    x: sample.x,
                    y: sample.y,
                    width: sample.width,
                    height: sample.height,
                    modifiers: InputModifiers::default(),
                }
            } else {
                DesktopInput::PointerUp {
                    button,
                    x: sample.x,
                    y: sample.y,
                    width: sample.width,
                    height: sample.height,
                    modifiers: InputModifiers::default(),
                }
            };
            input
                .apply(
                    &mut session.battle,
                    &mut session.controls,
                    session.player_side,
                    event,
                )
                .unwrap();
        }
    }

    #[test]
    fn viewport_ray_round_trips_fitted_battlefield_space() {
        let session = session();
        let point = BattlePoint::new(42_000, 38_000);
        let sample = pointer_for_ground(&session, point);
        let round_trip = viewport_to_world(&session.battle, &session.controls, sample).unwrap();
        let dx = i64::from(round_trip.x_mm) - i64::from(point.x_mm);
        let dy = i64::from(round_trip.y_mm) - i64::from(point.y_mm);
        assert!(dx.abs() <= 1);
        assert!(dy.abs() <= 1);
    }

    #[test]
    fn click_and_drag_selection_follow_projected_unit_anchors() {
        let mut session = session();
        let mut input = DesktopInputState::default();
        let attacker_a = pointer_for_unit(&session, "attacker-a");
        click(&mut input, &mut session, PointerButton::Primary, attacker_a);
        let snapshot = render_snapshot(&session.battle, &session.controls);
        assert!(
            snapshot
                .units
                .iter()
                .find(|unit| unit.unit_id == "attacker-a")
                .unwrap()
                .selected
        );

        let first = pointer_for_unit(&session, "attacker-a");
        let second = pointer_for_unit(&session, "attacker-b");
        let start = PointerSample::new(
            first.x.min(second.x) - 20.0,
            first.y.min(second.y) - 20.0,
            1_000.0,
            1_000.0,
        )
        .unwrap();
        let end = PointerSample::new(
            first.x.max(second.x) + 20.0,
            first.y.max(second.y) + 20.0,
            1_000.0,
            1_000.0,
        )
        .unwrap();
        input
            .apply(
                &mut session.battle,
                &mut session.controls,
                session.player_side,
                DesktopInput::PointerDown {
                    button: PointerButton::Primary,
                    x: start.x,
                    y: start.y,
                    width: start.width,
                    height: start.height,
                    modifiers: InputModifiers::default(),
                },
            )
            .unwrap();
        input
            .apply(
                &mut session.battle,
                &mut session.controls,
                session.player_side,
                DesktopInput::PointerUp {
                    button: PointerButton::Primary,
                    x: end.x,
                    y: end.y,
                    width: end.width,
                    height: end.height,
                    modifiers: InputModifiers::default(),
                },
            )
            .unwrap();
        let snapshot = render_snapshot(&session.battle, &session.controls);
        assert!(snapshot.units.iter().find(|unit| unit.unit_id == "attacker-a").unwrap().selected);
        assert!(snapshot.units.iter().find(|unit| unit.unit_id == "attacker-b").unwrap().selected);
    }

    #[test]
    fn right_click_moves_or_engages_using_the_visible_projection() {
        let mut session = session();
        session
            .controls
            .apply_request(
                &mut session.battle,
                TacticalControlRequest {
                    kind: "selectReplace".to_owned(),
                    unit_ids: Some(vec!["attacker-a".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        let mut input = DesktopInputState::default();
        let destination = BattlePoint::new(55_000, 45_000);
        let ground = pointer_for_ground(&session, destination);
        click(&mut input, &mut session, PointerButton::Secondary, ground);
        assert_eq!(
            session
                .battle
                .units()
                .iter()
                .find(|unit| unit.id() == "attacker-a")
                .unwrap()
                .destination(),
            Some(destination)
        );

        let defender = pointer_for_unit(&session, "defender");
        click(&mut input, &mut session, PointerButton::Secondary, defender);
        assert_eq!(
            session
                .battle
                .units()
                .iter()
                .find(|unit| unit.id() == "attacker-a")
                .unwrap()
                .engagement_target(),
            Some("defender")
        );
    }

    #[test]
    fn keyboard_camera_groups_and_escape_map_to_semantic_controls() {
        let mut session = session();
        let mut input = DesktopInputState::default();
        let before = session.controls.render_view(&session.battle).camera;
        input
            .apply(
                &mut session.battle,
                &mut session.controls,
                session.player_side,
                DesktopInput::KeyDown {
                    code: "ArrowRight".to_owned(),
                    modifiers: InputModifiers::default(),
                },
            )
            .unwrap();
        assert!(
            session
                .controls
                .render_view(&session.battle)
                .camera
                .target_x_mm()
                > before.target_x_mm()
        );

        session
            .controls
            .apply_request(
                &mut session.battle,
                TacticalControlRequest {
                    kind: "selectReplace".to_owned(),
                    unit_ids: Some(vec!["attacker-a".to_owned()]),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        input
            .apply(
                &mut session.battle,
                &mut session.controls,
                session.player_side,
                DesktopInput::KeyDown {
                    code: "Digit1".to_owned(),
                    modifiers: InputModifiers {
                        control: true,
                        shift: false,
                    },
                },
            )
            .unwrap();
        session
            .controls
            .apply_request(
                &mut session.battle,
                TacticalControlRequest {
                    kind: "clearSelection".to_owned(),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        input
            .apply(
                &mut session.battle,
                &mut session.controls,
                session.player_side,
                DesktopInput::KeyDown {
                    code: "Digit1".to_owned(),
                    modifiers: InputModifiers::default(),
                },
            )
            .unwrap();
        let snapshot = render_snapshot(&session.battle, &session.controls);
        assert!(snapshot.units.iter().find(|unit| unit.unit_id == "attacker-a").unwrap().selected);
        assert!(
            input
                .apply(
                    &mut session.battle,
                    &mut session.controls,
                    session.player_side,
                    DesktopInput::KeyDown {
                        code: "Escape".to_owned(),
                        modifiers: InputModifiers::default(),
                    },
                )
                .unwrap()
                .close_requested
        );
    }

    #[test]
    fn browser_input_is_fail_closed_and_ordered() {
        let mut session = session();
        session.input.reset_browser_sequence();
        let first = BrowserInputEnvelope {
            sequence: 1,
            input: DesktopInput::Wheel { delta_y: -1.0 },
        };
        let skipped = BrowserInputEnvelope {
            sequence: 3,
            input: DesktopInput::Wheel { delta_y: -1.0 },
        };
        let NativeBattleSession {
            battle,
            controls,
            player_side,
            input,
        } = &mut session;
        input
            .apply_browser(battle, controls, *player_side, first)
            .unwrap();
        assert!(input.apply_browser(battle, controls, *player_side, skipped).is_err());
    }
}

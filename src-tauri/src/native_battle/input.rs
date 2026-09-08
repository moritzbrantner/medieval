use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use medieval_core::{BattlePoint, BattleSide, TacticalBattle};
use serde::Deserialize;
use tauri::{Listener, Window};

use super::{NativeBattleSession, SharedError, SharedSession};
use super::controls::{TacticalControlError, TacticalControlRequest, TacticalControls};

pub const BROWSER_INPUT_EVENT: &str = "medieval:tactical-input";

const CAMERA_PAN_FRACTION: f32 = 0.05;
const CAMERA_ZOOM_FACTOR: f32 = 1.15;
const DRAG_THRESHOLD_NORMALIZED: f64 = 0.02;
const CLICK_RADIUS_FRACTION: f64 = 0.08;
const MIN_CLICK_RADIUS_MM: f64 = 2_000.0;
const MAX_CLICK_RADIUS_MM: f64 = 12_000.0;

#[derive(Copy, Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InputModifiers {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub control: bool,
}

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
            return Err("desktop tactical pointer coordinates must be finite with a positive viewport".to_owned());
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
    next_browser_sequence: u64,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct InputOutcome {
    pub close_requested: bool,
}

impl DesktopInputState {
    pub fn reset_browser_sequence(&mut self) {
        self.pointer_gesture = None;
        self.next_browser_sequence = 1;
    }

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
        let zoom = camera.zoom.max(0.05);
        let step_x = battlefield.width_mm as f32 * CAMERA_PAN_FRACTION / zoom;
        let step_y = battlefield.depth_mm as f32 * CAMERA_PAN_FRACTION / zoom;
        let request = match code {
            "ArrowLeft" => Some(TacticalControlRequest {
                kind: "panCamera".to_owned(),
                delta_x_mm: Some(-step_x),
                delta_y_mm: Some(0.0),
                ..TacticalControlRequest::default()
            }),
            "ArrowRight" => Some(TacticalControlRequest {
                kind: "panCamera".to_owned(),
                delta_x_mm: Some(step_x),
                delta_y_mm: Some(0.0),
                ..TacticalControlRequest::default()
            }),
            "ArrowUp" => Some(TacticalControlRequest {
                kind: "panCamera".to_owned(),
                delta_x_mm: Some(0.0),
                delta_y_mm: Some(-step_y),
                ..TacticalControlRequest::default()
            }),
            "ArrowDown" => Some(TacticalControlRequest {
                kind: "panCamera".to_owned(),
                delta_x_mm: Some(0.0),
                delta_y_mm: Some(step_y),
                ..TacticalControlRequest::default()
            }),
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
        let dragged = (end_x - start_x).abs().max((end_y - start_y).abs())
            >= DRAG_THRESHOLD_NORMALIZED;
        let kind = selection_kind(modifiers);

        let unit_ids = if dragged {
            let first = viewport_to_world(battle, controls, start)?;
            let second = viewport_to_world(battle, controls, end)?;
            units_in_world_rect(battle, player_side, first, second)
        } else {
            let point = viewport_to_world(battle, controls, end)?;
            nearest_unit(battle, controls, point, |unit| {
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
        let destination = viewport_to_world(battle, controls, sample)?;
        if let Some(target_unit_id) = nearest_unit(battle, controls, destination, |unit| {
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
    suffix.as_bytes()[0].checked_sub(b'0').filter(|digit| *digit <= 9)
}

fn viewport_to_world(
    battle: &TacticalBattle,
    controls: &TacticalControls,
    sample: PointerSample,
) -> Result<BattlePoint, String> {
    let battlefield = battle.battlefield();
    let camera = controls.render_view(battle).camera;
    if !camera.zoom.is_finite() || camera.zoom <= 0.0 {
        return Err("desktop tactical camera zoom is invalid".to_owned());
    }
    let (normalized_x, normalized_y) = sample.normalized();
    let clip_x = normalized_x * 2.0 - 1.0;
    let clip_y = 1.0 - normalized_y * 2.0;
    let zoom = f64::from(camera.zoom.max(0.05));
    let world_x = f64::from(camera.center_x_mm)
        + clip_x * (f64::from(battlefield.width_mm) / 2.0) / zoom;
    let world_y = f64::from(camera.center_y_mm)
        - clip_y * (f64::from(battlefield.depth_mm) / 2.0) / zoom;
    Ok(BattlePoint::new(
        world_x
            .round()
            .clamp(0.0, f64::from(battlefield.width_mm)) as u32,
        world_y
            .round()
            .clamp(0.0, f64::from(battlefield.depth_mm)) as u32,
    ))
}

fn nearest_unit<F>(
    battle: &TacticalBattle,
    controls: &TacticalControls,
    point: BattlePoint,
    mut predicate: F,
) -> Option<String>
where
    F: FnMut(&medieval_core::TacticalUnit) -> bool,
{
    let battlefield = battle.battlefield();
    let zoom = f64::from(controls.render_view(battle).camera.zoom.max(0.05));
    let radius = (f64::from(battlefield.width_mm.min(battlefield.depth_mm))
        * CLICK_RADIUS_FRACTION
        / zoom)
        .clamp(MIN_CLICK_RADIUS_MM, MAX_CLICK_RADIUS_MM);
    let radius_squared = radius * radius;
    battle
        .units()
        .iter()
        .filter(|unit| predicate(unit))
        .filter_map(|unit| {
            let position = unit.position();
            let dx = f64::from(position.x_mm) - f64::from(point.x_mm);
            let dy = f64::from(position.y_mm) - f64::from(point.y_mm);
            let distance_squared = dx * dx + dy * dy;
            (distance_squared <= radius_squared).then_some((distance_squared, unit.id()))
        })
        .min_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(right.1))
        })
        .map(|(_, unit_id)| unit_id.to_owned())
}

fn units_in_world_rect(
    battle: &TacticalBattle,
    player_side: BattleSide,
    first: BattlePoint,
    second: BattlePoint,
) -> Vec<String> {
    let min_x = first.x_mm.min(second.x_mm);
    let max_x = first.x_mm.max(second.x_mm);
    let min_y = first.y_mm.min(second.y_mm);
    let max_y = first.y_mm.max(second.y_mm);
    battle
        .units()
        .iter()
        .filter(|unit| {
            let position = unit.position();
            unit.side() == player_side
                && !unit.is_routed()
                && !unit.is_destroyed()
                && (min_x..=max_x).contains(&position.x_mm)
                && (min_y..=max_y).contains(&position.y_mm)
        })
        .map(|unit| unit.id().to_owned())
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
    use gtk::prelude::*;

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
            let modifiers = modifiers_from_gdk(event.state());
            let outcome = apply_shared_input(
                &key_session,
                DesktopInput::KeyDown {
                    code: code.to_owned(),
                    modifiers,
                },
            );
            finish_native_input(outcome, &key_window, &key_running, &key_error);
            return gdk::EVENT_STOP;
        }
        gdk::EVENT_PROPAGATE
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
            return gdk::EVENT_PROPAGATE;
        };
        let (x, y) = event.position();
        let input = DesktopInput::PointerDown {
            button,
            x,
            y,
            width: f64::from(widget.allocated_width().max(1)),
            height: f64::from(widget.allocated_height().max(1)),
            modifiers: modifiers_from_gdk(event.state()),
        };
        let outcome = apply_shared_input(&press_session, input);
        finish_native_input(outcome, &press_window, &press_running, &press_error);
        gdk::EVENT_STOP
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
            return gdk::EVENT_PROPAGATE;
        };
        let (x, y) = event.position();
        let input = DesktopInput::PointerUp {
            button,
            x,
            y,
            width: f64::from(widget.allocated_width().max(1)),
            height: f64::from(widget.allocated_height().max(1)),
            modifiers: modifiers_from_gdk(event.state()),
        };
        let outcome = apply_shared_input(&release_session, input);
        finish_native_input(outcome, &release_window, &release_running, &release_error);
        gdk::EVENT_STOP
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
        gdk::EVENT_STOP
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
fn apply_shared_input(session: &SharedSession, input: DesktopInput) -> Result<InputOutcome, String> {
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

    fn pointer(kind: PointerButton, x: f64, y: f64) -> PointerSample {
        PointerSample::new(x, y, 1_000.0, 1_000.0).unwrap()
    }

    #[test]
    fn viewport_coordinates_round_trip_to_fitted_battlefield_space() {
        let session = session();
        let center = viewport_to_world(
            &session.battle,
            &session.controls,
            pointer(PointerButton::Primary, 500.0, 500.0),
        )
        .unwrap();
        let top_left = viewport_to_world(
            &session.battle,
            &session.controls,
            pointer(PointerButton::Primary, 0.0, 0.0),
        )
        .unwrap();

        assert_eq!(center, BattlePoint::new(50_000, 50_000));
        assert_eq!(top_left, BattlePoint::new(0, 0));
    }

    #[test]
    fn click_and_drag_selection_are_resolved_in_rust() {
        let mut session = session();
        let mut input = DesktopInputState::default();
        input
            .apply(
                &mut session.battle,
                &mut session.controls,
                session.player_side,
                DesktopInput::PointerDown {
                    button: PointerButton::Primary,
                    x: 300.0,
                    y: 500.0,
                    width: 1_000.0,
                    height: 1_000.0,
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
                    x: 300.0,
                    y: 500.0,
                    width: 1_000.0,
                    height: 1_000.0,
                    modifiers: InputModifiers::default(),
                },
            )
            .unwrap();
        let snapshot = medieval_renderer::BattleRenderSnapshot::project(
            &session.battle,
            &session.controls.render_view(&session.battle),
        );
        assert!(snapshot.units.iter().find(|unit| unit.unit_id == "attacker-a").unwrap().selected);

        input
            .apply(
                &mut session.battle,
                &mut session.controls,
                session.player_side,
                DesktopInput::PointerDown {
                    button: PointerButton::Primary,
                    x: 250.0,
                    y: 450.0,
                    width: 1_000.0,
                    height: 1_000.0,
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
                    x: 500.0,
                    y: 650.0,
                    width: 1_000.0,
                    height: 1_000.0,
                    modifiers: InputModifiers::default(),
                },
            )
            .unwrap();
        let snapshot = medieval_renderer::BattleRenderSnapshot::project(
            &session.battle,
            &session.controls.render_view(&session.battle),
        );
        assert!(snapshot.units.iter().find(|unit| unit.unit_id == "attacker-a").unwrap().selected);
        assert!(snapshot.units.iter().find(|unit| unit.unit_id == "attacker-b").unwrap().selected);
    }

    #[test]
    fn right_click_moves_or_engages_through_existing_control_semantics() {
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

        input
            .apply(
                &mut session.battle,
                &mut session.controls,
                session.player_side,
                DesktopInput::PointerDown {
                    button: PointerButton::Secondary,
                    x: 550.0,
                    y: 500.0,
                    width: 1_000.0,
                    height: 1_000.0,
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
                    button: PointerButton::Secondary,
                    x: 550.0,
                    y: 500.0,
                    width: 1_000.0,
                    height: 1_000.0,
                    modifiers: InputModifiers::default(),
                },
            )
            .unwrap();
        assert_eq!(
            session
                .battle
                .units()
                .iter()
                .find(|unit| unit.id() == "attacker-a")
                .unwrap()
                .destination(),
            Some(BattlePoint::new(55_000, 50_000))
        );

        input
            .apply(
                &mut session.battle,
                &mut session.controls,
                session.player_side,
                DesktopInput::PointerDown {
                    button: PointerButton::Secondary,
                    x: 700.0,
                    y: 500.0,
                    width: 1_000.0,
                    height: 1_000.0,
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
                    button: PointerButton::Secondary,
                    x: 700.0,
                    y: 500.0,
                    width: 1_000.0,
                    height: 1_000.0,
                    modifiers: InputModifiers::default(),
                },
            )
            .unwrap();
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
    fn keyboard_camera_groups_stop_and_escape_map_to_semantic_controls() {
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
        assert!(session.controls.render_view(&session.battle).camera.center_x_mm > before.center_x_mm);

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
        session.controls.apply_request(
            &mut session.battle,
            TacticalControlRequest {
                kind: "clearSelection".to_owned(),
                ..TacticalControlRequest::default()
            },
        ).unwrap();
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
        let snapshot = medieval_renderer::BattleRenderSnapshot::project(
            &session.battle,
            &session.controls.render_view(&session.battle),
        );
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
        input.apply_browser(battle, controls, *player_side, first).unwrap();
        assert!(input.apply_browser(battle, controls, *player_side, skipped).is_err());
    }
}

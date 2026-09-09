use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use medieval_core::{
    BattlePoint, BattleSide, FlatBattlefield, Formation, TacticalBattle, TacticalUnit,
};
use medieval_renderer::{BattleRenderSnapshot, GpuBattleRenderer};
use serde::Serialize;
use tauri::{Manager, State, WebviewWindow, Window, WindowEvent};

mod controls;
mod input;

use controls::{TacticalControlRequest, TacticalControls};
use input::DesktopInputState;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use input::install_browser_input_listener;

const BATTLE_WINDOW_LABEL: &str = "tactical-battle";
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.055,
    g: 0.047,
    b: 0.035,
    a: 1.0,
};

type SharedRenderer = Arc<Mutex<Option<NativeSurfaceRenderer>>>;
type SharedSession = Arc<Mutex<NativeBattleSession>>;
type SharedError = Arc<Mutex<Option<String>>>;

pub struct NativeBattleState {
    main_window: WebviewWindow,
    window: Window,
    renderer: SharedRenderer,
    session: SharedSession,
    running: Arc<AtomicBool>,
    initializing: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    last_error: SharedError,
}

impl Drop for NativeBattleState {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        self.shutdown.store(true, Ordering::Release);
    }
}

struct NativeBattleSession {
    battle: TacticalBattle,
    controls: TacticalControls,
    player_side: BattleSide,
    input: DesktopInputState,
}

impl NativeBattleSession {
    fn snapshot(&self) -> BattleRenderSnapshot {
        BattleRenderSnapshot::project(&self.battle, &self.controls.render_view(&self.battle))
    }
}

#[derive(Copy, Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeBattleOpenResult {
    browser_input: bool,
}

struct NativeSurfaceRenderer {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: GpuBattleRenderer,
    snapshot: BattleRenderSnapshot,
}

impl NativeSurfaceRenderer {
    async fn new(window: Window, snapshot: BattleRenderSnapshot) -> Result<Self, String> {
        let size = window
            .inner_size()
            .map_err(|error| format!("could not read tactical battle window size: {error}"))?;
        let width = size.width.max(1);
        let height = size.height.max(1);
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .map_err(|error| format!("could not create tactical wgpu surface: {error}"))?;
        let adapter = wgpu::util::initialize_adapter_from_env_or_default(&instance, Some(&surface))
            .await
            .map_err(|error| format!("could not select a tactical wgpu adapter: {error}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Medieval tactical device"),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("could not create tactical wgpu device: {error}"))?;
        let config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| {
                "selected adapter cannot present to the tactical battle window".to_owned()
            })?;
        surface.configure(&device, &config);

        let mut renderer = GpuBattleRenderer::new(&device, config.format);
        renderer.upload_snapshot(&device, &queue, &snapshot);

        Ok(Self {
            instance,
            surface,
            adapter,
            device,
            queue,
            config,
            renderer,
            snapshot,
        })
    }

    fn render_frame(
        &mut self,
        window: &Window,
        snapshot: &BattleRenderSnapshot,
    ) -> Result<(), String> {
        let size = window
            .inner_size()
            .map_err(|error| format!("could not read tactical battle window size: {error}"))?;
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        self.resize(size.width, size.height);

        if self.snapshot != *snapshot {
            self.renderer
                .upload_snapshot(&self.device, &self.queue, snapshot);
            self.snapshot = snapshot.clone();
        }

        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => {
                self.present(frame);
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.present(frame);
                self.surface.configure(&self.device, &self.config);
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                Ok(())
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.recreate_surface(window.clone(), size.width, size.height)
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                Err("wgpu rejected the tactical surface frame as invalid".to_owned())
            }
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn recreate_surface(&mut self, window: Window, width: u32, height: u32) -> Result<(), String> {
        let surface = self
            .instance
            .create_surface(window)
            .map_err(|error| format!("could not recreate lost tactical wgpu surface: {error}"))?;
        let config = surface
            .get_default_config(&self.adapter, width.max(1), height.max(1))
            .ok_or_else(|| {
                "selected adapter no longer supports the tactical battle window".to_owned()
            })?;

        if config.format != self.config.format {
            self.renderer = GpuBattleRenderer::new(&self.device, config.format);
            self.renderer
                .upload_snapshot(&self.device, &self.queue, &self.snapshot);
        }
        surface.configure(&self.device, &config);
        self.surface = surface;
        self.config = config;
        Ok(())
    }

    fn present(&self, frame: wgpu::SurfaceTexture) {
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Medieval tactical frame encoder"),
            });
        self.renderer.render(&mut encoder, &view, CLEAR_COLOR);
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }
}

pub fn install(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let main_window = app
        .get_webview_window("main")
        .expect("configured Medieval main window must exist during setup");
    let window = build_battle_window(app, &main_window)?;
    let renderer = Arc::new(Mutex::new(None));
    let session = Arc::new(Mutex::new(sample_session()));
    let running = Arc::new(AtomicBool::new(false));
    let initializing = Arc::new(AtomicBool::new(false));
    let shutdown = Arc::new(AtomicBool::new(false));
    let last_error = Arc::new(Mutex::new(None));

    let shutdown_window = window.clone();
    let shutdown_renderer = Arc::clone(&renderer);
    let shutdown_running = Arc::clone(&running);
    let shutdown_flag = Arc::clone(&shutdown);
    let overlay_main = main_window.clone();
    let overlay_window = window.clone();
    main_window.on_window_event(move |event| match event {
        WindowEvent::Destroyed => {
            shutdown_running.store(false, Ordering::Release);
            shutdown_flag.store(true, Ordering::Release);
            if let Ok(mut renderer) = shutdown_renderer.lock() {
                *renderer = None;
            }
            let _ = shutdown_window.destroy();
        }
        WindowEvent::Resized(_) | WindowEvent::Moved(_) => {
            let _ = sync_browser_input_window(&overlay_main, &overlay_window);
        }
        _ => {}
    });

    install_close_handler(&window, Arc::clone(&running));

    #[cfg(target_os = "linux")]
    input::install_linux_input(
        &window,
        Arc::clone(&session),
        Arc::clone(&running),
        Arc::clone(&last_error),
    )
    .map_err(std::io::Error::other)?;

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        window.set_ignore_cursor_events(true)?;
        install_browser_input_listener(
            app,
            window.clone(),
            Arc::clone(&session),
            Arc::clone(&running),
            Arc::clone(&last_error),
        );
        sync_browser_input_window(&main_window, &window).map_err(std::io::Error::other)?;
    }

    spawn_frame_scheduler(
        window.clone(),
        Arc::clone(&renderer),
        Arc::clone(&session),
        Arc::clone(&running),
        Arc::clone(&shutdown),
        Arc::clone(&last_error),
    );

    assert!(app.manage(NativeBattleState {
        main_window,
        window,
        renderer,
        session,
        running,
        initializing,
        shutdown,
        last_error,
    }));
    Ok(())
}

#[cfg(target_os = "windows")]
fn build_battle_window(app: &tauri::App, main_window: &WebviewWindow) -> tauri::Result<Window> {
    let parent = main_window.hwnd()?;
    tauri::window::WindowBuilder::new(app, BATTLE_WINDOW_LABEL)
        .parent_raw(parent)
        .title("Medieval — Tactical Battle")
        .decorations(false)
        .resizable(false)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()
}

#[cfg(target_os = "macos")]
fn build_battle_window(app: &tauri::App, main_window: &WebviewWindow) -> tauri::Result<Window> {
    let parent = main_window.ns_window()?;
    tauri::window::WindowBuilder::new(app, BATTLE_WINDOW_LABEL)
        .parent_raw(parent)
        .title("Medieval — Tactical Battle")
        .decorations(false)
        .resizable(false)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()
}

#[cfg(target_os = "linux")]
fn build_battle_window(app: &tauri::App, _main_window: &WebviewWindow) -> tauri::Result<Window> {
    tauri::window::WindowBuilder::new(app, BATTLE_WINDOW_LABEL)
        .title("Medieval — Tactical Battle")
        .inner_size(1024.0, 720.0)
        .min_inner_size(640.0, 480.0)
        .visible(false)
        .build()
}

#[cfg(target_os = "windows")]
fn sync_browser_input_window(main_window: &WebviewWindow, window: &Window) -> Result<(), String> {
    let size = main_window.inner_size().map_err(|error| {
        format!("could not read main window size for tactical overlay: {error}")
    })?;
    window
        .set_position(tauri::PhysicalPosition::new(0, 0))
        .map_err(|error| format!("could not position tactical child window: {error}"))?;
    window
        .set_size(size)
        .map_err(|error| format!("could not size tactical child window: {error}"))
}

#[cfg(target_os = "macos")]
fn sync_browser_input_window(main_window: &WebviewWindow, window: &Window) -> Result<(), String> {
    let position = main_window.inner_position().map_err(|error| {
        format!("could not read main window position for tactical overlay: {error}")
    })?;
    let size = main_window.inner_size().map_err(|error| {
        format!("could not read main window size for tactical overlay: {error}")
    })?;
    window
        .set_position(position)
        .map_err(|error| format!("could not position tactical child window: {error}"))?;
    window
        .set_size(size)
        .map_err(|error| format!("could not size tactical child window: {error}"))
}

#[cfg(target_os = "linux")]
fn sync_browser_input_window(_main_window: &WebviewWindow, _window: &Window) -> Result<(), String> {
    Ok(())
}

const fn browser_input_enabled() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos"))
}

#[tauri::command]
pub async fn open_native_battle_renderer(
    state: State<'_, NativeBattleState>,
) -> Result<NativeBattleOpenResult, String> {
    ensure_renderer_initialized(&state)?;
    sync_browser_input_window(&state.main_window, &state.window)?;
    state
        .session
        .lock()
        .map_err(|_| "native tactical session lock was poisoned".to_owned())?
        .input
        .reset_browser_sequence();
    state
        .window
        .show()
        .map_err(|error| format!("could not show tactical battle window: {error}"))?;

    #[cfg(target_os = "linux")]
    state
        .window
        .set_focus()
        .map_err(|error| format!("could not focus tactical battle window: {error}"))?;

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    state
        .main_window
        .set_focus()
        .map_err(|error| format!("could not keep the tactical input host focused: {error}"))?;

    state.running.store(true, Ordering::Release);
    Ok(NativeBattleOpenResult {
        browser_input: browser_input_enabled(),
    })
}

#[tauri::command]
pub fn control_native_battle(
    state: State<'_, NativeBattleState>,
    request: TacticalControlRequest,
) -> Result<(), String> {
    let mut session = state
        .session
        .lock()
        .map_err(|_| "native tactical session lock was poisoned".to_owned())?;
    let NativeBattleSession {
        battle, controls, ..
    } = &mut *session;
    controls
        .apply_request(battle, request)
        .map_err(|error| error.to_string())
}

fn ensure_renderer_initialized(state: &NativeBattleState) -> Result<(), String> {
    if state
        .renderer
        .lock()
        .map_err(|_| "native renderer lock was poisoned".to_owned())?
        .is_some()
    {
        return Ok(());
    }

    state
        .initializing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "native tactical renderer initialization is already in progress".to_owned())?;

    let result = initialize_renderer_on_main_thread(state);
    state.initializing.store(false, Ordering::Release);
    result
}

fn initialize_renderer_on_main_thread(state: &NativeBattleState) -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let window = state.window.clone();
    let renderer = Arc::clone(&state.renderer);
    let snapshot = state
        .session
        .lock()
        .map_err(|_| "native tactical session lock was poisoned".to_owned())?
        .snapshot();
    let last_error = Arc::clone(&state.last_error);

    state
        .window
        .run_on_main_thread(move || {
            let result =
                tauri::async_runtime::block_on(NativeSurfaceRenderer::new(window, snapshot));
            match result {
                Ok(surface_renderer) => {
                    if let Ok(mut guard) = renderer.lock() {
                        *guard = Some(surface_renderer);
                    }
                    if let Ok(mut error) = last_error.lock() {
                        *error = None;
                    }
                    let _ = sender.send(Ok(()));
                }
                Err(error) => {
                    if let Ok(mut last_error) = last_error.lock() {
                        *last_error = Some(error.clone());
                    }
                    let _ = sender.send(Err(error));
                }
            }
        })
        .map_err(|error| format!("could not initialize renderer on the main thread: {error}"))?;

    receiver
        .recv()
        .map_err(|_| "native renderer initialization ended without a result".to_owned())?
}

fn install_close_handler(window: &Window, running: Arc<AtomicBool>) {
    let window_to_hide = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            running.store(false, Ordering::Release);
            let _ = window_to_hide.hide();
        }
    });
}

fn spawn_frame_scheduler(
    window: Window,
    renderer: SharedRenderer,
    session: SharedSession,
    running: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    last_error: SharedError,
) {
    let frame_queued = Arc::new(AtomicBool::new(false));
    thread::Builder::new()
        .name("medieval-tactical-frames".to_owned())
        .spawn(move || {
            while !shutdown.load(Ordering::Acquire) {
                if running.load(Ordering::Acquire) && !frame_queued.swap(true, Ordering::AcqRel) {
                    let queued = Arc::clone(&frame_queued);
                    let frame_renderer = Arc::clone(&renderer);
                    let frame_session = Arc::clone(&session);
                    let frame_running = Arc::clone(&running);
                    let frame_last_error = Arc::clone(&last_error);
                    let frame_window = window.clone();
                    let schedule_result = window.run_on_main_thread(move || {
                        let render_result: Result<(), String> = (|| {
                            let snapshot = frame_session
                                .lock()
                                .map_err(|_| {
                                    "native tactical session lock was poisoned".to_owned()
                                })?
                                .snapshot();
                            let mut renderer = frame_renderer
                                .lock()
                                .map_err(|_| "native renderer lock was poisoned".to_owned())?;
                            let renderer = renderer.as_mut().ok_or_else(|| {
                                "native renderer disappeared while frames were active".to_owned()
                            })?;
                            renderer.render_frame(&frame_window, &snapshot)
                        })();

                        if let Err(error) = render_result {
                            frame_running.store(false, Ordering::Release);
                            if let Ok(mut renderer) = frame_renderer.lock() {
                                *renderer = None;
                            }
                            if let Ok(mut last_error) = frame_last_error.lock() {
                                *last_error = Some(error);
                            }
                        }
                        queued.store(false, Ordering::Release);
                    });

                    if let Err(error) = schedule_result {
                        frame_queued.store(false, Ordering::Release);
                        running.store(false, Ordering::Release);
                        if let Ok(mut last_error) = last_error.lock() {
                            *last_error = Some(format!(
                                "could not schedule tactical frame on the main thread: {error}"
                            ));
                        }
                    }
                }
                thread::sleep(FRAME_INTERVAL);
            }
        })
        .expect("failed to start Medieval tactical frame scheduler");
}

fn sample_session() -> NativeBattleSession {
    let battlefield = FlatBattlefield::new(100_000, 100_000);
    let battle = TacticalBattle::new(
        battlefield,
        vec![
            TacticalUnit::new(
                "attacker-spears",
                BattleSide::Attacker,
                80,
                BattlePoint::new(30_000, 50_000),
                Formation::Line { files: 20 },
                1_000,
            ),
            TacticalUnit::new(
                "defender-spears",
                BattleSide::Defender,
                80,
                BattlePoint::new(70_000, 50_000),
                Formation::Column { files: 16 },
                1_000,
            ),
        ],
    )
    .expect("native renderer sample battle is valid");
    let player_side = BattleSide::Attacker;
    let mut controls = TacticalControls::new(&battle, player_side);
    controls
        .apply_request(
            &mut battle.clone(),
            TacticalControlRequest {
                kind: "selectReplace".to_owned(),
                unit_ids: Some(vec!["attacker-spears".to_owned()]),
                ..TacticalControlRequest::default()
            },
        )
        .expect("sample attacker is controllable");
    controls
        .apply_request(
            &mut battle.clone(),
            TacticalControlRequest {
                kind: "setOrderPreview".to_owned(),
                active: Some(true),
                ..TacticalControlRequest::default()
            },
        )
        .expect("sample order preview is valid");

    NativeBattleSession {
        battle,
        controls,
        player_side,
        input: DesktopInputState::default(),
    }
}

#[cfg(test)]
fn sample_snapshot() -> BattleRenderSnapshot {
    sample_session().snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_preview_snapshot_is_projected_from_medieval_core_and_controls() {
        let snapshot = sample_snapshot();

        assert_eq!(snapshot.tick, 0);
        assert_eq!(snapshot.units.len(), 2);
        assert_eq!(snapshot.units[0].unit_id, "attacker-spears");
        assert!(snapshot.units[0].selected);
        assert!(snapshot.units[0].order_preview);
        assert_eq!(snapshot.units[1].unit_id, "defender-spears");
    }

    #[test]
    fn native_session_reprojects_changed_rust_control_state() {
        let mut session = sample_session();
        let battle_before = session.battle.clone();
        let before = session.snapshot();

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
        session
            .controls
            .apply_request(
                &mut session.battle,
                TacticalControlRequest {
                    kind: "panCamera".to_owned(),
                    delta_x_mm: Some(5_000.0),
                    delta_y_mm: Some(0.0),
                    ..TacticalControlRequest::default()
                },
            )
            .unwrap();
        let after = session.snapshot();

        assert_eq!(session.battle, battle_before);
        assert_ne!(before.camera, after.camera);
        assert!(before.units[0].selected);
        assert!(!after.units[0].selected);
    }

    #[test]
    fn frame_scheduler_is_bounded_to_about_sixty_hz() {
        assert_eq!(FRAME_INTERVAL, Duration::from_millis(16));
    }

    #[test]
    fn browser_input_is_used_only_for_overlay_platforms() {
        assert_eq!(
            browser_input_enabled(),
            cfg!(any(target_os = "windows", target_os = "macos"))
        );
    }
}

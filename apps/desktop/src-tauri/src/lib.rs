use std::sync::Mutex;

use tauri::{
    AppHandle, Manager, RunEvent, State, WebviewWindow, WindowEvent,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use veneer_core::{
    Controller, ThemeSummary,
    applier::{ThemeApplier, ThemeReport, ThemeRevert},
    wallpaper::WallpaperController,
};
use veneer_ipc::{IntegrationStage, PROTOCOL_VERSION, RuntimeStatus};
use veneer_rule_engine::WindowContext;
use veneer_state_journal::Journal;
use veneer_windows_platform::{
    autostart::Autostart,
    dwm::DwmBackend,
    os::{build_number, local_time_of_day},
    wallpaper::DesktopWallpaperBackend,
};

type Applier = ThemeApplier<DwmBackend, DesktopWallpaperBackend>;
type SharedApplier = Mutex<Applier>;

/// Opens the journal and restores anything a crashed run left behind. If the
/// journal cannot be opened, wallpaper changes are disabled rather than made
/// without a way back.
fn open_wallpaper(app: &tauri::App) -> Option<WallpaperController<DesktopWallpaperBackend>> {
    let path = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|dir| std::fs::create_dir_all(&dir).ok().map(|_| dir))
        .map(|dir| dir.join("journal.json"));
    let Some(path) = path else {
        tracing::error!("no data directory: wallpaper changes disabled");
        return None;
    };
    match Journal::open(path) {
        Ok((journal, opened)) => {
            tracing::info!(?opened, "journal opened");
            let mut controller = WallpaperController::new(DesktopWallpaperBackend, journal);
            match controller.recover() {
                Ok(report) => tracing::info!(?report, "recovery finished"),
                Err(error) => tracing::error!(%error, "recovery failed"),
            }
            Some(controller)
        }
        Err(error) => {
            tracing::error!(%error, "journal unavailable: wallpaper changes disabled");
            None
        }
    }
}

const AUTOSTART_NAME: &str = "Veneer";
const MINIMIZED_FLAG: &str = "--minimized";

#[tauri::command]
fn get_autostart() -> bool {
    Autostart::new(AUTOSTART_NAME).is_enabled()
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<bool, String> {
    let entry = Autostart::new(AUTOSTART_NAME);
    if enabled {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        entry
            .enable(&format!("\"{}\" {MINIMIZED_FLAG}", exe.display()))
            .map_err(|error| error.to_string())?;
    } else {
        entry.disable().map_err(|error| error.to_string())?;
    }
    Ok(entry.is_enabled())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Apri Veneer", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Esci", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut tray = TrayIconBuilder::new()
        .tooltip("Veneer")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
fn get_runtime_status() -> RuntimeStatus {
    RuntimeStatus {
        protocol_version: PROTOCOL_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        platform: std::env::consts::OS.to_owned(),
        integration: IntegrationStage::WindowFrames,
    }
}

#[tauri::command]
fn validate_theme(theme: String) -> Result<ThemeSummary, String> {
    veneer_core::validate_theme(&theme).map_err(|error| error.to_string())
}

#[tauri::command]
fn apply_theme(
    window: WebviewWindow,
    applier: State<'_, SharedApplier>,
    theme: String,
) -> Result<ThemeReport, String> {
    let handle = window_handle(&window)?;
    let now = local_time_of_day().ok_or("impossibile leggere l'ora locale")?;
    let context = window_context(&window);
    let mut applier = applier
        .lock()
        .map_err(|_| "stato interno non disponibile")?;
    applier
        .apply(handle, &theme, &context, now)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn revert_theme(
    window: WebviewWindow,
    applier: State<'_, SharedApplier>,
) -> Result<ThemeRevert, String> {
    let handle = window_handle(&window)?;
    let mut applier = applier
        .lock()
        .map_err(|_| "stato interno non disponibile")?;
    Ok(applier.revert(handle))
}

fn window_handle(window: &WebviewWindow) -> Result<isize, String> {
    window
        .hwnd()
        .map(|hwnd| hwnd.0 as isize)
        .map_err(|error| error.to_string())
}

fn window_context(window: &WebviewWindow) -> WindowContext {
    let process_name = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let on_primary_monitor = match (window.current_monitor(), window.primary_monitor()) {
        (Ok(Some(current)), Ok(Some(primary))) => Some(current.position() == primary.position()),
        _ => None,
    };
    WindowContext {
        process_name,
        title: window.title().unwrap_or_default(),
        on_primary_monitor,
        is_foreground: window.is_focused().unwrap_or(false),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            if let Ok(dir) = app.path().app_log_dir() {
                let _ = std::fs::create_dir_all(&dir);
                if let Err(error) = veneer_diagnostics::init_file(&dir.join("veneer.log")) {
                    eprintln!("logging disabled: {error}");
                }
            }
            let frames = Controller::new(DwmBackend::default(), build_number());
            app.manage::<SharedApplier>(Mutex::new(ThemeApplier::new(frames, open_wallpaper(app))));
            setup_tray(app)?;
            if std::env::args().any(|arg| arg == MINIMIZED_FLAG)
                && let Some(window) = app.get_webview_window("main")
            {
                let _ = window.hide();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Veneer in the tray; "Esci" really quits.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_status,
            validate_theme,
            apply_theme,
            revert_theme,
            get_autostart,
            set_autostart
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Veneer desktop application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(applier) = handle.try_state::<SharedApplier>()
                && let Ok(mut applier) = applier.lock()
            {
                applier.revert_all();
            }
        }
    });
}

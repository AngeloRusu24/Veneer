use std::sync::Mutex;

use tauri::{Manager, RunEvent, State, WebviewWindow};
use veneer_core::{ApplyReport, Controller, RevertReport, ThemeSummary};
use veneer_ipc::{IntegrationStage, PROTOCOL_VERSION, RuntimeStatus};
use veneer_rule_engine::WindowContext;
use veneer_windows_platform::{
    dwm::DwmBackend,
    os::{build_number, local_time_of_day},
};

type SharedController = Mutex<Controller<DwmBackend>>;

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
    controller: State<'_, SharedController>,
    theme: String,
) -> Result<ApplyReport, String> {
    let handle = window_handle(&window)?;
    let now = local_time_of_day().ok_or("impossibile leggere l'ora locale")?;
    let context = window_context(&window);
    let mut controller = controller
        .lock()
        .map_err(|_| "stato interno non disponibile")?;
    controller
        .apply_theme(handle, &theme, &context, now)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn revert_theme(
    window: WebviewWindow,
    controller: State<'_, SharedController>,
) -> Result<RevertReport, String> {
    let handle = window_handle(&window)?;
    let mut controller = controller
        .lock()
        .map_err(|_| "stato interno non disponibile")?;
    Ok(controller.revert(handle))
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
        .manage::<SharedController>(Mutex::new(Controller::new(
            DwmBackend::default(),
            build_number(),
        )))
        .setup(|app| {
            if let Ok(dir) = app.path().app_log_dir() {
                let _ = std::fs::create_dir_all(&dir);
                if let Err(error) = veneer_diagnostics::init_file(&dir.join("veneer.log")) {
                    eprintln!("logging disabled: {error}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_status,
            validate_theme,
            apply_theme,
            revert_theme
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Veneer desktop application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event
            && let Some(controller) = handle.try_state::<SharedController>()
            && let Ok(mut controller) = controller.lock()
        {
            controller.revert_all();
        }
    });
}

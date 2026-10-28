use veneer_ipc::{IntegrationStage, RuntimeStatus, PROTOCOL_VERSION};

#[tauri::command]
fn get_runtime_status() -> RuntimeStatus {
    RuntimeStatus {
        protocol_version: PROTOCOL_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        platform: std::env::consts::OS.to_owned(),
        integration: IntegrationStage::Scaffold,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_runtime_status])
        .run(tauri::generate_context!())
        .expect("failed to run Veneer desktop application");
}

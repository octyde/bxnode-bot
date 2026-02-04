//! BXNode Bot Desktop - Tauri Application

/// Tauri command: Get server status
#[tauri::command]
fn get_status() -> String {
    serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }).to_string()
}

/// Tauri command: Start the gateway server
#[tauri::command]
async fn start_server(port: u16) -> Result<String, String> {
    // TODO: Start the gateway server in background
    Ok(format!("Server starting on port {}", port))
}

/// Tauri command: Stop the gateway server
#[tauri::command]
async fn stop_server() -> Result<String, String> {
    // TODO: Stop the gateway server
    Ok("Server stopped".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            start_server,
            stop_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

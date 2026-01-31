//! RemoShell Tauri Application Library
//!
//! This module provides the Tauri application setup and plugin configuration.

use tauri::Manager;

/// Sample Tauri command for testing
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to RemoShell.", name)
}

/// Configure and run the Tauri application
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // Add plugins
    let builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());

    // Add barcode scanner plugin on mobile platforms
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());

    builder
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

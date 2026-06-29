#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use commands::{
    create_session, kill_session, list_sessions, read_session, resize_session, write_session,
    AppState,
};
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            create_session,
            write_session,
            resize_session,
            kill_session,
            list_sessions,
            read_session
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_title("baton")?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run baton desktop shell");
}

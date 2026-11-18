//! Metadata Copier and Editor — Rust core.
//!
//! Layers, each depending only on the ones listed after it:
//! `commands` (Tauri IPC) → `state` → `ops` (journal, executor) → `planning` (Preview/Review)
//! → `metadata` (tags, fields, dates) → `engine` (ExifTool) → `platform` (OS services).

pub mod commands;
pub mod engine;
pub mod geo;
pub mod metadata;
pub mod ops;
pub mod planning;
pub mod platform;
pub mod settings;
pub mod state;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;

/// App data folder name under `%LOCALAPPDATA%` on Windows.
const DATA_DIR_NAME: &str = "Metadata Copier and Editor";

/// Starts the app (after the WebView2 check on Windows).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if !platform::webview::ensure_webview2() {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let base = app
                .path()
                .local_data_dir()
                .map(|d| if cfg!(windows) { d.join(DATA_DIR_NAME) } else { d })
                .unwrap_or_else(|_| std::env::temp_dir().join(DATA_DIR_NAME));
            let state = Arc::new(AppState::new(base));
            state.journal.mark_interrupted();
            state.start_engine();
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::engine_status,
            commands::system::about,
            commands::files::expand_paths,
            commands::files::suggest_converted,
            commands::files::pair_files,
            commands::files::thumbnail,
            commands::files::search_places,
            commands::files::nearest_place,
            commands::copy::plan_copy,
            commands::copy::run_copy,
            commands::copy::cancel_run,
            commands::edit::read_edit_fields,
            commands::edit::plan_edit,
            commands::edit::run_edit,
            commands::history::list_history,
            commands::history::undo_op,
            commands::history::pending_recovery,
            commands::history::recover_op,
            commands::settings::get_settings,
            commands::settings::set_settings,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the app")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event
                && let Some(s) = app.try_state::<Arc<AppState>>()
            {
                s.shutdown();
            }
        });
}

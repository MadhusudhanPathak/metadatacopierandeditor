//! Settings.

use super::R;
use crate::settings::{self, Settings};
use crate::state::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, Arc<AppState>>) -> Settings {
    state.settings.lock().unwrap().clone()
}

/// Saves the settings to disk, then makes them current.
#[tauri::command]
pub fn set_settings(state: State<'_, Arc<AppState>>, settings: Settings) -> R<Settings> {
    settings::save(&state.base, &settings)?;
    *state.settings.lock().unwrap() = settings.clone();
    Ok(settings)
}

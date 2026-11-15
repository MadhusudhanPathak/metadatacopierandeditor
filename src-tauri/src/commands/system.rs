//! Engine status and About information.

use super::{R, app_state, blocking};
use crate::engine::extract;
use crate::platform::fsops;
use crate::state::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    ready: bool,
    error: Option<String>,
}

/// Whether the bundled engine has started (polled by the top bar).
#[tauri::command]
pub fn engine_status(state: State<'_, Arc<AppState>>) -> EngineStatus {
    match state.engine_status() {
        None => EngineStatus { ready: false, error: None },
        Some(Ok(())) => EngineStatus { ready: true, error: None },
        Some(Err(e)) => EngineStatus { ready: false, error: Some(e) },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct About {
    app_version: String,
    exiftool_version: String,
    platform: &'static str,
    can_set_created: bool,
    data_dir: String,
}

/// Versions, platform and data folder for the About screen.
#[tauri::command]
pub async fn about(app: AppHandle) -> R<About> {
    let s = app_state(&app);
    let app_version = app.package_info().version.to_string();
    blocking(move || {
        let exiftool_version = s.engine().and_then(|e| e.version()).unwrap_or_else(|_| extract::EXIFTOOL_VERSION.to_string());
        Ok(About {
            app_version,
            exiftool_version,
            platform: if cfg!(windows) {
                "windows"
            } else if cfg!(target_os = "android") {
                "android"
            } else {
                "other"
            },
            can_set_created: fsops::CAN_SET_CREATED,
            data_dir: s.base.to_string_lossy().into_owned(),
        })
    })
    .await
}

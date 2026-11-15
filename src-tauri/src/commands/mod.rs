//! Tauri commands: the only surface the UI talks to (typed in `src/api`). Slow work runs on
//! blocking threads; every error is a plain-language string.

pub mod copy;
pub mod edit;
pub mod files;
pub mod history;
mod runner;
pub mod settings;
pub mod system;

use crate::state::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

type R<T> = Result<T, String>;

/// Runs `f` on the blocking thread pool so the UI thread stays responsive.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("Internal error: {e}"))?
}

fn app_state(app: &AppHandle) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

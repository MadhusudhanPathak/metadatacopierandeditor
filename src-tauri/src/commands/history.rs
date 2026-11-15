//! History, Undo and crash recovery.

use super::runner::with_ctx;
use super::{R, app_state, blocking};
use crate::ops::executor::{self, Recovery};
use crate::ops::journal::{ItemStatus, Op, OpKind, OpStatus, Summary};
use serde::Serialize;
use tauri::AppHandle;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    label: String,
    status: ItemStatus,
    message: Option<String>,
    warnings: Vec<String>,
    details: Vec<String>,
    final_path: String,
    original: String,
    /// Where the user's original now lives (`name_original`).
    original_kept_as: String,
}

/// An operation as the UI shows it (no paths to temp files or ExifTool arguments).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OpView {
    id: String,
    kind: OpKind,
    title: String,
    created_ms: i64,
    finished_ms: Option<i64>,
    status: OpStatus,
    summary: Summary,
    undo_available: bool,
    items: Vec<ItemView>,
    /// Set when the run as a whole failed (e.g. the engine couldn't start).
    pub error: Option<String>,
}

/// The UI view of a journal operation.
pub fn op_view(op: &Op) -> OpView {
    OpView {
        id: op.id.clone(),
        kind: op.kind,
        title: op.title.clone(),
        created_ms: op.created_ms,
        finished_ms: op.finished_ms,
        status: op.status,
        summary: op.summary(),
        undo_available: op.undo_available(),
        items: op
            .items
            .iter()
            .map(|i| ItemView {
                label: i.label.clone(),
                status: i.status,
                message: i.message.clone(),
                warnings: i.warnings.clone(),
                details: i.details.clone(),
                final_path: i.final_path.clone(),
                original: i.original.clone(),
                original_kept_as: i.source_renamed.clone(),
            })
            .collect(),
        error: None,
    }
}

/// Every finished operation, newest first.
#[tauri::command]
pub async fn list_history(app: AppHandle) -> R<Vec<OpView>> {
    let s = app_state(&app);
    blocking(move || Ok(s.journal.list().iter().filter(|o| o.status != OpStatus::Running).map(op_view).collect())).await
}

/// Undoes a finished operation and returns its new state.
#[tauri::command]
pub async fn undo_op(app: AppHandle, id: String) -> R<OpView> {
    blocking(move || {
        let s = app_state(&app);
        let mut op = s.journal.load(&id)?;
        with_ctx(&app, |ctx| executor::undo(ctx, &mut op))?;
        Ok(op_view(&s.journal.load(&id)?))
    })
    .await
}

/// Operations a previous session left unfinished.
#[tauri::command]
pub async fn pending_recovery(app: AppHandle) -> R<Vec<OpView>> {
    let s = app_state(&app);
    blocking(move || Ok(s.journal.list().iter().filter(|o| o.status == OpStatus::Interrupted).map(op_view).collect())).await
}

/// Finishes or rolls back an interrupted operation and returns its new state.
#[tauri::command]
pub async fn recover_op(app: AppHandle, id: String, action: Recovery) -> R<OpView> {
    blocking(move || {
        let s = app_state(&app);
        let mut op = s.journal.load(&id)?;
        with_ctx(&app, |ctx| executor::recover(ctx, &mut op, action))?;
        Ok(op_view(&s.journal.load(&id)?))
    })
    .await
}

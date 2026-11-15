//! Copy flow: Preview and Apply.

use super::runner::start_op;
use super::{R, app_state, blocking};
use crate::metadata::datetime::SystemTz;
use crate::ops::items::copy_items;
use crate::ops::journal::{self, Op, OpKind};
use crate::planning::copy::{self, CopyPlan, CopyReq};
use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};

/// Builds the Preview. It becomes the only plan `run_copy` accepts.
#[tauri::command]
pub async fn plan_copy(app: AppHandle, req: CopyReq) -> R<CopyPlan> {
    let s = app_state(&app);
    blocking(move || {
        let engine = s.engine()?;
        let plan = copy::plan(&engine, &s.meta, &req, &SystemTz, journal::new_id());
        s.set_copy_plan(plan.clone());
        Ok(plan)
    })
    .await
}

/// Applies the Preview, minus the pairs the user skipped. Returns the operation id.
#[tauri::command]
pub fn run_copy(app: AppHandle, plan_id: String, skip: Vec<String>) -> R<String> {
    let plan = app_state(&app).copy_plan(&plan_id).ok_or("That preview has expired. Please preview again.")?;
    let id = journal::new_id();
    let items = copy_items(&plan, &id, &skip);
    let n = items.len();
    start_op(&app, Op::new(id, OpKind::Copy, format!("Copied metadata to {n} file{}", if n == 1 { "" } else { "s" }), items))
}

/// Stops a running Copy or Edit after the current item.
#[tauri::command]
pub fn cancel_run(state: State<'_, Arc<AppState>>) {
    state.cancel.store(true, Ordering::SeqCst);
}

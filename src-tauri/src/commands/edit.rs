//! Edit flow: read fields, Review and Save.

use super::runner::start_op;
use super::{R, app_state, blocking};
use crate::metadata::datetime::SystemTz;
use crate::ops::items::edit_items;
use crate::ops::journal::{self, Op, OpKind};
use crate::planning::edit::{self, EditPlan, EditReq, FileFields};
use tauri::AppHandle;

/// Curated fields of every picked file, for the editor.
#[tauri::command]
pub async fn read_edit_fields(app: AppHandle, paths: Vec<String>) -> R<Vec<FileFields>> {
    let s = app_state(&app);
    blocking(move || {
        let engine = s.engine()?;
        Ok(edit::read_fields(&engine, &s.meta, &paths, &SystemTz))
    })
    .await
}

/// Builds the Review. It becomes the only plan `run_edit` accepts.
#[tauri::command]
pub async fn plan_edit(app: AppHandle, req: EditReq) -> R<EditPlan> {
    let s = app_state(&app);
    blocking(move || {
        let engine = s.engine()?;
        let plan = edit::plan(&engine, &s.meta, &req, &SystemTz, journal::new_id());
        s.set_edit_plan(plan.clone());
        Ok(plan)
    })
    .await
}

/// Saves the Review. Returns the operation id.
#[tauri::command]
pub fn run_edit(app: AppHandle, plan_id: String) -> R<String> {
    let plan = app_state(&app).edit_plan(&plan_id).ok_or("That review has expired. Please review again.")?;
    let id = journal::new_id();
    let items = edit_items(&plan, &id);
    let n = items.len();
    start_op(&app, Op::new(id, OpKind::Edit, format!("Edited {n} file{}", if n == 1 { "" } else { "s" }), items))
}

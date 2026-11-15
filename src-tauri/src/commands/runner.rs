//! Running operations: Apply in the background with progress events, Undo and recovery in the
//! foreground. Only one operation runs at a time.

use super::history::op_view;
use super::{R, app_state};
use crate::ops::executor::{self, Ctx, Progress};
use crate::ops::journal::Op;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

/// Per-item progress for the Run screen.
const EVENT_PROGRESS: &str = "op-progress";
/// Emitted once with the finished operation.
const EVENT_FINISHED: &str = "op-finished";

/// Starts `op` on its own thread and returns its id at once. Progress and the final result
/// arrive as events.
pub fn start_op(app: &AppHandle, mut op: Op) -> R<String> {
    let s = app_state(app);
    let guard = s.begin_operation()?;
    if op.items.is_empty() {
        return Err("There's nothing ready to apply.".into());
    }
    s.cancel.store(false, Ordering::SeqCst);
    let id = op.id.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        let progress_app = app.clone();
        let progress = move |p: Progress| {
            let _ = progress_app.emit(EVENT_PROGRESS, p);
        };
        let result = s.engine().and_then(|engine| {
            let stop_on_failure = s.settings.lock().unwrap().stop_on_failure;
            let ctx = Ctx { engine: &engine, journal: &s.journal, cancel: &s.cancel, stop_on_failure, progress: &progress };
            executor::run(&ctx, &mut op)
        });
        s.meta.clear();
        let finished = s.journal.load(&op.id).unwrap_or(op);
        let mut view = op_view(&finished);
        view.error = result.err();
        drop(guard);
        let _ = app.emit(EVENT_FINISHED, view);
    });
    Ok(id)
}

/// Runs Undo or recovery synchronously (on the caller's blocking thread), never cancellable.
pub fn with_ctx<T>(app: &AppHandle, f: impl FnOnce(&Ctx) -> R<T>) -> R<T> {
    let s = app_state(app);
    let _guard = s.begin_operation()?;
    let progress_app = app.clone();
    let progress = move |p: Progress| {
        let _ = progress_app.emit(EVENT_PROGRESS, p);
    };
    let never = AtomicBool::new(false);
    let res = s.engine().and_then(|engine| f(&Ctx { engine: &engine, journal: &s.journal, cancel: &never, stop_on_failure: false, progress: &progress }));
    s.meta.clear();
    res
}

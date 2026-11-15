//! Executor (spec 5.5): runs journal items — write a verified temp file, swap, apply file
//! dates — with cancellation and stop-on-failure, plus Undo and crash recovery. The journal is
//! written before any file is touched and every step is logged.

use super::journal::{ItemStatus, Journal, OP_VERSION, Op, OpStatus};
use super::rollback::{rollback_item, undo_blocker};
use super::verify::{human_engine_error, verify};
use crate::engine::{Engine, path_arg, split_messages};
use crate::platform::fsops::{self, file_name, same_path};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Undo lists at most this many blockers before "(and N more)".
const MAX_BLOCKERS_SHOWN: usize = 3;

/// Live progress for the Run screen.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub op_id: String,
    pub index: usize,
    pub total: usize,
    pub label: String,
    /// "working", "done", "warning", "failed" or "skipped".
    pub status: &'static str,
    pub message: Option<String>,
    pub done: usize,
}

/// What an operation needs while it runs.
pub struct Ctx<'a> {
    pub engine: &'a Engine,
    pub journal: &'a Journal,
    pub cancel: &'a AtomicBool,
    pub stop_on_failure: bool,
    pub progress: &'a (dyn Fn(Progress) + Sync),
}

impl Ctx<'_> {
    fn report(&self, op: &Op, i: usize, status: &'static str, done: usize) {
        let it = &op.items[i];
        let message = if status == "working" { None } else { it.message.clone() };
        (self.progress)(Progress { op_id: op.id.clone(), index: i, total: op.items.len(), label: it.label.clone(), status, message, done });
    }
}

/// Step 1: write metadata into the temp file and verify it.
fn step_write(ctx: &Ctx, op: &mut Op, i: usize) -> Result<(), String> {
    let it = &op.items[i];
    let temp = Path::new(&it.temp).to_path_buf();
    if temp.exists() {
        let _ = std::fs::remove_file(&temp);
    }
    let mut args = it.args.clone();
    args.push("-o".into());
    args.push(path_arg(&temp));
    args.push(path_arg(Path::new(&it.original)));
    let out = ctx.engine.run(&args)?;
    let (warnings, errors) = split_messages(&out.stderr);
    let created = out.stdout.contains("1 image files created") || temp.exists();
    if !errors.is_empty() || !created {
        let _ = std::fs::remove_file(&temp);
        let errs = if errors.is_empty() {
            let mut why: Vec<String> = warnings.clone();
            why.extend(out.stdout.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from));
            if why.is_empty() { vec!["no file was produced".to_string()] } else { why }
        } else {
            errors
        };
        return Err(human_engine_error(&errs));
    }
    if let Err(e) = verify(ctx.engine, &temp, &it.expect) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    let it = &mut op.items[i];
    it.details.extend(warnings);
    it.temp_stamp = fsops::stamp(&temp).ok();
    ctx.journal.log_stamps(op, i);
    ctx.journal.log_step(op, i, 1);
    Ok(())
}

/// Steps 2–5 from wherever the item is (see the step machine in journal.rs).
fn step_swap(ctx: &Ctx, op: &mut Op, i: usize) -> Result<(), String> {
    let it = op.items[i].clone();
    let p = |s: &str| Path::new(s).to_path_buf();
    if it.step < 2 {
        fsops::move_file(&p(&it.source), &p(&it.source_renamed))?;
        ctx.journal.log_step(op, i, 2);
    }
    if it.step < 3 {
        let (temp, fin) = (p(&it.temp), p(&it.final_path));
        if same_path(&it.final_path, &it.original) && fin.exists() {
            // Copy where the Converted file already has the final name: replace it in one
            // atomic step (the temp sits in the same folder).
            fsops::replace_file(&temp, &fin)?;
        } else {
            fsops::move_file(&temp, &fin)?;
        }
        ctx.journal.log_step(op, i, 3);
    }
    if it.step < 4 {
        if let Some(t) = &it.final_times
            && fsops::set_times(&p(&it.final_path), t).is_err()
        {
            op.items[i].warnings.push("The file's dates couldn't be set.".into());
        }
        ctx.journal.log_step(op, i, 4);
    }
    if it.step < 5 {
        // Copy: the pre-change Converted file is now redundant (D15). Removed last, only after
        // the updated file is safely in place.
        let orig = p(&it.original);
        if !same_path(&it.original, &it.final_path)
            && !same_path(&it.original, &it.source)
            && orig.exists()
            && let Err(e) = std::fs::remove_file(fsops::long_path(&orig))
        {
            op.items[i].warnings.push(format!("The old converted copy couldn't be removed: {}", fsops::describe_io(&e, &orig)));
        }
        let item = &mut op.items[i];
        item.final_stamp = fsops::stamp(&p(&item.final_path)).ok();
        item.source_renamed_stamp = fsops::stamp(&p(&item.source_renamed)).ok();
        ctx.journal.log_stamps(op, i);
        ctx.journal.log_step(op, i, 5);
    }
    Ok(())
}

/// Checks the files still look like they did at Preview time, and nothing would be overwritten.
fn recheck(op: &Op, i: usize) -> Result<(), String> {
    let it = &op.items[i];
    let changed = |path: &str, want: fsops::Stamp| fsops::stamp(Path::new(path)).map(|s| s != want).unwrap_or(true);
    for (path, stamp) in [(&it.original, it.original_stamp), (&it.source, it.source_stamp)] {
        if changed(path, stamp) {
            return Err(format!("{} changed since the preview. Go back and preview again.", file_name(Path::new(path))));
        }
    }
    if Path::new(&it.source_renamed).exists() {
        return Err(format!("{} appeared since the preview, so nothing was overwritten.", file_name(Path::new(&it.source_renamed))));
    }
    let fin = Path::new(&it.final_path);
    let is_ours = fsops::same_file(fin, Path::new(&it.original)) || fsops::same_file(fin, Path::new(&it.source));
    if fin.exists() && !is_ours {
        return Err(format!("A file named {} appeared since the preview, so nothing was overwritten.", file_name(fin)));
    }
    Ok(())
}

fn run_item(ctx: &Ctx, op: &mut Op, i: usize) -> Result<(), String> {
    recheck(op, i)?;
    step_write(ctx, op, i)?;
    step_swap(ctx, op, i)
}

/// Runs a whole operation. Each item is finished or rolled back before the next starts.
pub fn run(ctx: &Ctx, op: &mut Op) -> Result<(), String> {
    ctx.journal.create(op)?;
    let mut done = 0;
    let mut stop = false;
    for i in 0..op.items.len() {
        if op.items[i].status != ItemStatus::Pending {
            continue;
        }
        if stop || ctx.cancel.load(Ordering::SeqCst) {
            op.items[i].status = ItemStatus::Skipped;
            op.items[i].message = Some(if stop { "Not started because an earlier file failed." } else { "Not started because you cancelled." }.into());
            ctx.journal.log_item(op, i);
            ctx.report(op, i, "skipped", done);
            continue;
        }
        ctx.report(op, i, "working", done);
        match run_item(ctx, op, i) {
            Ok(()) => {
                op.items[i].status = ItemStatus::Done;
                ctx.journal.log_item(op, i);
            }
            Err(msg) => {
                let rb = rollback_item(&op.items[i]);
                let it = &mut op.items[i];
                it.status = ItemStatus::Failed;
                it.message = Some(match rb {
                    Ok(()) => msg,
                    Err(e) => format!("{msg} Putting things back also hit a problem: {e}"),
                });
                ctx.journal.log_step(op, i, 0);
                ctx.journal.log_item(op, i);
                stop = ctx.stop_on_failure;
            }
        }
        done += 1;
        let it = &op.items[i];
        let status = match it.status {
            ItemStatus::Done if !it.warnings.is_empty() => "warning",
            ItemStatus::Done => "done",
            _ => "failed",
        };
        ctx.report(op, i, status, done);
    }
    let cancelled = ctx.cancel.load(Ordering::SeqCst);
    ctx.journal.log_status(op, if cancelled { OpStatus::Cancelled } else { OpStatus::Completed });
    ctx.journal.write_full(op)
}

/// Undo a finished operation (spec 5.5): all-or-nothing pre-check, then restore every item,
/// newest first.
pub fn undo(ctx: &Ctx, op: &mut Op) -> Result<(), String> {
    if !op.undo_available() {
        return Err(if op.version < OP_VERSION {
            "This operation was made by an older test version of the app and can't be undone automatically.".into()
        } else {
            "There's nothing to undo in this operation.".into()
        });
    }
    let blockers: Vec<String> = op.items.iter().filter(|i| i.status == ItemStatus::Done).filter_map(undo_blocker).collect();
    if !blockers.is_empty() {
        let more = if blockers.len() > MAX_BLOCKERS_SHOWN { format!(" (and {} more)", blockers.len() - MAX_BLOCKERS_SHOWN) } else { String::new() };
        return Err(format!("Undo was stopped so nothing gets lost: {}{more}", blockers[..blockers.len().min(MAX_BLOCKERS_SHOWN)].join(" ")));
    }
    let total = op.items.len();
    let mut errors = vec![];
    for i in (0..total).rev() {
        if op.items[i].status != ItemStatus::Done {
            continue;
        }
        ctx.report(op, i, "working", total - i - 1);
        let mut it = op.items[i].clone();
        it.step = 5;
        match rollback_item(&it) {
            Ok(()) => {
                op.items[i].status = ItemStatus::Undone;
                ctx.journal.log_step(op, i, 0);
                ctx.journal.log_item(op, i);
            }
            Err(e) => errors.push(format!("{}: {e}", it.label)),
        }
    }
    ctx.journal.log_status(op, OpStatus::Undone);
    ctx.journal.write_full(op)?;
    if errors.is_empty() { Ok(()) } else { Err(errors.join(" ")) }
}

/// The user's choice for an interrupted operation.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Recovery {
    /// Complete items that got past step 1; skip the rest.
    Finish,
    /// Put every item back as it was.
    RollBack,
}

/// Crash recovery (spec 5.5): finish the interrupted items or roll the whole operation back.
pub fn recover(ctx: &Ctx, op: &mut Op, action: Recovery) -> Result<(), String> {
    let mut errors = vec![];
    for i in 0..op.items.len() {
        let it = op.items[i].clone();
        match action {
            Recovery::RollBack => {
                if it.step > 0 || it.status == ItemStatus::Done {
                    let mut x = it.clone();
                    if x.status == ItemStatus::Done {
                        x.step = 5;
                    }
                    match rollback_item(&x) {
                        Ok(()) => op.items[i].status = ItemStatus::RolledBack,
                        Err(e) => errors.push(format!("{}: {e}", it.label)),
                    }
                    ctx.journal.log_step(op, i, 0);
                } else if it.status == ItemStatus::Pending {
                    op.items[i].status = ItemStatus::Skipped;
                    op.items[i].message = Some("Not started before the app closed.".into());
                }
                ctx.journal.log_item(op, i);
            }
            Recovery::Finish => {
                if it.status != ItemStatus::Pending {
                    continue;
                }
                let result = match it.step {
                    0 => {
                        op.items[i].status = ItemStatus::Skipped;
                        op.items[i].message = Some("Not started before the app closed.".into());
                        let _ = rollback_item(&it);
                        ctx.journal.log_item(op, i);
                        continue;
                    }
                    5 => Ok(()),
                    s if s < 3 && !Path::new(&it.temp).exists() => Err("The half-finished file was missing, so this one was put back as it was.".to_string()),
                    _ => step_swap(ctx, op, i),
                };
                match result {
                    Ok(()) => op.items[i].status = ItemStatus::Done,
                    Err(e) => {
                        let _ = rollback_item(&op.items[i]);
                        op.items[i].status = ItemStatus::Failed;
                        op.items[i].message = Some(e);
                        ctx.journal.log_step(op, i, 0);
                    }
                }
                ctx.journal.log_item(op, i);
            }
        }
    }
    ctx.journal.log_status(op, if action == Recovery::RollBack { OpStatus::RolledBack } else { OpStatus::Completed });
    ctx.journal.write_full(op)?;
    if errors.is_empty() { Ok(()) } else { Err(errors.join(" ")) }
}

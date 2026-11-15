//! Operation journal (spec 5.5, T10): the Op/Item model and its persistence.
//!
//! Each operation is `journal/<id>.json` (written in full before any file is touched) plus an
//! append-only `journal/<id>.log` of step/status entries (fsync'd), so progress survives a
//! crash without rewriting the whole plan per file.
//!
//! Item step machine (both Copy and Edit, D15):
//!   0 planned → 1 temp written & verified → 2 `source` renamed to visible `_original`
//!   → 3 temp moved to the final path → 4 file dates applied
//!   → 5 Copy only: the now-redundant pre-change Converted file is removed (done).
//! For Edit, `source == original == final`: the edited file itself becomes `_original`.

use crate::platform::fsops::{FileTimes, Stamp};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OpKind {
    Copy,
    Edit,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OpStatus {
    Running,
    Completed,
    Cancelled,
    Interrupted,
    Undone,
    RolledBack,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ItemStatus {
    Pending,
    Done,
    Failed,
    Skipped,
    Undone,
    RolledBack,
}

/// One file (Copy pair or edited file) in an operation, with everything needed to finish or
/// reverse it after a crash.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub index: usize,
    pub label: String,
    pub status: ItemStatus,
    pub step: u8,
    pub message: Option<String>,
    pub warnings: Vec<String>,
    /// Raw ExifTool warning text, shown under "Details" (spec 11).
    pub details: Vec<String>,
    /// The file that becomes `_original` (Copy: the Source; Edit: the edited file).
    pub source: String,
    /// Where `source` is moved to: the visible `name_original`.
    pub source_renamed: String,
    pub source_times: FileTimes,
    pub source_stamp: Stamp,
    /// The file ExifTool reads (Copy: the Converted file; Edit: the edited file).
    pub original: String,
    pub original_times: FileTimes,
    pub original_stamp: Stamp,
    /// Hidden-name temp file next to `original` that ExifTool writes (`.<stem>.mce-tmp-…`).
    pub temp: String,
    pub final_path: String,
    /// File dates to apply to the final file.
    pub final_times: Option<FileTimes>,
    /// Size/mtime of the verified temp file (identifies our file if a crash hit mid-move).
    pub temp_stamp: Option<Stamp>,
    /// Recorded after completion, to block Undo if files were changed later.
    pub final_stamp: Option<Stamp>,
    /// Recorded after completion, to block Undo if the `_original` was changed later.
    pub source_renamed_stamp: Option<Stamp>,
    /// ExifTool arguments (before `-o TEMP ORIGINAL`), kept so recovery can finish the item.
    pub args: Vec<String>,
    /// Tag → expected value checks after writing (spec 5.5 step 2 "key tags re-read correctly").
    pub expect: Vec<(String, String)>,
}

/// One Copy or Edit batch.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Op {
    pub id: String,
    pub kind: OpKind,
    pub created_ms: i64,
    pub finished_ms: Option<i64>,
    pub status: OpStatus,
    pub title: String,
    pub items: Vec<Item>,
    /// Journal format. Operations from older test builds (no version) can't be undone.
    #[serde(default)]
    pub version: u32,
}

/// Current journal format (D15 step machine). Older ops are listed but can't be undone.
pub const OP_VERSION: u32 = 2;

impl Op {
    /// A new running operation.
    pub fn new(id: String, kind: OpKind, title: String, items: Vec<Item>) -> Self {
        Op { id, kind, created_ms: now_ms(), finished_ms: None, status: OpStatus::Running, title, items, version: OP_VERSION }
    }

    /// Counts of items by outcome, for Done and History.
    pub fn summary(&self) -> Summary {
        let mut s = Summary::default();
        for i in &self.items {
            match i.status {
                ItemStatus::Done if !i.warnings.is_empty() => s.warnings += 1,
                ItemStatus::Done => s.ok += 1,
                ItemStatus::Failed => s.failed += 1,
                ItemStatus::Skipped | ItemStatus::Pending => s.skipped += 1,
                ItemStatus::Undone | ItemStatus::RolledBack => s.undone += 1,
            }
        }
        s
    }

    /// Current-format, finished, with at least one item done.
    pub fn undo_available(&self) -> bool {
        self.version >= OP_VERSION
            && matches!(self.status, OpStatus::Completed | OpStatus::Cancelled)
            && self.items.iter().any(|i| i.status == ItemStatus::Done)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub ok: usize,
    pub warnings: usize,
    pub failed: usize,
    pub skipped: usize,
    pub undone: usize,
}

/// One line of the append-only log.
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "t", rename_all = "camelCase")]
enum Entry {
    Step { i: usize, step: u8 },
    Item { i: usize, status: ItemStatus, message: Option<String>, warnings: Vec<String>, details: Vec<String> },
    Stamps { i: usize, temp_stamp: Option<Stamp>, final_stamp: Option<Stamp>, source_renamed_stamp: Option<Stamp> },
    Op { status: OpStatus, finished_ms: Option<i64> },
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Sortable, unique-per-process id: `YYYYMMDD-HHMMSS-<pid><counter>`.
pub fn new_id() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let t = chrono::Local::now().format("%Y%m%d-%H%M%S");
    format!("{t}-{:04x}{n:02x}", std::process::id() & 0xffff)
}

/// The journal folder in app data.
pub struct Journal {
    dir: PathBuf,
}

impl Journal {
    pub fn new(base: &Path) -> Self {
        let dir = base.join("journal");
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    fn json_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
    fn log_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.log"))
    }

    /// Writes the full plan *before* touching any file (spec 5.5 step 1).
    pub fn create(&self, op: &Op) -> Result<(), String> {
        self.write_full(op)?;
        let _ = std::fs::remove_file(self.log_path(&op.id));
        Ok(())
    }

    /// Rewrites the op JSON atomically (write temp, fsync, rename over) and drops the log.
    pub fn write_full(&self, op: &Op) -> Result<(), String> {
        let path = self.json_path(&op.id);
        let tmp = path.with_extension("json.tmp");
        let save_err = |e: std::io::Error| format!("Couldn't save the operation journal: {e}");
        let data = serde_json::to_vec_pretty(op).map_err(|e| e.to_string())?;
        {
            let mut f = std::fs::File::create(&tmp).map_err(save_err)?;
            f.write_all(&data).map_err(save_err)?;
            f.sync_all().map_err(save_err)?;
        }
        std::fs::rename(&tmp, &path).map_err(save_err)?;
        let _ = std::fs::remove_file(self.log_path(&op.id));
        Ok(())
    }

    /// Appends one fsync'd log line. Best effort: a failed write only loses resumability.
    fn append(&self, id: &str, e: &Entry) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.log_path(id))
            && let Ok(line) = serde_json::to_string(e)
        {
            let _ = writeln!(f, "{line}");
            let _ = f.sync_data();
        }
    }

    /// Records that item `i` reached `step`.
    pub fn log_step(&self, op: &mut Op, i: usize, step: u8) {
        op.items[i].step = step;
        self.append(&op.id, &Entry::Step { i, step });
    }

    /// Records item `i`'s status, message and warnings.
    pub fn log_item(&self, op: &Op, i: usize) {
        let it = &op.items[i];
        self.append(&op.id, &Entry::Item { i, status: it.status, message: it.message.clone(), warnings: it.warnings.clone(), details: it.details.clone() });
    }

    /// Records item `i`'s size/mtime stamps.
    pub fn log_stamps(&self, op: &Op, i: usize) {
        let it = &op.items[i];
        self.append(&op.id, &Entry::Stamps { i, temp_stamp: it.temp_stamp, final_stamp: it.final_stamp, source_renamed_stamp: it.source_renamed_stamp });
    }

    /// Sets and records the operation status (and finish time unless running).
    pub fn log_status(&self, op: &mut Op, status: OpStatus) {
        op.status = status;
        if status != OpStatus::Running {
            op.finished_ms = Some(now_ms());
        }
        self.append(&op.id, &Entry::Op { status, finished_ms: op.finished_ms });
    }

    /// The op JSON with the log replayed on top.
    pub fn load(&self, id: &str) -> Result<Op, String> {
        let data = std::fs::read(self.json_path(id)).map_err(|_| "That operation's journal couldn't be found.".to_string())?;
        let mut op: Op = serde_json::from_slice(&data).map_err(|e| format!("The journal is damaged: {e}"))?;
        if let Ok(log) = std::fs::read_to_string(self.log_path(id)) {
            for line in log.lines() {
                let Ok(e) = serde_json::from_str::<Entry>(line) else { continue };
                match e {
                    Entry::Step { i, step } if i < op.items.len() => op.items[i].step = step,
                    Entry::Item { i, status, message, warnings, details } if i < op.items.len() => {
                        let it = &mut op.items[i];
                        it.status = status;
                        it.message = message;
                        it.warnings = warnings;
                        it.details = details;
                    }
                    Entry::Stamps { i, temp_stamp, final_stamp, source_renamed_stamp } if i < op.items.len() => {
                        let it = &mut op.items[i];
                        it.temp_stamp = temp_stamp;
                        it.final_stamp = final_stamp;
                        it.source_renamed_stamp = source_renamed_stamp;
                    }
                    Entry::Op { status, finished_ms } => {
                        op.status = status;
                        op.finished_ms = finished_ms;
                    }
                    _ => {}
                }
            }
        }
        Ok(op)
    }

    /// Every readable operation, newest first.
    pub fn list(&self) -> Vec<Op> {
        let mut ops = vec![];
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if let Some(id) = name.strip_suffix(".json")
                    && let Ok(op) = self.load(id)
                {
                    ops.push(op);
                }
            }
        }
        ops.sort_by_key(|o| std::cmp::Reverse(o.created_ms));
        ops
    }

    /// Marks operations left "running" by a crash or kill as interrupted (spec 5.5, 11).
    pub fn mark_interrupted(&self) {
        for mut op in self.list().into_iter().filter(|o| o.status == OpStatus::Running) {
            self.log_status(&mut op, OpStatus::Interrupted);
            let _ = self.write_full(&op);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A Copy item in `dir`: Source `IMG.jpg`, Converted `conv/IMG.jpg`.
    pub(crate) fn item(dir: &Path) -> Item {
        let s = |n: &str| dir.join(n).to_string_lossy().into_owned();
        Item {
            index: 0,
            label: "x".into(),
            status: ItemStatus::Pending,
            step: 0,
            message: None,
            warnings: vec![],
            details: vec![],
            source: s("IMG.jpg"),
            source_renamed: s("IMG_original.jpg"),
            source_times: FileTimes::default(),
            source_stamp: Stamp::default(),
            original: s("conv/IMG.jpg"),
            original_times: FileTimes::default(),
            original_stamp: Stamp::default(),
            temp: s("conv/.IMG.mce-tmp.jpg"),
            final_path: s("IMG.jpg"),
            final_times: None,
            temp_stamp: None,
            final_stamp: None,
            source_renamed_stamp: None,
            args: vec![],
            expect: vec![],
        }
    }

    #[test]
    fn journal_replays_log() {
        let d = tempfile::tempdir().unwrap();
        let j = Journal::new(d.path());
        let mut op = Op::new("t1".into(), OpKind::Copy, "t".into(), vec![item(d.path())]);
        j.create(&op).unwrap();
        j.log_step(&mut op, 0, 3);
        op.items[0].status = ItemStatus::Failed;
        op.items[0].message = Some("nope".into());
        j.log_item(&op, 0);
        let loaded = j.load("t1").unwrap();
        assert_eq!(loaded.items[0].step, 3);
        assert_eq!(loaded.items[0].status, ItemStatus::Failed);
        assert_eq!(loaded.items[0].message.as_deref(), Some("nope"));
        assert_eq!(loaded.status, OpStatus::Running);
    }
}

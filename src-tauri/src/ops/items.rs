//! Turns Preview plans into journal items, ready for the executor.

use super::journal::{Item, ItemStatus};
use crate::planning::PairState;
use crate::planning::copy::CopyPlan;
use crate::planning::edit::EditPlan;
use crate::platform::fsops::file_name;
use std::path::{Path, PathBuf};

/// Temp file ExifTool writes next to `p`: `.<stem>.mce-tmp-<op>-<i>.<ext>`. It is deliberately
/// not marked hidden: Windows keeps that attribute through the final rename (T18).
fn temp_name(p: &Path, op: &str, i: usize) -> PathBuf {
    let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    dir.join(format!(".{stem}.mce-tmp-{op}-{i}{ext}"))
}

/// Items for every ready Copy pair the user didn't skip. `source` is the Source file and
/// `original` the Converted file (ExifTool reads it and writes next to it).
pub fn copy_items(plan: &CopyPlan, op_id: &str, skip: &[String]) -> Vec<Item> {
    plan.pairs
        .iter()
        .filter(|p| p.state == PairState::Ready && !skip.contains(&p.id))
        .enumerate()
        .map(|(i, p)| Item {
            index: i,
            label: file_name(Path::new(&p.final_path)),
            status: ItemStatus::Pending,
            step: 0,
            message: None,
            warnings: p.warnings.iter().map(|w| w.message.clone()).collect(),
            details: p.details.clone(),
            source: p.source.clone(),
            source_renamed: p.source_renamed.clone(),
            source_times: p.exec.source_times,
            source_stamp: p.exec.source_stamp,
            original: p.converted.clone(),
            original_times: p.exec.conv_times,
            original_stamp: p.exec.conv_stamp,
            temp: temp_name(Path::new(&p.converted), op_id, i).to_string_lossy().into_owned(),
            final_path: p.final_path.clone(),
            final_times: Some(p.exec.source_times),
            temp_stamp: None,
            final_stamp: None,
            source_renamed_stamp: None,
            args: p.exec.args.clone(),
            expect: p.exec.expect.clone(),
        })
        .collect()
}

/// Items for every ready Edit file that has changes. The edited file is `source`, `original`
/// and `final_path` at once: it becomes `name_original` and the updated copy takes its name (D15).
pub fn edit_items(plan: &EditPlan, op_id: &str) -> Vec<Item> {
    plan.items
        .iter()
        .filter(|p| p.state == PairState::Ready && !p.no_change)
        .enumerate()
        .map(|(i, p)| Item {
            index: i,
            label: p.name.clone(),
            status: ItemStatus::Pending,
            step: 0,
            message: None,
            warnings: p.warnings.iter().map(|w| w.message.clone()).collect(),
            details: vec![],
            source: p.path.clone(),
            source_renamed: p.backup_path.clone(),
            source_times: p.exec.times,
            source_stamp: p.exec.stamp,
            original: p.path.clone(),
            original_times: p.exec.times,
            original_stamp: p.exec.stamp,
            temp: temp_name(Path::new(&p.path), op_id, i).to_string_lossy().into_owned(),
            final_path: p.path.clone(),
            final_times: Some(p.exec.final_times),
            temp_stamp: None,
            final_stamp: None,
            source_renamed_stamp: None,
            args: p.exec.args.clone(),
            expect: p.exec.expect.clone(),
        })
        .collect()
}

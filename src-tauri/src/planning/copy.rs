//! Copy planner (spec 5.1–5.5, D6, D7): builds the Preview — final names, conflicts, pre-flight
//! problems, before/after diff, per-category counts, timezone and orientation decisions — and
//! the exact ExifTool arguments Apply will run.

use super::copy_pair::{self, PairInputs};
pub use super::copy_pair::{CatCount, TzView};
use super::preflight::{file_access_problem, folder_problem, space_needed};
use super::{DiffRow, Issue, PairState, issue};
use crate::engine::Engine;
use crate::metadata::cache::MetaCache;
use crate::metadata::categories::IDS;
use crate::metadata::datetime::{DeviceTz, TzSource, parse_offset};
use crate::metadata::tags::{Kind, kind_of};
use crate::platform::fsops::{self, FileTimes, Stamp, file_name, path_key};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Longest base name accepted from "Pick another name" (bytes).
const MAX_BASE_NAME: usize = 200;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PairReq {
    pub id: String,
    pub source: String,
    pub converted: String,
    /// "Pick another name" for a conflict: new base name for the final file.
    pub final_name: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CopyReq {
    pub pairs: Vec<PairReq>,
    pub categories: Vec<String>,
    /// "+HH:MM" chosen by the user for files whose timezone had to be assumed.
    pub tz_override: Option<String>,
}

/// Execution data for one pair, kept server-side (not sent to the UI).
#[derive(Clone, Debug, Default)]
pub struct Exec {
    pub args: Vec<String>,
    pub expect: Vec<(String, String)>,
    pub source_times: FileTimes,
    pub source_stamp: Stamp,
    pub conv_times: FileTimes,
    pub conv_stamp: Stamp,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PairPlan {
    pub id: String,
    pub source: String,
    pub converted: String,
    pub source_kind: Kind,
    pub converted_kind: Kind,
    /// Where the updated Converted file ends up: the Source's folder and base name, own extension.
    pub final_path: String,
    /// What the Source is renamed to (`name_original`, `name_original_2`, ...).
    pub source_renamed: String,
    pub state: PairState,
    pub conflict_with: Option<String>,
    pub problems: Vec<Issue>,
    pub warnings: Vec<Issue>,
    /// Raw ExifTool warnings, shown under "Details".
    pub details: Vec<String>,
    pub diff: Vec<DiffRow>,
    pub categories: Vec<CatCount>,
    pub tz: Option<TzView>,
    #[serde(skip)]
    pub exec: Exec,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CopyPlan {
    pub plan_id: String,
    pub pairs: Vec<PairPlan>,
    pub ready: usize,
    pub conflicts: usize,
    pub blocked: usize,
    /// Pairs whose timezone was assumed or chosen by the user.
    pub tz_assumed: usize,
    /// Batch totals per category.
    pub categories: Vec<CatCount>,
}

/// Validates a user-chosen base name ("Pick another name").
fn valid_base_name(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s.len() < MAX_BASE_NAME
        && !s.chars().any(|c| matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control())
        && !s.ends_with('.')
}

fn stem_of(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn ext_of(p: &Path) -> String {
    p.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Missing files, same file twice, read-only/locked files, read-only folders and disk space.
fn preflight(source: &Path, converted: &Path) -> Vec<Issue> {
    let mut problems = vec![];
    let (src_ok, conv_ok) = (source.is_file(), converted.is_file());
    if !src_ok {
        problems.push(issue("missing", format!("{} couldn't be found.", file_name(source))));
    }
    if !conv_ok {
        problems.push(issue("missing", format!("{} couldn't be found.", file_name(converted))));
    }
    if src_ok && conv_ok && fsops::same_file(source, converted) {
        problems.push(issue("sameFile", "The Source and Converted are the same file. Pick two different files."));
    }
    for f in [source, converted] {
        if f.is_file() {
            problems.extend(file_access_problem(f));
        }
    }
    let ds = source.parent().unwrap_or(Path::new(""));
    let dc = converted.parent().unwrap_or(Path::new(""));
    for d in [ds, dc] {
        if d.is_dir() {
            problems.extend(folder_problem(d));
        }
    }
    let need = space_needed(std::fs::metadata(converted).map(|m| m.len()).unwrap_or(0));
    for d in [dc, ds] {
        if let Some(free) = fsops::free_space(d).filter(|free| *free < need) {
            problems.push(issue(
                "noSpace",
                format!(
                    "Not enough free space on that drive ({} MB free, about {} MB needed). Free some space, then try again.",
                    free / 1048576,
                    need / 1048576
                ),
            ));
            break;
        }
    }
    problems
}

/// Builds the whole Preview. Final and `_original` names are reserved across the batch, so two
/// pairs never plan the same target.
pub fn plan(engine: &Engine, cache: &MetaCache, req: &CopyReq, device: &dyn DeviceTz, plan_id: String) -> CopyPlan {
    let enabled: HashSet<String> = req.categories.iter().cloned().collect();
    let tz_override = req.tz_override.as_deref().and_then(parse_offset);
    let writable = engine.writable_tags().clone();
    let paths: Vec<PathBuf> = req.pairs.iter().flat_map(|p| [PathBuf::from(&p.source), PathBuf::from(&p.converted)]).collect();
    cache.prefetch(engine, &paths);

    let mut plan = CopyPlan { plan_id, ..Default::default() };
    let mut reserved: HashSet<String> = HashSet::new();
    for p in &req.pairs {
        let source = PathBuf::from(&p.source);
        let converted = PathBuf::from(&p.converted);
        let ds = source.parent().map(Path::to_path_buf).unwrap_or_default();
        let bs = stem_of(&source);
        let ec = ext_of(&converted);
        let mut problems = vec![];
        let mut warnings = vec![];

        let base = match p.final_name.as_deref().map(str::trim) {
            Some(n) if !n.is_empty() => {
                let n = n.strip_suffix(&format!(".{ec}")).unwrap_or(n).to_string();
                if !valid_base_name(&n) {
                    problems.push(issue("badName", "That name has characters Windows doesn't allow in file names (like / \\ : * ? \" < > |)."));
                }
                n
            }
            _ => bs.clone(),
        };
        let dot_ec = if ec.is_empty() { String::new() } else { format!(".{ec}") };
        let final_path = ds.join(format!("{base}{dot_ec}"));
        let renamed = fsops::unique_original_name(&ds, &bs, &ext_of(&source), |c| c.exists() || reserved.contains(&path_key(c)));
        reserved.insert(path_key(&renamed));

        problems.extend(preflight(&source, &converted));
        let duplicate_final = !reserved.insert(format!("final:{}", path_key(&final_path)));
        let taken = final_path.exists() && !fsops::same_file(&final_path, &source) && !fsops::same_file(&final_path, &converted);
        let conflict_with = (duplicate_final || taken).then(|| final_path.to_string_lossy().into_owned());

        let mut details = vec![];
        let mut exec = Exec {
            source_times: fsops::read_times(&source).unwrap_or_default(),
            source_stamp: fsops::stamp(&source).unwrap_or_default(),
            conv_times: fsops::read_times(&converted).unwrap_or_default(),
            conv_stamp: fsops::stamp(&converted).unwrap_or_default(),
            ..Default::default()
        };
        let (mut diff, mut cats, mut tz) = (vec![], vec![], None);
        match (cache.get(&source), cache.get(&converted)) {
            (Some(s), Some(c)) => {
                for (m, f) in [(&s, &source), (&c, &converted)] {
                    if let Some(e) = m.error() {
                        problems.push(issue("corrupt", format!("{} couldn't be read: {e}", file_name(f))));
                    }
                    if let Some(w) = m.warning() {
                        details.push(format!("{}: {w}", file_name(f)));
                    }
                }
                let b = copy_pair::build(&PairInputs {
                    source: &source,
                    converted: &converted,
                    src: &s,
                    conv: &c,
                    enabled: &enabled,
                    writable: &writable,
                    tz_override,
                    device,
                });
                problems.extend(b.problems);
                warnings.extend(b.warnings);
                exec.args = b.args;
                exec.expect = b.expect;
                (diff, cats, tz) = (b.diff, b.categories, b.tz);
            }
            _ => {
                if source.is_file() && converted.is_file() {
                    problems.push(issue("unreadable", "The metadata engine couldn't read these files."));
                }
            }
        }

        let state = if !problems.is_empty() {
            PairState::Blocked
        } else if conflict_with.is_some() {
            PairState::Conflict
        } else {
            PairState::Ready
        };
        match state {
            PairState::Ready => plan.ready += 1,
            PairState::Conflict => plan.conflicts += 1,
            PairState::Blocked => plan.blocked += 1,
        }
        if tz.as_ref().is_some_and(|t: &TzView| matches!(t.source, TzSource::Assumed | TzSource::Override)) {
            plan.tz_assumed += 1;
        }
        plan.pairs.push(PairPlan {
            id: p.id.clone(),
            source: p.source.clone(),
            converted: p.converted.clone(),
            source_kind: kind_of(&source),
            converted_kind: kind_of(&converted),
            final_path: final_path.to_string_lossy().into_owned(),
            source_renamed: renamed.to_string_lossy().into_owned(),
            state,
            conflict_with,
            problems,
            warnings,
            details,
            diff,
            categories: cats,
            tz,
            exec,
        });
    }
    plan.categories = batch_totals(&plan.pairs, &enabled);
    plan
}

/// Sums the per-pair category counts, listing each unstorable tag name once.
fn batch_totals(pairs: &[PairPlan], enabled: &HashSet<String>) -> Vec<CatCount> {
    IDS.iter()
        .map(|id| {
            let mut t = CatCount { id, enabled: enabled.contains(*id), ..Default::default() };
            for c in pairs.iter().filter_map(|p| p.categories.iter().find(|c| c.id == *id)) {
                t.copied += c.copied;
                t.kept += c.kept;
                for n in &c.not_storable {
                    if !t.not_storable.contains(n) {
                        t.not_storable.push(n.clone());
                    }
                }
            }
            t
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_base_name("My photo 1"));
        assert!(!valid_base_name("a/b"));
        assert!(!valid_base_name(" "));
        assert!(!valid_base_name("trailing."));
    }
}

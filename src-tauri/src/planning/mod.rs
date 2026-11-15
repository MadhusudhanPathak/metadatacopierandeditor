//! Preview planning (spec 4.2, 4.3, 5.1–5.5): pairing, and the Copy and Edit plans with the
//! exact ExifTool arguments Apply will run. Nothing in this layer changes files.

pub mod copy;
mod copy_pair;
pub mod edit;
mod guards;
pub mod pairing;
mod preflight;

use crate::metadata::fields::{self, ALL_FIELDS, Field, Fields};
use serde::Serialize;

/// A problem (blocks the item) or warning (shown, never blocks), with a stable code for the UI.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: &'static str,
    pub message: String,
}

pub(crate) fn issue(code: &'static str, message: impl Into<String>) -> Issue {
    Issue { code, message: message.into() }
}

/// One curated field before and after the change, for the Preview/Review tables.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub field: Field,
    pub label: &'static str,
    pub before: Option<String>,
    pub after: Option<String>,
    pub note: Option<String>,
}

/// Whether an item will run when the user presses Apply.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PairState {
    Ready,
    /// The final name is taken; the user must skip or pick another name.
    Conflict,
    Blocked,
}

/// Rows for every field that has a value before or after (or a note).
pub fn diff_rows(before: &Fields, after: &Fields, notes: &[(Field, String)]) -> Vec<DiffRow> {
    ALL_FIELDS
        .iter()
        .filter_map(|&f| {
            let b = fields::display(before, f);
            let a = fields::display(after, f);
            let note = notes.iter().find(|(x, _)| *x == f).map(|(_, n)| n.clone());
            (b.is_some() || a.is_some() || note.is_some()).then_some(DiffRow { field: f, label: fields::field_label(f), before: b, after: a, note })
        })
        .collect()
}

//! Applying plans to files (spec 5.4, 5.5): the journal, the executor with its step machine,
//! rollback, Undo and crash recovery.

pub mod executor;
pub mod items;
pub mod journal;
pub mod rollback;
mod verify;

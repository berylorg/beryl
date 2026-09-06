#![cfg(feature = "test-faults")]

#[path = "draft_edit_history/accounting_recovery.rs"]
mod accounting_recovery;
#[path = "draft_edit_history/adoption.rs"]
mod adoption;
#[path = "draft_edit_history/creation.rs"]
mod creation;
#[path = "draft_edit_history/session_faults.rs"]
mod session_faults;
#[path = "draft_edit_history/support.rs"]
mod support;

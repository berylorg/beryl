#![cfg(feature = "test-faults")]

#[path = "phase146_draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

#[path = "phase176_abandon_fresh_candidate/codec.rs"]
mod codec;
#[path = "phase176_abandon_fresh_candidate/concurrency.rs"]
mod concurrency;
#[path = "phase176_abandon_fresh_candidate/reconciliation.rs"]
mod reconciliation;
#[path = "phase176_abandon_fresh_candidate/rejection.rs"]
mod rejection;
#[path = "phase176_abandon_fresh_candidate/shared.rs"]
mod shared;
#[path = "phase176_abandon_fresh_candidate/success.rs"]
mod success;

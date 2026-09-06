#![cfg(feature = "test-faults")]

#[path = "draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

#[path = "abandon_fresh_candidate/codec.rs"]
mod codec;
#[path = "abandon_fresh_candidate/concurrency.rs"]
mod concurrency;
#[path = "abandon_fresh_candidate/reconciliation.rs"]
mod reconciliation;
#[path = "abandon_fresh_candidate/rejection.rs"]
mod rejection;
#[path = "abandon_fresh_candidate/shared.rs"]
mod shared;
#[path = "abandon_fresh_candidate/success.rs"]
mod success;

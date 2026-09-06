#![cfg(feature = "test-faults")]

#[path = "draft_edit_history_retention/common.rs"]
mod edit_support;
#[path = "abandon_fresh_candidate/shared.rs"]
#[allow(dead_code)]
mod publication_support;
#[path = "draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

#[path = "submission_disposal/acceptance.rs"]
mod acceptance;
#[path = "submission_disposal/faults.rs"]
mod faults;
#[path = "submission_disposal/fixture.rs"]
mod fixture;

#![cfg(feature = "test-faults")]

#[path = "draft_edit_history_retention/common.rs"]
mod edit_support;
#[path = "abandon_fresh_candidate/shared.rs"]
#[allow(dead_code)]
mod publication_support;
#[path = "draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

#[path = "editor_checkpoint/checkpoint.rs"]
mod checkpoint;
#[path = "editor_checkpoint/disposal.rs"]
mod disposal;
#[path = "editor_checkpoint/publication.rs"]
mod publication;

#[path = "submission_disposal/fixture.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "editor_checkpoint/submission.rs"]
mod submission;

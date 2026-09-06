#![cfg(feature = "test-faults")]
#![allow(dead_code, unused_imports)]

#[path = "draft_edit_history/support.rs"]
mod support;

#[path = "draft_edit_history_retention/authentication.rs"]
mod authentication;
#[path = "draft_edit_history_retention/common.rs"]
mod common;
#[path = "draft_edit_history_retention/lineage.rs"]
mod lineage;
#[path = "draft_edit_history_retention/recovery.rs"]
mod recovery;
#[path = "draft_edit_history_retention/saturation.rs"]
mod saturation;

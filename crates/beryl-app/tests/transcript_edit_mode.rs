#![allow(dead_code, private_interfaces, unused_imports)]

mod shell {
    #[path = "../../src/shell/composer_draft.rs"]
    mod composer_draft;
    #[path = "../../src/shell/composer_image_labels.rs"]
    mod composer_image_labels;
    #[path = "../../src/shell/execution_detail.rs"]
    mod execution_detail;
    #[path = "../../src/shell/transcript_branch_menu_state.rs"]
    mod transcript_branch_menu_state;
    #[path = "../../src/shell/transcript_edit_menu_state.rs"]
    mod transcript_edit_menu_state;
    #[path = "../../src/shell/transcript_edit_mode_state.rs"]
    mod transcript_edit_mode_state;
    #[path = "../../src/shell/transcript_presentation.rs"]
    mod transcript_presentation;
    #[path = "../../src/shell/transcript_projection.rs"]
    mod transcript_projection;
    #[allow(dead_code)]
    #[path = "../../src/shell/virtual_list/mod.rs"]
    mod virtual_list;

    pub(super) use self::transcript_branch_menu_state::TranscriptBranchMenuState;
    pub(super) use self::transcript_edit_menu_state::{
        TranscriptEditMenuEntry, TranscriptEditRequest, TranscriptEditTarget,
    };
    pub(super) use self::transcript_edit_mode_state::{
        TranscriptEditModeState, TranscriptEditSubmitContext, TranscriptEditSubmitRoute,
        cancel_transcript_edit_mode_slot, transcript_edit_submit_route,
    };
}

use gpui::{point, px};
use shell::{
    TranscriptBranchMenuState, TranscriptEditMenuEntry, TranscriptEditModeState,
    TranscriptEditRequest, TranscriptEditSubmitContext, TranscriptEditSubmitRoute,
    TranscriptEditTarget, cancel_transcript_edit_mode_slot, transcript_edit_submit_route,
};

#[test]
fn edit_mode_carries_target_seed_and_dim_boundaries() {
    let target = edit_target();
    let mode = TranscriptEditModeState::from_request(TranscriptEditRequest::for_test(target));

    assert_eq!(mode.source_thread_id(), "thread_a");
    assert_eq!(mode.source_turn_index(), 2);
    assert_eq!(mode.rollback_turn_count(), 3);
    assert_eq!(mode.draft_seed_text(), "Original prompt");

    let snapshot = mode.snapshot();
    assert!(!snapshot.dims_row(Some("thread_a"), 1));
    assert!(snapshot.dims_row(Some("thread_a"), 2));
    assert!(snapshot.dims_row(Some("thread_a"), 4));
    assert!(!snapshot.dims_row(Some("thread_b"), 4));
    assert!(!snapshot.dims_row(None, 4));
}

#[test]
fn edit_mode_cancel_removes_only_edit_state() {
    let mut edit_mode = Some(TranscriptEditModeState::from_request(
        TranscriptEditRequest::for_test(edit_target()),
    ));
    let composer_text = "Original prompt with user edits".to_string();

    assert!(cancel_transcript_edit_mode_slot(&mut edit_mode));
    assert!(edit_mode.is_none());
    assert_eq!(composer_text, "Original prompt with user edits");
    assert!(!cancel_transcript_edit_mode_slot(&mut edit_mode));
}

#[test]
fn edit_request_from_turn_menu_enters_dimmed_mode_and_escape_cancel_clears_only_mode() {
    let target = edit_target();
    let mut menu = TranscriptBranchMenuState::default();
    menu.open_menu(
        None,
        Some(TranscriptEditMenuEntry::Enabled(target)),
        None,
        point(px(120.0), px(80.0)),
    );
    let request = menu
        .accept_edit()
        .expect("enabled edit row should produce request");
    let mut edit_mode = Some(TranscriptEditModeState::from_request(request));
    let snapshot = edit_mode
        .as_ref()
        .expect("edit mode should be active")
        .snapshot();
    let composer_text = "Original prompt with user edits".to_string();

    assert!(!snapshot.dims_row(Some("thread_a"), 1));
    assert!(snapshot.dims_row(Some("thread_a"), 2));
    assert!(snapshot.dims_row(Some("thread_a"), 4));
    assert!(cancel_transcript_edit_mode_slot(&mut edit_mode));
    assert!(edit_mode.is_none());
    assert_eq!(composer_text, "Original prompt with user edits");
}

#[test]
fn edit_mode_invalidates_on_thread_or_active_state_changes() {
    let mode =
        TranscriptEditModeState::from_request(TranscriptEditRequest::for_test(edit_target()));

    assert!(mode.remains_valid(Some("thread_a"), true, false, false, true));
    assert!(!mode.remains_valid(Some("thread_b"), true, false, false, true));
    assert!(!mode.remains_valid(Some("thread_a"), false, false, false, true));
    assert!(!mode.remains_valid(Some("thread_a"), true, true, false, true));
    assert!(!mode.remains_valid(Some("thread_a"), true, false, true, true));
    assert!(!mode.remains_valid(Some("thread_a"), true, false, false, false));
}

#[test]
fn edit_mode_submit_route_takes_precedence_over_normal_destinations() {
    let mode =
        TranscriptEditModeState::from_request(TranscriptEditRequest::for_test(edit_target()));

    assert_eq!(
        transcript_edit_submit_route(
            Some(&mode),
            TranscriptEditSubmitContext {
                status_operation_active: false,
                active_turn_active: false,
                selected_thread_compaction_active: false,
            },
        ),
        Some(TranscriptEditSubmitRoute::EditCommit)
    );
    assert_eq!(
        transcript_edit_submit_route(
            Some(&mode),
            TranscriptEditSubmitContext {
                status_operation_active: true,
                active_turn_active: true,
                selected_thread_compaction_active: true,
            },
        ),
        Some(TranscriptEditSubmitRoute::EditCommit)
    );
    assert_eq!(
        transcript_edit_submit_route(
            None,
            TranscriptEditSubmitContext {
                status_operation_active: true,
                active_turn_active: true,
                selected_thread_compaction_active: true,
            },
        ),
        None
    );
}

fn edit_target() -> TranscriptEditTarget {
    TranscriptEditTarget::for_test(
        "thread_a",
        "turn_3",
        2,
        3,
        vec!["Original prompt".to_string()],
    )
}

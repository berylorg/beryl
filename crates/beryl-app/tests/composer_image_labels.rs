#![allow(dead_code)]

#[path = "../src/shell/composer_image_labels.rs"]
mod composer_image_labels;

use beryl_backend::{ThreadItem, TurnInfo, TurnStatus, UserInput, UserMessageItem};
use composer_image_labels::{
    COMPOSER_IMAGE_LABEL_MAX_THREADS, COMPOSER_IMAGE_LABEL_SCAN_ERROR_MAX_BYTES,
    ComposerImageLabelHistoryFrontier, ComposerImageLabelHistorySyncRequest,
    ComposerImageLabelObservations, ComposerImageLabelState, ComposerImagePasteReadiness,
    image_label_for_index, image_label_index,
};

#[test]
fn label_sequence_uses_spreadsheet_style_after_z() {
    assert_eq!(image_label_for_index(0), "A");
    assert_eq!(image_label_for_index(25), "Z");
    assert_eq!(image_label_for_index(26), "AA");
    assert_eq!(image_label_for_index(27), "AB");
    assert_eq!(image_label_for_index(701), "ZZ");
    assert_eq!(image_label_for_index(702), "AAA");

    assert_eq!(image_label_index("A"), Some(0));
    assert_eq!(image_label_index("Z"), Some(25));
    assert_eq!(image_label_index("AA"), Some(26));
    assert_eq!(image_label_index("AB"), Some(27));
    assert_eq!(image_label_index("ZZ"), Some(701));
    assert_eq!(image_label_index("AAA"), Some(702));
    assert_eq!(image_label_index(""), None);
    assert_eq!(image_label_index("a"), None);
}

#[test]
fn generated_backend_label_text_seeds_next_label_only_when_attached_to_image() {
    let mut state = ComposerImageLabelState::default();

    state.observe_backend_input(
        None,
        &[
            UserInput::text("Image Z:"),
            UserInput::text("not an image"),
            UserInput::text("Image Y: "),
            UserInput::local_image("/tmp/y.png"),
            UserInput::text("before Image C:"),
            UserInput::local_image("/tmp/c.png"),
        ],
    );

    assert_eq!(state.allocate(None), "Z");
}

#[test]
fn delayed_generated_backend_label_text_seeds_next_label() {
    let mut state = ComposerImageLabelState::default();

    state.observe_backend_input(
        Some("thread_1"),
        &[
            UserInput::text("Testing image paste: Image B:\nmore text after the marker"),
            UserInput::local_image("/tmp/b.png"),
        ],
    );

    assert_eq!(state.allocate(Some("thread_1")), "C");
}

#[test]
fn selected_threads_and_pending_new_thread_have_independent_sequences() {
    let mut state = ComposerImageLabelState::default();

    assert_eq!(state.allocate(Some("thread_1")), "A");
    assert_eq!(state.allocate(Some("thread_1")), "B");
    assert_eq!(state.allocate(Some("thread_2")), "A");
    assert_eq!(state.allocate(None), "A");
    assert_eq!(state.allocate(None), "B");

    state.bind_pending_new_thread_to_thread("thread_3");

    assert_eq!(state.allocate(Some("thread_3")), "C");
    assert_eq!(state.allocate(None), "A");
}

#[test]
fn loaded_thread_history_advances_thread_allocator() {
    let mut state = ComposerImageLabelState::default();
    let turn = image_turn("turn_1", "AA");

    state.observe_thread_turns("thread_1", &[turn]);

    assert_eq!(state.allocate(Some("thread_1")), "AB");
    assert_eq!(state.allocate(Some("thread_2")), "A");
}

#[test]
fn observed_discarded_tail_labels_are_not_reused_after_rollback_projection() {
    let mut state = ComposerImageLabelState::default();

    state.observe_thread_turns(
        "thread_1",
        &[image_turn("kept", "A"), image_turn("discarded", "AA")],
    );
    state.observe_thread_turns("thread_1", &[image_turn("kept", "A")]);

    assert_eq!(state.allocate(Some("thread_1")), "AB");
}

#[test]
fn existing_thread_paste_is_blocked_until_history_scan_completes() {
    let mut state = ComposerImageLabelState::default();

    state.observe_thread_turns("thread_1", &[image_turn("latest", "A")]);
    state.prepare_thread_history_scan("thread_1", true);

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Scanning
    );

    let mut observations = ComposerImageLabelObservations::default();
    observations.observe_turns(&[image_turn("older", "Z")]);
    state.finish_thread_history_scan("thread_1", observations);

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Ready
    );
    assert_eq!(state.allocate(Some("thread_1")), "AA");
}

#[test]
fn validated_cache_must_be_revalidated_before_unblocking_paste() {
    let mut state = ComposerImageLabelState::default();
    let mut observations = ComposerImageLabelObservations::default();
    observations.observe_turns(&[image_turn("turn_1", "AA")]);

    state.finish_thread_history_scan_with_frontier(
        "thread_1",
        observations,
        Some(ComposerImageLabelHistoryFrontier::new(
            Some("turn_1".to_string()),
            1,
        )),
    );

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Ready
    );

    state.mark_thread_history_needs_validation("thread_1");

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Validating
    );
    assert!(state.begin_thread_history_validation("thread_1"));
    assert!(!state.begin_thread_history_validation("thread_1"));
    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Validating
    );
}

#[test]
fn complete_cache_revalidates_when_unloaded_history_is_reported() {
    let mut state = ComposerImageLabelState::default();

    state.prepare_thread_history_scan("thread_1", false);
    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Ready
    );

    state.prepare_thread_history_scan("thread_1", true);
    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Validating
    );
    assert_eq!(
        state.selected_thread_needing_history_sync(Some("thread_1")),
        Some(ComposerImageLabelHistorySyncRequest::Validate {
            thread_id: "thread_1".to_string(),
            frontier: None,
        })
    );
    assert_eq!(
        state.selected_thread_needing_history_scan(Some("thread_1")),
        None
    );
    assert!(state.begin_thread_history_validation("thread_1"));
    assert!(!state.begin_thread_history_validation("thread_1"));
    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Validating
    );
}

#[test]
fn validation_completion_marks_cached_thread_ready_without_scan() {
    let mut state = ComposerImageLabelState::default();
    let frontier = ComposerImageLabelHistoryFrontier::new(Some("turn_1".to_string()), 1);

    state.finish_thread_history_scan_with_frontier(
        "thread_1",
        ComposerImageLabelObservations::default(),
        Some(frontier.clone()),
    );
    state.prepare_thread_history_scan("thread_1", true);

    assert!(state.begin_thread_history_validation("thread_1"));
    assert!(state.finish_thread_history_validation("thread_1", frontier));

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Ready
    );
    assert_eq!(
        state.selected_thread_needing_history_sync(Some("thread_1")),
        None
    );
}

#[test]
fn guarded_allocation_does_not_advance_when_existing_thread_history_is_not_ready() {
    let mut state = ComposerImageLabelState::default();

    state.prepare_thread_history_scan("thread_1", true);

    assert!(matches!(
        state.try_allocate(Some("thread_1")),
        Err(ComposerImagePasteReadiness::Scanning)
    ));

    state.finish_thread_history_scan("thread_1", ComposerImageLabelObservations::default());

    assert_eq!(state.try_allocate(Some("thread_1")), Ok("A".to_string()));
    assert_eq!(state.try_allocate(None), Ok("A".to_string()));
}

#[test]
fn validation_invalidation_preserves_high_water_label_state() {
    let mut state = ComposerImageLabelState::default();
    let frontier = ComposerImageLabelHistoryFrontier::new(Some("turn_1".to_string()), 1);
    let mut observations = ComposerImageLabelObservations::default();
    observations.observe_turns(&[image_turn("turn_1", "AA")]);

    state.finish_thread_history_scan_with_frontier(
        "thread_1",
        observations,
        Some(frontier.clone()),
    );
    state.mark_thread_history_needs_validation("thread_1");

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Validating
    );
    assert!(state.begin_thread_history_validation("thread_1"));
    assert!(state.finish_thread_history_validation("thread_1", frontier));
    assert_eq!(state.try_allocate(Some("thread_1")), Ok("AB".to_string()));
}

#[test]
fn invalidation_requeues_in_flight_scan_without_clobbering_high_water() {
    let mut state = ComposerImageLabelState::default();

    state.prepare_thread_history_scan("thread_1", true);
    assert!(state.begin_thread_history_scan("thread_1"));
    state.mark_thread_history_needs_validation("thread_1");

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Scanning
    );
    assert_eq!(
        state.selected_thread_needing_history_sync(Some("thread_1")),
        Some(ComposerImageLabelHistorySyncRequest::Scan {
            thread_id: "thread_1".to_string(),
        })
    );
}

#[test]
fn stale_in_flight_scan_completion_after_invalidation_does_not_unblock_paste() {
    let mut state = ComposerImageLabelState::default();
    let mut observations = ComposerImageLabelObservations::default();
    observations.observe_turns(&[image_turn("older", "Z")]);

    state.prepare_thread_history_scan("thread_1", true);
    assert!(state.begin_thread_history_scan("thread_1"));
    state.mark_thread_history_needs_validation("thread_1");

    assert!(!state.finish_in_flight_thread_history_scan_with_frontier(
        "thread_1",
        observations,
        Some(ComposerImageLabelHistoryFrontier::new(
            Some("older".to_string()),
            1,
        )),
    ));
    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Scanning
    );
    assert_eq!(
        state.try_allocate(Some("thread_1")),
        Err(ComposerImagePasteReadiness::Scanning)
    );
    assert_eq!(
        state.selected_thread_needing_history_sync(Some("thread_1")),
        Some(ComposerImageLabelHistorySyncRequest::Scan {
            thread_id: "thread_1".to_string(),
        })
    );
}

#[test]
fn stale_in_flight_validation_completion_after_invalidation_does_not_unblock_paste() {
    let mut state = ComposerImageLabelState::default();
    let frontier = ComposerImageLabelHistoryFrontier::new(Some("turn_1".to_string()), 1);

    state.finish_thread_history_scan_with_frontier(
        "thread_1",
        ComposerImageLabelObservations::default(),
        Some(frontier.clone()),
    );
    state.mark_thread_history_needs_validation("thread_1");
    assert!(state.begin_thread_history_validation("thread_1"));
    state.mark_thread_history_needs_validation("thread_1");

    assert!(!state.finish_thread_history_validation("thread_1", frontier.clone()));
    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Validating
    );
    assert_eq!(
        state.try_allocate(Some("thread_1")),
        Err(ComposerImagePasteReadiness::Validating)
    );
    assert_eq!(
        state.selected_thread_needing_history_sync(Some("thread_1")),
        Some(ComposerImageLabelHistorySyncRequest::Validate {
            thread_id: "thread_1".to_string(),
            frontier: Some(frontier),
        })
    );
}

#[test]
fn stale_in_flight_validation_scan_transition_after_invalidation_does_not_start_scan() {
    let mut state = ComposerImageLabelState::default();
    let frontier = ComposerImageLabelHistoryFrontier::new(Some("turn_1".to_string()), 1);

    state.finish_thread_history_scan_with_frontier(
        "thread_1",
        ComposerImageLabelObservations::default(),
        Some(frontier.clone()),
    );
    state.mark_thread_history_needs_validation("thread_1");
    assert!(state.begin_thread_history_validation("thread_1"));
    state.mark_thread_history_needs_validation("thread_1");

    assert!(!state.begin_thread_history_scan_after_validation("thread_1", frontier.clone()));
    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Validating
    );
    assert_eq!(
        state.selected_thread_needing_history_sync(Some("thread_1")),
        Some(ComposerImageLabelHistorySyncRequest::Validate {
            thread_id: "thread_1".to_string(),
            frontier: Some(frontier),
        })
    );
}

#[test]
fn validation_failure_keeps_existing_thread_paste_unavailable() {
    let mut state = ComposerImageLabelState::default();

    state.prepare_thread_history_scan("thread_1", false);
    state.mark_thread_history_needs_validation("thread_1");
    state.fail_thread_history_validation("thread_1", "validation unavailable");

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Unavailable {
            message: "validation unavailable".to_string(),
        }
    );
    assert_eq!(state.allocate(None), "A");
}

#[test]
fn existing_thread_without_unloaded_history_is_paste_ready() {
    let mut state = ComposerImageLabelState::default();

    state.prepare_thread_history_scan("thread_1", false);

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Ready
    );
    assert_eq!(state.allocate(Some("thread_1")), "A");
}

#[test]
fn scan_failure_keeps_existing_thread_paste_blocked() {
    let mut state = ComposerImageLabelState::default();

    state.prepare_thread_history_scan("thread_1", true);
    state.fail_thread_history_scan("thread_1", "history unavailable");

    assert_eq!(
        state.paste_readiness(Some("thread_1")),
        ComposerImagePasteReadiness::Failed {
            message: "history unavailable".to_string()
        }
    );
    assert_eq!(state.allocate(None), "A");
}

#[test]
fn retained_thread_label_state_is_capped_and_protects_touched_thread() {
    let mut state = ComposerImageLabelState::default();

    for index in 0..=COMPOSER_IMAGE_LABEL_MAX_THREADS {
        state.observe_thread_backend_input(&format!("thread_{index}"), &[]);
    }

    assert_eq!(
        state.retained_thread_count_for_test(),
        COMPOSER_IMAGE_LABEL_MAX_THREADS
    );
    assert!(!state.has_thread_for_test("thread_0"));
    assert!(state.has_thread_for_test(&format!("thread_{}", COMPOSER_IMAGE_LABEL_MAX_THREADS)));
    assert_eq!(
        state.paste_readiness(Some("thread_0")),
        ComposerImagePasteReadiness::Scanning
    );
    assert_eq!(
        state.selected_thread_needing_history_sync(Some("thread_0")),
        Some(ComposerImageLabelHistorySyncRequest::Scan {
            thread_id: "thread_0".to_string()
        })
    );
}

#[test]
fn scan_failure_message_is_truncated() {
    let mut state = ComposerImageLabelState::default();
    let message = "x".repeat(5000);

    state.fail_thread_history_scan("thread_1", message);

    let ComposerImagePasteReadiness::Failed { message } = state.paste_readiness(Some("thread_1"))
    else {
        panic!("thread should report failed scan");
    };
    assert!(message.len() <= COMPOSER_IMAGE_LABEL_SCAN_ERROR_MAX_BYTES);
    assert!(message.ends_with("..."));
}

fn image_turn(id: &str, label: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
        error: None,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![
                UserInput::text("Look "),
                UserInput::text(format!("Image {label}:")),
                UserInput::local_image(format!("/tmp/{label}.png")),
            ],
        })],
    }
}

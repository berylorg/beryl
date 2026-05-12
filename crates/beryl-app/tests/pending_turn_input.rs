#[path = "../src/shell/execution_detail.rs"]
mod execution_detail;
#[path = "../src/shell/pending_turn_input.rs"]
mod pending_turn_input;

use beryl_backend::{TurnStartOptions, UserInput};
use beryl_model::workspace::WorkspaceId;
use execution_detail::{
    TranscriptImageMarkerSpec, TranscriptImagePreviewState,
    transcript_image_source_from_local_image,
};
use pending_turn_input::{
    PENDING_ACTIVE_TURN_STEERING_MAX_FRAGMENTS, PENDING_TURN_INPUT_MAX_FRAGMENTS,
    PENDING_TURN_INPUT_MAX_PAYLOAD_BYTES, PendingActiveTurnSteeringQueue,
    PendingActiveTurnSteeringSubmissionPlan, PendingInputAdmissionError, PendingTurnInputQueue,
    PendingTurnInputSubmissionPlan, validate_pending_turn_input_fragments,
};

#[test]
fn pending_turn_input_queue_preserves_ordered_fragments_and_metadata() {
    let workspace = WorkspaceId::host_windows("C:\\work\\beryl");
    let options = TurnStartOptions::default()
        .with_model("gpt-5.1")
        .with_reasoning_effort("high");
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".to_string(),
        workspace.clone(),
        true,
        options.clone(),
        7,
        fragment("First fragment"),
    );

    assert_eq!(queue.thread_id(), "thread_1");
    assert_eq!(queue.execution_target(), &workspace);
    assert!(queue.automatic_title_generation_allowed());
    assert_eq!(queue.turn_options(), &options);
    assert_eq!(queue.turn_index(), 7);
    assert!(queue.is_for_thread("thread_1"));
    assert!(!queue.is_for_thread("thread_2"));

    assert_eq!(queue.append(fragment("Second fragment")), 1);
    assert_eq!(queue.append(fragment("Third fragment")), 2);
    assert_eq!(queue.fragment_count(), 3);
    assert!(queue.payload_bytes_lower_bound() >= "thread_1".len() + "First fragment".len());
    assert_eq!(
        fragment_texts(&queue.clone().into_fragments()),
        vec![
            "First fragment".to_string(),
            "Second fragment".to_string(),
            "Third fragment".to_string(),
        ]
    );
    assert_eq!(
        fragment_texts(&queue.into_fragments()),
        vec![
            "First fragment".to_string(),
            "Second fragment".to_string(),
            "Third fragment".to_string(),
        ]
    );
}

#[test]
fn pending_turn_input_submission_plan_starts_appends_or_rejects_by_thread() {
    let workspace = WorkspaceId::host_windows("C:\\work\\beryl");
    let queue = PendingTurnInputQueue::new(
        "thread_1".to_string(),
        workspace,
        false,
        TurnStartOptions::default(),
        7,
        fragment("First queued prompt"),
    );

    assert_eq!(
        PendingTurnInputQueue::submission_plan(None, "thread_1"),
        Some(PendingTurnInputSubmissionPlan::StartQueue)
    );
    assert_eq!(
        PendingTurnInputQueue::submission_plan(Some(&queue), "thread_1"),
        Some(PendingTurnInputSubmissionPlan::AppendToQueue {
            turn_index: 7,
            fragment_index: 1,
        })
    );
    assert_eq!(
        PendingTurnInputQueue::submission_plan(Some(&queue), "thread_2"),
        None
    );
}

#[test]
fn pending_turn_input_queue_rejects_fragment_count_and_byte_overflow() {
    let workspace = WorkspaceId::host_windows("C:\\work\\beryl");
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".to_string(),
        workspace.clone(),
        false,
        TurnStartOptions::default(),
        7,
        fragment("First queued prompt"),
    );

    for index in 1..PENDING_TURN_INPUT_MAX_FRAGMENTS {
        queue
            .try_append(fragment(&format!("fragment {index}")))
            .unwrap();
    }

    assert_eq!(
        queue.try_append(fragment("one too many")).unwrap_err(),
        PendingInputAdmissionError::TooManyFragments {
            max_fragments: PENDING_TURN_INPUT_MAX_FRAGMENTS
        }
    );

    let too_large = "x".repeat(PENDING_TURN_INPUT_MAX_PAYLOAD_BYTES + 1);
    assert!(matches!(
        PendingTurnInputQueue::try_new(
            "thread_1".to_string(),
            workspace,
            false,
            TurnStartOptions::default(),
            7,
            fragment(&too_large),
        ),
        Err(PendingInputAdmissionError::TooManyBytes { .. })
    ));
}

#[test]
fn pending_turn_input_queue_preserves_repeated_image_reference_backend_records() {
    let workspace = WorkspaceId::host_windows("C:\\work\\beryl");
    let image_fragment = execution_detail::UserInputFragment::from_backend_input_with_image_markers(
        "See [A] and [A]",
        vec![
            UserInput::text("See "),
            UserInput::text("Image A:"),
            UserInput::local_image("/tmp/a.png"),
            UserInput::text(" and "),
            UserInput::text("[Image A]"),
        ],
        vec![
            TranscriptImageMarkerSpec::new(
                "A",
                4..7,
                transcript_image_source_from_local_image(
                    "/tmp/a.png",
                    Some("asset-a".to_string()),
                    TranscriptImagePreviewState::Available,
                ),
            ),
            TranscriptImageMarkerSpec::new(
                "A",
                12..15,
                transcript_image_source_from_local_image(
                    "/tmp/a.png",
                    Some("asset-a".to_string()),
                    TranscriptImagePreviewState::Available,
                ),
            ),
        ],
    );
    let queue = PendingTurnInputQueue::new(
        "thread_1".to_string(),
        workspace,
        false,
        TurnStartOptions::default(),
        7,
        image_fragment,
    );

    let fragments = queue.into_fragments();
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0].text, "See [A] and [A]");
    let markers = fragments[0].image_markers();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].label(), "A");
    assert_eq!(markers[0].display_range(), 4..7);
    assert_eq!(markers[0].source().asset_id(), Some("asset-a"));
    assert_eq!(
        markers[0].source().preview_state(),
        TranscriptImagePreviewState::Available
    );
    assert_eq!(markers[1].label(), "A");
    assert_eq!(markers[1].display_range(), 12..15);
    assert_eq!(markers[1].source().asset_id(), Some("asset-a"));
    assert_eq!(
        fragments[0].backend_input(),
        &[
            UserInput::text("See "),
            UserInput::text("Image A:"),
            UserInput::local_image("/tmp/a.png"),
            UserInput::text(" and "),
            UserInput::text("[Image A]"),
        ]
    );
}

#[test]
fn pending_turn_input_batch_admission_is_transactional_for_steering_fallback() {
    let workspace = WorkspaceId::host_windows("C:\\work\\beryl");
    let mut queue = PendingTurnInputQueue::new(
        "thread_1".to_string(),
        workspace.clone(),
        false,
        TurnStartOptions::default(),
        7,
        fragment("First queued prompt"),
    );

    for index in 1..PENDING_TURN_INPUT_MAX_FRAGMENTS {
        queue
            .try_append(fragment(&format!("fragment {index}")))
            .unwrap();
    }

    let original_fragments = fragment_texts(&queue.clone().into_fragments());
    assert_eq!(
        validate_pending_turn_input_fragments(
            Some(&queue),
            "thread_1",
            &workspace,
            false,
            &TurnStartOptions::default(),
            8,
            &[fragment("one too many")],
        )
        .unwrap_err(),
        PendingInputAdmissionError::TooManyFragments {
            max_fragments: PENDING_TURN_INPUT_MAX_FRAGMENTS
        }
    );
    assert_eq!(
        fragment_texts(&queue.clone().into_fragments()),
        original_fragments
    );

    assert_eq!(
        validate_pending_turn_input_fragments(
            Some(&queue),
            "thread_2",
            &workspace,
            false,
            &TurnStartOptions::default(),
            8,
            &[fragment("different thread")],
        )
        .unwrap(),
        false
    );

    assert_eq!(
        validate_pending_turn_input_fragments(
            None,
            "thread_1",
            &workspace,
            false,
            &TurnStartOptions::default(),
            8,
            &[fragment("new pending turn")],
        )
        .unwrap(),
        true
    );
}

fn fragment(text: &str) -> execution_detail::UserInputFragment {
    execution_detail::UserInputFragment::text(text)
}

fn fragment_texts(fragments: &[execution_detail::UserInputFragment]) -> Vec<String> {
    fragments
        .iter()
        .map(|fragment| fragment.text.clone())
        .collect()
}

#[test]
fn pending_active_turn_steering_queue_preserves_fragments_for_one_turn() {
    let mut queue =
        PendingActiveTurnSteeringQueue::new("thread_1".to_string(), 3, "First steer".to_string());

    assert!(queue.is_for_turn("thread_1", 3));
    assert!(!queue.is_for_turn("thread_1", 4));
    assert!(!queue.is_for_turn("thread_2", 3));

    queue.append("Second steer".to_string());
    assert_eq!(queue.fragment_count(), 2);
    assert_eq!(
        queue.fragments(),
        &["First steer".to_string(), "Second steer".to_string()]
    );
    assert_eq!(
        queue.into_fragments(),
        vec!["First steer".to_string(), "Second steer".to_string()]
    );
}

#[test]
fn pending_active_turn_steering_submission_plan_starts_appends_or_rejects_by_turn() {
    let queue =
        PendingActiveTurnSteeringQueue::new("thread_1".to_string(), 3, "First steer".to_string());

    assert_eq!(
        PendingActiveTurnSteeringQueue::<String>::submission_plan(None, "thread_1", 3),
        Some(PendingActiveTurnSteeringSubmissionPlan::StartQueue)
    );
    assert_eq!(
        PendingActiveTurnSteeringQueue::submission_plan(Some(&queue), "thread_1", 3),
        Some(PendingActiveTurnSteeringSubmissionPlan::AppendToQueue)
    );
    assert_eq!(
        PendingActiveTurnSteeringQueue::submission_plan(Some(&queue), "thread_1", 4),
        None
    );
    assert_eq!(
        PendingActiveTurnSteeringQueue::submission_plan(Some(&queue), "thread_2", 3),
        None
    );
}

#[test]
fn pending_active_turn_steering_queue_rejects_fragment_count_overflow() {
    let mut queue =
        PendingActiveTurnSteeringQueue::new("thread_1".to_string(), 3, "first".to_string());

    for index in 1..PENDING_ACTIVE_TURN_STEERING_MAX_FRAGMENTS {
        queue
            .try_append(format!("fragment {index}"), String::len)
            .unwrap();
    }

    assert_eq!(
        queue
            .try_append("one too many".to_string(), String::len)
            .unwrap_err(),
        PendingInputAdmissionError::TooManyFragments {
            max_fragments: PENDING_ACTIVE_TURN_STEERING_MAX_FRAGMENTS
        }
    );
}

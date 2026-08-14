#[path = "../src/shell/execution_detail.rs"]
mod execution_detail;
#[path = "../src/shell/turn_steering.rs"]
mod turn_steering;

use beryl_backend::{JsonRpcError, ManagedBackendError, TurnSteerResponse, UserInput};
use execution_detail::{
    TranscriptImageMarkerSpec, TranscriptImagePreviewState,
    transcript_image_source_from_local_image,
};
use serde_json::json;
use turn_steering::{
    SteeringInputFragment, TurnSteeringOutcome, backend_input_for_steering_fragments,
    turn_steering_outcome_from_result,
};

#[test]
fn steering_success_finishes_without_next_turn_queue() {
    let outcome = turn_steering_outcome_from_result(
        "thread_1".to_string(),
        "turn_1",
        vec![fragment(0, 11, "Steer now")],
        Ok(TurnSteerResponse {
            turn_id: "turn_1".to_string(),
        }),
    );

    assert_eq!(outcome, TurnSteeringOutcome::Steered);
}

#[test]
fn steering_stale_turn_response_queues_fragments_for_next_turn() {
    let fragments = vec![
        fragment(0, 11, "First steer"),
        fragment(0, 12, "Second steer"),
    ];
    let outcome = turn_steering_outcome_from_result(
        "thread_1".to_string(),
        "turn_1",
        fragments.clone(),
        Ok(TurnSteerResponse {
            turn_id: "turn_2".to_string(),
        }),
    );

    let TurnSteeringOutcome::QueueForNextTurn {
        thread_id,
        fragments: queued,
        message,
    } = outcome
    else {
        panic!("expected stale steering response to queue fragments for a later turn");
    };

    assert_eq!(thread_id, "thread_1");
    assert_eq!(queued, fragments);
    assert!(message.contains("turn_2"));
    assert!(message.contains("turn_1"));
}

#[test]
fn non_steerable_error_queues_fragments_for_next_turn() {
    let fragments = vec![fragment(4, 21, "Steer during compaction")];
    let outcome = turn_steering_outcome_from_result(
        "thread_1".to_string(),
        "turn_1",
        fragments.clone(),
        Err(ManagedBackendError::RequestFailed {
            method: "turn/steer".to_string(),
            error: JsonRpcError {
                code: -32000,
                message: "active turn cannot be steered".to_string(),
                data: Some(json!({
                    "codexErrorInfo": {
                        "activeTurnNotSteerable": {
                            "turnKind": "compact"
                        }
                    }
                })),
            },
        }),
    );

    let TurnSteeringOutcome::QueueForNextTurn {
        thread_id,
        fragments: queued,
        message,
    } = outcome
    else {
        panic!("expected non-steerable response to queue fragments for a later turn");
    };

    assert_eq!(thread_id, "thread_1");
    assert_eq!(queued, fragments);
    assert!(message.contains("compact"));
}

#[test]
fn steering_fragments_preserve_ordered_backend_input_records() {
    let fragment = execution_detail::UserInputFragment::from_backend_input_with_image_markers(
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
    let steering = SteeringInputFragment::from_user_input_fragment(2, &fragment);

    assert_eq!(
        backend_input_for_steering_fragments(&[steering.clone()]),
        vec![
            UserInput::text("See "),
            UserInput::text("Image A:"),
            UserInput::local_image("/tmp/a.png"),
            UserInput::text(" and "),
            UserInput::text("[Image A]"),
        ]
    );

    let restored = steering.into_user_input_fragment();
    let markers = restored.image_markers();
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
}

fn fragment(turn_index: usize, fragment_id: u64, text: &str) -> SteeringInputFragment {
    let fragment = execution_detail::UserInputFragment::text(text);
    let mut steering = SteeringInputFragment::from_user_input_fragment(turn_index, &fragment);
    steering.fragment_id = fragment_id;
    steering
}

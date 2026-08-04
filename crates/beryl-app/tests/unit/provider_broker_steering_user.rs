#[path = "provider_broker_steering_user/support.rs"]
mod support;

use std::time::Duration;

use beryl_backend::{
    ClientUserMessageId, ManagedBackendError, OrderedTurnStreamProgress,
    OrderedTurnStreamRejection, OrderedTurnStreamSubmitCause, SteeringUserMessageError,
    UserMessageEchoLifecycle,
};
use beryl_home_store::HomeGeneration;
use beryl_model::SyndicAcceptedInputId;
use syndic_storage::SyndicDeliveringSteeringInput;

use crate::cas_projection::{
    connection::{
        CheckedSteeringLifecycleWaitError, router::LiveEventTargetCloseReason,
    },
    input_replay::encode_accepted_input_steering_correlation,
};
use support::SteeringFixture;

#[test]
fn real_marker_free_started_and_completed_publish_exact_fresh_checked_results() {
    let mut fixture = SteeringFixture::new(181);
    let correlation = fixture.correlation();
    let route = fixture.delivering_route();
    let home_generation = fixture.home.health().generation().unwrap();

    assert_eq!(
        fixture
            .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
            .unwrap(),
        OrderedTurnStreamProgress::Progress
    );
    let started = fixture
        .broker
        .as_ref()
        .unwrap()
        .take_checked_steering_lifecycle()
        .expect("Started publishes one checked result");
    assert_checked(
        &fixture,
        &started,
        &route,
        home_generation,
        UserMessageEchoLifecycle::Started,
        123,
        &correlation,
    );
    assert!(
        fixture
            .broker
            .as_ref()
            .unwrap()
            .take_checked_steering_lifecycle()
            .is_none()
    );
    assert_eq!(fixture.delivering_route(), route);

    assert_eq!(
        fixture
            .decode_lifecycle(UserMessageEchoLifecycle::Completed, &correlation)
            .unwrap(),
        OrderedTurnStreamProgress::Progress
    );
    let completed = fixture
        .broker
        .as_ref()
        .unwrap()
        .take_checked_steering_lifecycle()
        .expect("Completed publishes the second checked result");
    assert_checked(
        &fixture,
        &completed,
        &route,
        home_generation,
        UserMessageEchoLifecycle::Completed,
        124,
        &correlation,
    );
    assert_eq!(fixture.delivering_route(), route);
    assert_eq!(fixture.registration.terminal_reason(), None);

    fixture.close();
}

#[test]
fn outbound_owner_consumes_the_exact_sequence_and_releases_only_after_completion() {
    let mut fixture = SteeringFixture::new(194);
    let correlation = fixture.correlation();
    let attempt = fixture.active_attempt();
    let mut owner = fixture.take_lifecycle_owner();

    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap();
    let started = owner.wait_started(&attempt).unwrap();
    assert_eq!(
        started.message().lifecycle(),
        UserMessageEchoLifecycle::Started
    );
    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Completed, &correlation)
        .unwrap();
    let completed = owner.wait_completed(&attempt).unwrap();
    assert_eq!(
        completed.message().lifecycle(),
        UserMessageEchoLifecycle::Completed
    );

    owner.release_after_disposition().unwrap();
    attempt.finish().unwrap();
    fixture.close();
}

#[test]
fn no_lifecycle_seal_cannot_overtake_an_already_reserved_started() {
    let mut fixture = SteeringFixture::new(195);
    let correlation = fixture.correlation();
    let attempt = fixture.active_attempt();
    let mut owner = fixture.take_lifecycle_owner();
    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap();

    assert_eq!(
        owner.seal_without_lifecycle(),
        Err(CheckedSteeringLifecycleWaitError::LifecycleAlreadyReserved)
    );
    owner.wait_started(&attempt).unwrap();
    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Completed, &correlation)
        .unwrap();
    owner.wait_completed(&attempt).unwrap();
    owner.release_after_disposition().unwrap();
    attempt.finish().unwrap();
    fixture.close();
}

#[test]
fn no_lifecycle_seal_stays_occupied_until_explicit_disposition_release() {
    let mut fixture = SteeringFixture::new(196);
    let attempt = fixture.active_attempt();
    let mut owner = fixture.take_lifecycle_owner();

    owner.seal_without_lifecycle().unwrap();
    owner.release_after_disposition().unwrap();
    attempt.finish().unwrap();
    fixture.close();
}

#[test]
fn real_image_bearing_started_and_completed_replay_the_exact_local_path_fresh() {
    let mut fixture = SteeringFixture::image(190);
    let correlation = fixture.correlation();
    let route = fixture.delivering_route();
    let home_generation = fixture.home.health().generation().unwrap();
    let image_path = fixture
        .image_path()
        .expect("image fixture exposes its verified runtime path");
    assert!(!image_path.is_empty());
    let started_json = fixture.lifecycle_json(UserMessageEchoLifecycle::Started, &correlation);
    assert!(started_json.contains(r#""type":"localImage""#));
    assert!(started_json.contains(&serde_json::to_string(image_path).unwrap()));

    fixture.decode_json(&started_json).unwrap();
    let started = fixture
        .broker
        .as_ref()
        .unwrap()
        .take_checked_steering_lifecycle()
        .expect("image Started publishes one checked result");
    assert_checked(
        &fixture,
        &started,
        &route,
        home_generation,
        UserMessageEchoLifecycle::Started,
        123,
        &correlation,
    );

    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Completed, &correlation)
        .unwrap();
    let completed = fixture
        .broker
        .as_ref()
        .unwrap()
        .take_checked_steering_lifecycle()
        .expect("image Completed replays a fresh pass");
    assert_checked(
        &fixture,
        &completed,
        &route,
        home_generation,
        UserMessageEchoLifecycle::Completed,
        124,
        &correlation,
    );
    assert_eq!(fixture.delivering_route(), route);

    fixture.close();
}

#[test]
fn started_and_completed_buffer_as_one_bounded_ordered_sequence() {
    let mut fixture = SteeringFixture::new(182);
    let correlation = fixture.correlation();
    let route = fixture.delivering_route();
    let home_generation = fixture.home.health().generation().unwrap();
    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap();

    let broker = fixture.broker.as_ref().unwrap();
    assert!(broker.has_checked_steering_lifecycle());
    broker.wait_for_checked_steering_consumption(Duration::from_millis(10));
    assert!(
        broker.has_checked_steering_lifecycle(),
        "a timeout must not consume the buffered Started result"
    );

    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Completed, &correlation)
        .unwrap();
    let started = fixture
        .broker
        .as_ref()
        .unwrap()
        .take_checked_steering_lifecycle()
        .expect("the first buffered result remains Started");
    assert_checked(
        &fixture,
        &started,
        &route,
        home_generation,
        UserMessageEchoLifecycle::Started,
        123,
        &correlation,
    );
    let completed = fixture
        .broker
        .as_ref()
        .unwrap()
        .take_checked_steering_lifecycle()
        .expect("the second buffered result remains Completed");
    assert_checked(
        &fixture,
        &completed,
        &route,
        home_generation,
        UserMessageEchoLifecycle::Completed,
        124,
        &correlation,
    );
    assert!(!fixture
        .broker
        .as_ref()
        .unwrap()
        .has_checked_steering_lifecycle());
    assert_eq!(fixture.delivering_route(), route);
    assert_eq!(fixture.registration.terminal_reason(), None);

    fixture.close();
}

#[test]
fn connection_driver_poll_gate_stays_closed_until_checked_result_consumption() {
    let mut fixture = SteeringFixture::new(191);
    let correlation = fixture.correlation();
    let route = fixture.delivering_route();
    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap();

    let broker = fixture.broker.as_ref().unwrap();
    assert!(
        crate::cas_projection::connection::driver::checked_steering_blocks_stream_poll(
            broker,
            Duration::ZERO,
        ),
        "the driver must not poll while the checked result is pending"
    );
    assert!(broker.take_checked_steering_lifecycle().is_some());
    assert!(
        !crate::cas_projection::connection::driver::checked_steering_blocks_stream_poll(
            broker,
            Duration::ZERO,
        ),
        "consuming the checked result must reopen driver polling"
    );
    assert_eq!(fixture.delivering_route(), route);

    fixture.close();
}

#[test]
fn duplicate_and_reversed_lifecycles_fail_closed_through_projection_loss() {
    let mut reversed = SteeringFixture::new(183);
    let correlation = reversed.correlation();
    let error = reversed
        .decode_lifecycle(UserMessageEchoLifecycle::Completed, &correlation)
        .unwrap_err();
    assert_selection_cause(
        error,
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
    );
    reversed.converge_and_assert_projection_loss();
    reversed.close();

    let mut duplicate = SteeringFixture::new(184);
    let correlation = duplicate.correlation();
    duplicate
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap();
    assert!(
        duplicate
            .broker
            .as_ref()
            .unwrap()
            .take_checked_steering_lifecycle()
            .is_some()
    );
    let error = duplicate
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap_err();
    assert_selection_cause(
        error,
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
    );
    duplicate.converge_and_assert_projection_loss();
    duplicate.close();
}

#[test]
fn completed_sequence_remains_terminal_after_result_consumption() {
    let mut fixture = SteeringFixture::new(192);
    let correlation = fixture.correlation();

    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap();
    assert!(
        fixture
            .broker
            .as_ref()
            .unwrap()
            .take_checked_steering_lifecycle()
            .is_some()
    );
    fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Completed, &correlation)
        .unwrap();
    assert!(
        fixture
            .broker
            .as_ref()
            .unwrap()
            .take_checked_steering_lifecycle()
            .is_some()
    );

    let error = fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap_err();
    assert_selection_cause(
        error,
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
    );
    fixture.converge_and_assert_projection_loss();

    fixture.close();
}

#[test]
fn malformed_and_unknown_correlations_fail_closed_before_replay() {
    let mut malformed = SteeringFixture::new(185);
    let correlation = ClientUserMessageId::try_new("not-an-accepted-input").unwrap();
    let error = malformed
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap_err();
    assert_selection_cause(
        error,
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::SchemaMismatch),
    );
    malformed.converge_and_assert_projection_loss();
    malformed.close();

    let mut unknown = SteeringFixture::new(186);
    let correlation =
        encode_accepted_input_steering_correlation(SyndicAcceptedInputId::from_bytes([0xee; 16]));
    let error = unknown
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap_err();
    assert_selection_cause(
        error,
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
    );
    unknown.converge_and_assert_projection_loss();
    unknown.close();
}

#[test]
fn exact_router_target_mismatch_converges_the_production_target_loss() {
    let mut fixture = SteeringFixture::with_router_turn_mismatch(187);
    let correlation = fixture.correlation();

    let error = fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap_err();
    assert_selection_cause(
        error,
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::InvalidControl),
    );
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(LiveEventTargetCloseReason::StreamFailure)
    );
    fixture.converge_and_assert_projection_loss();

    fixture.close();
}

#[test]
fn durable_route_drift_after_selection_fails_the_final_recheck_closed() {
    let mut fixture = SteeringFixture::new(193);
    let correlation = fixture.correlation();

    let error = fixture
        .decode_lifecycle_while_selection_paused(
            UserMessageEchoLifecycle::Started,
            &correlation,
            SteeringFixture::retry_delivering_route,
        )
        .unwrap_err();
    assert_commit_cause(
        error,
        OrderedTurnStreamSubmitCause::Rejected(OrderedTurnStreamRejection::StagingConflict),
    );
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(LiveEventTargetCloseReason::StreamFailure)
    );
    assert!(
        fixture
            .broker
            .as_ref()
            .unwrap()
            .take_checked_steering_lifecycle()
            .is_none()
    );
    fixture.converge_and_assert_retryable_projection_loss();

    fixture.close();
}

#[test]
fn post_selection_schema_failure_abandons_the_permit_and_converges_projection_loss() {
    let mut fixture = SteeringFixture::new(188);
    let correlation = fixture.correlation();
    let valid = fixture.lifecycle_json(UserMessageEchoLifecycle::Started, &correlation);
    let malformed = valid.replace(
        r#""content":[{"type":"text","text":"delayed steering text","text_elements":[]}]"#,
        r#""content":{}"#,
    );
    assert_ne!(malformed, valid);

    let error = fixture.decode_json(&malformed).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ProviderObservation { .. }
    ));
    assert_eq!(
        fixture.registration.terminal_reason(),
        Some(LiveEventTargetCloseReason::StreamFailure)
    );
    fixture.converge_and_assert_projection_loss();

    fixture.close();
}

#[test]
fn preselection_cancellation_returns_typed_failure_and_converges_projection_loss() {
    let mut fixture = SteeringFixture::new(189);
    let correlation = fixture.correlation();
    fixture.broker.as_ref().unwrap().request_cancel();

    let error = fixture
        .decode_lifecycle(UserMessageEchoLifecycle::Started, &correlation)
        .unwrap_err();
    assert_selection_cause(error, OrderedTurnStreamSubmitCause::Cancelled);
    fixture.converge_and_assert_projection_loss();

    fixture.close();
}

fn assert_checked(
    fixture: &SteeringFixture,
    checked: &super::CheckedSteeringLifecycle,
    expected_route: &SyndicDeliveringSteeringInput,
    expected_home_generation: HomeGeneration,
    lifecycle: UserMessageEchoLifecycle,
    timestamp_ms: u64,
    correlation: &ClientUserMessageId,
) {
    assert_eq!(checked.route(), expected_route);
    assert_eq!(checked.home_generation(), expected_home_generation);
    assert_eq!(checked.route().input().id(), fixture.accepted_input_id);
    assert_eq!(checked.route().input().thread_id(), fixture.thread_id);
    assert_eq!(
        checked.route().target().pending().active_turn_id(),
        fixture.turn_id
    );
    assert_eq!(
        checked.route().target().pending().cas_thread_id(),
        &fixture.cas_thread_id
    );
    assert_eq!(checked.route().target().cas_turn_id(), &fixture.cas_turn_id);

    let message = checked.message();
    assert_eq!(message.lifecycle(), lifecycle);
    assert_eq!(message.thread_id(), &fixture.cas_thread_id);
    assert_eq!(message.turn_id(), &fixture.cas_turn_id);
    assert_eq!(message.item_id(), &fixture.cas_item_id);
    assert_eq!(message.timestamp().get(), timestamp_ms);
    assert_eq!(message.client_user_message_id(), correlation);
    assert_eq!(
        message.checked_input_items(),
        fixture.expected_input_items()
    );
}

fn assert_selection_cause(error: ManagedBackendError, expected: OrderedTurnStreamSubmitCause) {
    match error {
        ManagedBackendError::SteeringUserMessage {
            source: SteeringUserMessageError::Selection(actual),
            ..
        } => assert_eq!(actual, expected),
        error => panic!("unexpected steering selection error: {error:?}"),
    }
}

fn assert_commit_cause(error: ManagedBackendError, expected: OrderedTurnStreamSubmitCause) {
    match error {
        ManagedBackendError::SteeringUserMessage {
            source: SteeringUserMessageError::Commit(actual),
            ..
        } => assert_eq!(actual, expected),
        error => panic!("unexpected checked steering commit error: {error:?}"),
    }
}

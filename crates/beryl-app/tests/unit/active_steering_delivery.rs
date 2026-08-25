mod server {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/active_steering_delivery/server.rs"
    ));
}

mod support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/active_steering_delivery/support.rs"
    ));
}

mod submission_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/submission_fixture.rs"
    ));
}

use std::{thread, time::Instant};

use beryl_backend::{JsonRpcErrorVerdict, JsonRpcTurnKind, ManagedBackendError};
use syndic_storage::{
    AcceptedInputLifecycle, AcceptedRouteEffectiveState, BindingState, NextTurnReason,
};

use self::{
    server::{SteeringServer, SteeringServerScenario, TIMEOUT},
    support::{DeliveryFixture, RetryRaceBranch, STEERING_TEXT},
};
use super::test_support::DeliveryPause;
use super::model::{
    ActiveSteeringDeliveryError, ActiveSteeringDeliveryOutcome, ActiveSteeringProjectionLossCause,
    ActiveSteeringRetryCause, ActiveSteeringSaturationCause, ActiveSteeringUnknownCause,
};

fn wait_for_route_pair(
    fixture: &DeliveryFixture,
    first: AcceptedRouteEffectiveState,
    second: AcceptedRouteEffectiveState,
) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let diagnostics = fixture.scheduler_diagnostics();
        if fixture.route_state() == first
            && fixture.second_route_state() == second
            && diagnostics.workers_active() == 0
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "sibling-admission race did not settle: {diagnostics:?}",
        );
        thread::yield_now();
    }
}

#[test]
fn automatic_scheduler_claims_delivers_and_joins_at_minimum_capacity() {
    let server =
        SteeringServer::spawn(SteeringServerScenario::LifecycleBeforeSuccessResponse);
    let fixture = DeliveryFixture::new_scheduled(215, 4, &server, STEERING_TEXT);
    let deadline = Instant::now() + TIMEOUT;
    let (diagnostics, workers) = loop {
        let diagnostics = fixture.scheduler_diagnostics();
        let workers = fixture.worker_diagnostics();
        if fixture.route_state() == AcceptedRouteEffectiveState::Delivered
            && diagnostics.workers_active() == 0
            && diagnostics.workers_joined() == 1
            && workers.active() == 2
            && workers.available() == 2
        {
            break (diagnostics, workers);
        }
        assert!(
            Instant::now() < deadline,
            "automatic steering did not settle: {diagnostics:?}",
        );
        thread::yield_now();
    };

    assert_eq!(diagnostics.workers_started(), 1);
    assert_eq!(diagnostics.workers_high_water(), 1);
    assert_eq!(diagnostics.capacity_waits(), 0);
    assert_eq!(diagnostics.armed_retry_timers(), 0);
    assert_eq!(workers.capacity(), 4);
    assert_eq!(workers.active(), 2);
    assert_eq!(workers.available(), 2);
    assert_eq!(workers.high_water(), 4);

    fixture.close(server);
}

#[test]
fn same_source_inputs_deliver_in_accepted_order_with_one_active_attempt() {
    let server = SteeringServer::spawn(SteeringServerScenario::OrderedPairSuccess);
    let fixture = DeliveryFixture::new_scheduled_pair(218, &server);
    let deadline = Instant::now() + TIMEOUT;
    let diagnostics = loop {
        let diagnostics = fixture.scheduler_diagnostics();
        if fixture.route_state() == AcceptedRouteEffectiveState::Delivered
            && fixture.second_route_state()
                == AcceptedRouteEffectiveState::Delivered
            && diagnostics.workers_active() == 0
            && diagnostics.workers_joined() == 2
        {
            break diagnostics;
        }
        assert!(
            Instant::now() < deadline,
            "same-source steering inputs did not settle in order: {diagnostics:?}",
        );
        thread::yield_now();
    };

    assert_eq!(diagnostics.workers_started(), 2);
    assert_eq!(diagnostics.workers_high_water(), 1);
    assert!(diagnostics.attempt_waits() >= 1);

    fixture.close(server);
}

#[test]
fn sibling_admission_before_claim_preserves_order_and_scheduler_health() {
    let server = SteeringServer::spawn(SteeringServerScenario::OrderedPairSuccess);
    let mut fixture = DeliveryFixture::new(221, 5, &server, STEERING_TEXT);

    let outcome = fixture
        .deliver_with_sibling_at(DeliveryPause::BeforeDeliveryClaim)
        .unwrap();
    assert!(matches!(outcome, ActiveSteeringDeliveryOutcome::Delivered));
    wait_for_route_pair(
        &fixture,
        AcceptedRouteEffectiveState::Delivered,
        AcceptedRouteEffectiveState::Delivered,
    );
    assert!(fixture.service_is_accepting());

    fixture.close(server);
}

#[test]
fn sibling_admission_before_completion_preserves_known_success() {
    let server = SteeringServer::spawn(SteeringServerScenario::OrderedPairSuccess);
    let mut fixture = DeliveryFixture::new(222, 5, &server, STEERING_TEXT);

    let outcome = fixture
        .deliver_with_sibling_at(DeliveryPause::BeforeCompleteDisposition)
        .unwrap();
    assert!(matches!(outcome, ActiveSteeringDeliveryOutcome::Delivered));
    wait_for_route_pair(
        &fixture,
        AcceptedRouteEffectiveState::Delivered,
        AcceptedRouteEffectiveState::Delivered,
    );
    assert!(fixture.service_is_accepting());

    fixture.close(server);
}

#[test]
fn sibling_admission_before_retry_preserves_proven_non_dispatch() {
    let server = SteeringServer::spawn(SteeringServerScenario::SecondOnlySuccess);
    let mut fixture = DeliveryFixture::new(223, 5, &server, STEERING_TEXT);
    fixture.cancel();

    let outcome = fixture
        .deliver_with_sibling_at(DeliveryPause::BeforeRetryDisposition)
        .unwrap();
    assert!(matches!(
        outcome,
        ActiveSteeringDeliveryOutcome::Retryable { .. }
    ));
    wait_for_route_pair(
        &fixture,
        AcceptedRouteEffectiveState::Ready,
        AcceptedRouteEffectiveState::Delivered,
    );
    assert_eq!(fixture.route_lifecycle(), AcceptedInputLifecycle::Retryable);
    assert!(fixture.service_is_accepting());

    fixture.close(server);
}

#[test]
fn sibling_admission_before_rejection_preserves_exact_rejection() {
    let server =
        SteeringServer::spawn(SteeringServerScenario::StructuredRejectionThenSuccess);
    let mut fixture = DeliveryFixture::new(224, 5, &server, STEERING_TEXT);

    let outcome = fixture
        .deliver_with_sibling_at(DeliveryPause::BeforeRejectionDisposition)
        .unwrap();
    assert!(matches!(
        outcome,
        ActiveSteeringDeliveryOutcome::SteeringRejected { .. }
    ));
    wait_for_route_pair(
        &fixture,
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::SteeringRejected),
        AcceptedRouteEffectiveState::Delivered,
    );
    assert!(fixture.service_is_accepting());

    fixture.close(server);
}

#[test]
fn sibling_admission_before_abandonment_preserves_exact_loss_disposition() {
    let server = SteeringServer::spawn(SteeringServerScenario::UnconfirmedRejection);
    let mut fixture = DeliveryFixture::new(225, 5, &server, STEERING_TEXT);

    let outcome = fixture
        .deliver_with_sibling_at(DeliveryPause::BeforeLossAbandonment)
        .unwrap();
    assert!(matches!(
        outcome,
        ActiveSteeringDeliveryOutcome::ProjectionLost {
            cause: ActiveSteeringProjectionLossCause::UnconfirmedRejection(_),
        }
    ));
    wait_for_route_pair(
        &fixture,
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost),
    );
    assert!(fixture.service_is_accepting());

    fixture.close(server);
}

#[test]
fn durable_receipt_reconciles_after_delivery_claim_descendant() {
    let server =
        SteeringServer::spawn(SteeringServerScenario::LifecycleBeforeSuccessResponse);
    let fixture = DeliveryFixture::new_scheduled_descendant_reconciliation(
        220,
        &server,
        STEERING_TEXT,
    );
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let diagnostics = fixture.scheduler_diagnostics();
        if fixture.route_state() == AcceptedRouteEffectiveState::Delivered
            && diagnostics.workers_active() == 0
            && diagnostics.workers_joined() == 1
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "receipt-descendant delivery did not settle: {diagnostics:?}",
        );
        thread::yield_now();
    }
    assert!(fixture.service_is_accepting());

    fixture.close(server);
}

#[test]
fn parked_cancellation_ignores_ordinary_wake_until_lifecycle_renewal() {
    let server =
        SteeringServer::spawn(SteeringServerScenario::LifecycleBeforeSuccessResponse);
    let fixture =
        DeliveryFixture::new_scheduled_cancelled(216, 4, &server, STEERING_TEXT);
    let parked = fixture.scheduler_diagnostics();
    assert_eq!(fixture.route_state(), AcceptedRouteEffectiveState::Ready);
    assert_eq!(
        fixture.route_lifecycle(),
        AcceptedInputLifecycle::Retryable
    );
    assert_eq!(parked.workers_started(), 1);
    assert_eq!(parked.workers_joined(), 1);

    fixture.signal_ordinary_ready();
    let deadline = Instant::now() + TIMEOUT;
    let ordinary = loop {
        let diagnostics = fixture.scheduler_diagnostics();
        if diagnostics.pass_count() > parked.pass_count() {
            break diagnostics;
        }
        assert!(
            Instant::now() < deadline,
            "ordinary wake did not complete its ineligible scan: {diagnostics:?}",
        );
        thread::yield_now();
    };
    assert_eq!(ordinary.workers_started(), 1);
    assert_eq!(ordinary.workers_joined(), 1);
    assert_eq!(
        ordinary.retry_state(),
        crate::cas_projection::ActiveSteeringRetryState::Parked
    );

    fixture.renew_scheduler_cancellation();
    let deadline = Instant::now() + TIMEOUT;
    let delivered = loop {
        let diagnostics = fixture.scheduler_diagnostics();
        if fixture.route_state() == AcceptedRouteEffectiveState::Delivered
            && diagnostics.workers_active() == 0
            && diagnostics.workers_joined() == 2
            && diagnostics.retry_state()
                == crate::cas_projection::ActiveSteeringRetryState::Ineligible
        {
            break diagnostics;
        }
        assert!(
            Instant::now() < deadline,
            "renewed cancellation lifecycle did not reopen retry: {diagnostics:?}",
        );
        thread::yield_now();
    };
    assert_eq!(delivered.workers_started(), 2);
    assert_eq!(
        delivered.retry_state(),
        crate::cas_projection::ActiveSteeringRetryState::Ineligible
    );

    fixture.close(server);
}

#[test]
fn lifecycle_reopen_observed_before_older_parked_completion_wins_after_join() {
    let server =
        SteeringServer::spawn(SteeringServerScenario::LifecycleBeforeSuccessResponse);
    let fixture = DeliveryFixture::new_scheduled_cancelled_and_renewed(
        217,
        4,
        &server,
        STEERING_TEXT,
    );
    let deadline = Instant::now() + TIMEOUT;
    let diagnostics = loop {
        let diagnostics = fixture.scheduler_diagnostics();
        if fixture.route_state() == AcceptedRouteEffectiveState::Delivered
            && diagnostics.workers_active() == 0
            && diagnostics.workers_joined() == 2
            && diagnostics.retry_state()
                == crate::cas_projection::ActiveSteeringRetryState::Ineligible
        {
            break diagnostics;
        }
        assert!(
            Instant::now() < deadline,
            "pre-completion lifecycle reopen was lost: {diagnostics:?}",
        );
        thread::yield_now();
    };

    assert_eq!(diagnostics.workers_started(), 2);
    assert_eq!(
        diagnostics.retry_state(),
        crate::cas_projection::ActiveSteeringRetryState::Ineligible
    );

    fixture.close(server);
}

#[test]
fn full_worker_set_preserves_ready_input_for_delivery_after_release() {
    let server = SteeringServer::spawn(SteeringServerScenario::LifecycleBeforeSuccessResponse);
    let fixture = DeliveryFixture::new(201, 4, &server, STEERING_TEXT);
    let ready_before = fixture.ready_input();
    let gate_before = fixture.input_gate();

    assert!(matches!(
        fixture.deliver_with_workers_occupied(),
        Ok(ActiveSteeringDeliveryOutcome::Saturated {
            cause: ActiveSteeringSaturationCause::WorkerPoolFull,
        })
    ));
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::Ready
    );
    assert_eq!(fixture.route_lifecycle(), AcceptedInputLifecycle::Admitted);
    assert_eq!(fixture.ready_input(), ready_before);
    assert_eq!(fixture.input_gate(), gate_before);
    let diagnostics = fixture.worker_diagnostics();
    assert_eq!(diagnostics.available(), 2);
    assert_eq!(diagnostics.denied_singles(), 1);

    assert!(matches!(
        fixture.deliver(),
        Ok(ActiveSteeringDeliveryOutcome::Delivered)
    ));
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::Delivered
    );

    fixture.close(server);
}

#[test]
fn occupied_connection_attempt_preserves_ready_input_for_later_delivery() {
    let server = SteeringServer::spawn(SteeringServerScenario::LifecycleBeforeSuccessResponse);
    let fixture = DeliveryFixture::new(214, 4, &server, STEERING_TEXT);
    let ready_before = fixture.ready_input();
    let gate_before = fixture.input_gate();

    assert!(matches!(
        fixture.deliver_with_connection_attempt_occupied(),
        Ok(ActiveSteeringDeliveryOutcome::Saturated {
            cause: ActiveSteeringSaturationCause::ConnectionAttemptBusy,
        })
    ));
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::Ready
    );
    assert_eq!(fixture.route_lifecycle(), AcceptedInputLifecycle::Admitted);
    assert_eq!(fixture.ready_input(), ready_before);
    assert_eq!(fixture.input_gate(), gate_before);

    assert!(matches!(
        fixture.deliver(),
        Ok(ActiveSteeringDeliveryOutcome::Delivered)
    ));
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::Delivered
    );

    fixture.close(server);
}

#[test]
fn closed_structured_rejection_preserves_the_exact_diagnostic_and_queues_next_turn() {
    let server = SteeringServer::spawn(SteeringServerScenario::StructuredRejection);
    let fixture = DeliveryFixture::new(202, 4, &server, STEERING_TEXT);

    let outcome = fixture.deliver().unwrap();
    let ActiveSteeringDeliveryOutcome::SteeringRejected { rejection } = outcome else {
        panic!("closed structured rejection was misclassified: {outcome:?}");
    };
    assert_eq!(rejection.code(), -32_600);
    assert_eq!(rejection.message(), "outer steering diagnostic");
    assert!(rejection.data_was_present());
    assert_eq!(
        rejection.verdict(),
        Some(JsonRpcErrorVerdict::ActiveTurnNotSteerable {
            turn_kind: JsonRpcTurnKind::Review,
        })
    );
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::SteeringRejected)
    );
    assert!(matches!(
        fixture.binding_state(),
        BindingState::Active(_)
    ));

    fixture.close(server);
}

#[test]
fn exact_rejection_without_closed_verdict_retires_target_but_preserves_named_input() {
    let server = SteeringServer::spawn(SteeringServerScenario::UnconfirmedRejection);
    let fixture = DeliveryFixture::new(203, 4, &server, STEERING_TEXT);

    let outcome = fixture.deliver().unwrap();
    let ActiveSteeringDeliveryOutcome::ProjectionLost {
        cause: ActiveSteeringProjectionLossCause::UnconfirmedRejection(rejection),
    } = outcome
    else {
        panic!("unconfirmed exact rejection was misclassified: {outcome:?}");
    };
    assert_eq!(rejection.code(), -32_600);
    assert_eq!(rejection.message(), "active turn mismatch");
    assert_eq!(rejection.verdict(), None);
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert!(matches!(fixture.binding_state(), BindingState::Stale(_)));

    fixture.close(server);
}

#[test]
fn completion_unknown_terminalizes_the_input_and_retires_projection_authority() {
    let server = SteeringServer::spawn(SteeringServerScenario::CompletionUnknown);
    let fixture = DeliveryFixture::new(204, 4, &server, STEERING_TEXT);

    let outcome = fixture.deliver().unwrap();
    let ActiveSteeringDeliveryOutcome::DeliveryUnknown {
        cause: ActiveSteeringUnknownCause::Backend(error),
    } = outcome
    else {
        panic!("post-dispatch response mismatch was misclassified: {outcome:?}");
    };
    assert!(matches!(
        *error,
        ManagedBackendError::TurnResponseIdentityMismatch {
            ref method,
            ref expected,
            ref actual,
        } if method == "turn/steer"
            && expected.as_str() == server::CAS_TURN_ID
            && actual.as_str() == "phase54-wrong-turn"
    ));
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::DeliveryUnknown
    );
    assert!(matches!(fixture.binding_state(), BindingState::Stale(_)));

    fixture.close(server);
}

#[test]
fn exact_response_waits_for_started_then_completed_before_durable_success() {
    let server =
        SteeringServer::spawn(SteeringServerScenario::SuccessResponseBeforeLifecycle);
    let mut fixture = DeliveryFixture::new(205, 4, &server, STEERING_TEXT);

    let (state_before_lifecycle, outcome) =
        fixture.deliver_after_delayed_lifecycle(&server);
    assert_eq!(
        state_before_lifecycle,
        AcceptedRouteEffectiveState::Delivering,
        "the exact response cannot complete delivery before delayed lifecycle"
    );
    let outcome = outcome.unwrap();
    assert!(
        matches!(outcome, ActiveSteeringDeliveryOutcome::Delivered),
        "exact success settled unexpectedly: {outcome:?}",
    );
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::Delivered
    );
    assert!(matches!(fixture.binding_state(), BindingState::Active(_)));

    fixture.close(server);
}

#[test]
fn lifecycle_before_exact_response_still_completes_delivery() {
    let server = SteeringServer::spawn(SteeringServerScenario::LifecycleBeforeSuccessResponse);
    let fixture = DeliveryFixture::new(213, 4, &server, STEERING_TEXT);

    let outcome = fixture.deliver().unwrap();
    assert!(
        matches!(outcome, ActiveSteeringDeliveryOutcome::Delivered),
        "lifecycle-before-response success settled unexpectedly: {outcome:?}",
    );
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::Delivered
    );
    assert!(matches!(fixture.binding_state(), BindingState::Active(_)));
    #[cfg(feature = "test-faults")]
    assert_eq!(
        fixture.free_space_observation_count(),
        0,
        "active steering must not probe the new-turn free-space reserve"
    );

    fixture.close(server);
}

#[test]
fn repeated_image_labels_dispatch_one_image_and_replay_both_checked_lifecycles() {
    let server = SteeringServer::spawn(SteeringServerScenario::RepeatedImageSuccess);
    let fixture = DeliveryFixture::new_repeated_image(207, 4, &server);

    let outcome = fixture.deliver().unwrap();
    assert!(
        matches!(outcome, ActiveSteeringDeliveryOutcome::Delivered),
        "image-bearing success settled unexpectedly: {outcome:?}",
    );
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::Delivered
    );

    fixture.close(server);
}

#[test]
fn cancellation_during_preparation_returns_the_exact_route_to_retryable() {
    let server = SteeringServer::spawn(SteeringServerScenario::NoSteeringRequest);
    let fixture = DeliveryFixture::new(208, 4, &server, STEERING_TEXT);
    fixture.cancel();

    let outcome = fixture.deliver().unwrap();
    assert!(matches!(
        outcome,
        ActiveSteeringDeliveryOutcome::Retryable {
            cause: ActiveSteeringRetryCause::Preparation(_),
        }
    ));
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::Ready
    );
    assert_eq!(
        fixture.route_lifecycle(),
        AcceptedInputLifecycle::Retryable
    );
    assert!(matches!(fixture.binding_state(), BindingState::Active(_)));

    let ready_before_saturation = fixture.ready_input();
    let gate_before_saturation = fixture.input_gate();
    assert!(matches!(
        fixture.deliver_with_workers_occupied(),
        Ok(ActiveSteeringDeliveryOutcome::Saturated {
            cause: ActiveSteeringSaturationCause::WorkerPoolFull,
        })
    ));
    assert_eq!(fixture.ready_input(), ready_before_saturation);
    assert_eq!(fixture.input_gate(), gate_before_saturation);

    fixture.close(server);
}

#[test]
fn target_loss_after_exact_response_without_lifecycle_terminalizes_delivery_unknown() {
    let server = SteeringServer::spawn(SteeringServerScenario::SuccessThenTargetLoss);
    let mut fixture = DeliveryFixture::new(209, 4, &server, STEERING_TEXT);

    let (state_before_loss, outcome) = fixture.deliver_after_delayed_lifecycle(&server);
    assert_eq!(state_before_loss, AcceptedRouteEffectiveState::Delivering);
    assert!(matches!(
        outcome.unwrap(),
        ActiveSteeringDeliveryOutcome::DeliveryUnknown {
            cause: ActiveSteeringUnknownCause::Lifecycle(_),
        }
    ));
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::DeliveryUnknown
    );
    assert!(matches!(fixture.binding_state(), BindingState::Stale(_)));

    fixture.close(server);
}

#[test]
fn lifecycle_arm_retry_precedes_concurrent_ordinary_loss() {
    let server = SteeringServer::spawn(SteeringServerScenario::NoSteeringRequest);
    let mut fixture = DeliveryFixture::new(211, 4, &server, STEERING_TEXT);

    let outcome = fixture
        .deliver_retry_during_ordinary_loss(RetryRaceBranch::LifecycleArm)
        .unwrap();
    assert!(matches!(
        outcome,
        ActiveSteeringDeliveryOutcome::ProjectionLost {
            cause: ActiveSteeringProjectionLossCause::LifecycleArm(_),
        }
    ));
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert_eq!(
        fixture.route_lifecycle(),
        AcceptedInputLifecycle::Retryable
    );
    assert!(matches!(fixture.binding_state(), BindingState::Stale(_)));

    fixture.close(server);
}

#[test]
fn authorization_retry_precedes_concurrent_ordinary_loss() {
    let server = SteeringServer::spawn(SteeringServerScenario::NoSteeringRequest);
    let mut fixture = DeliveryFixture::new(212, 4, &server, STEERING_TEXT);

    let outcome = fixture
        .deliver_retry_during_ordinary_loss(RetryRaceBranch::CommandAuthorization)
        .unwrap();
    assert!(matches!(
        outcome,
        ActiveSteeringDeliveryOutcome::ProjectionLost {
            cause: ActiveSteeringProjectionLossCause::TargetAuthorization(_),
        }
    ));
    assert_eq!(
        fixture.route_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert_eq!(
        fixture.route_lifecycle(),
        AcceptedInputLifecycle::Retryable
    );
    assert!(matches!(fixture.binding_state(), BindingState::Stale(_)));

    fixture.close(server);
}

#[cfg(feature = "test-faults")]
#[test]
fn after_persist_claim_fault_fails_closed_without_steering_dispatch() {
    let server = SteeringServer::spawn(SteeringServerScenario::NoSteeringRequest);
    let fixture = DeliveryFixture::new(210, 4, &server, STEERING_TEXT);
    fixture.fail_next_claim_after_persist();

    let outcome = fixture.deliver();
    assert!(matches!(
        outcome,
        Err(ActiveSteeringDeliveryError::PersistentFailureCut)
    ));
    // `NoSteeringRequest` makes `fixture.close` prove the persistent-failure cut
    // did not dispatch `turn/steer`.

    fixture.close(server);
}

#[cfg(feature = "test-faults")]
#[test]
fn repeated_proven_zero_byte_failure_retries_before_connection_loss_reclassifies_projection() {
    for seed in 206..214 {
        let server = SteeringServer::spawn(SteeringServerScenario::NoSteeringRequest);
        let fixture = DeliveryFixture::new(seed, 4, &server, STEERING_TEXT);
        fixture.fail_next_write_before_dispatch();

        let outcome = fixture.deliver().unwrap();
        let ActiveSteeringDeliveryOutcome::ProjectionLost {
            cause: ActiveSteeringProjectionLossCause::TargetClosed,
        } = outcome
        else {
            panic!("iteration {seed}: proven pre-dispatch failure was misclassified: {outcome:?}");
        };
        assert_eq!(
            fixture.route_state(),
            AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost),
            "iteration {seed}"
        );
        assert_eq!(
            fixture.route_lifecycle(),
            AcceptedInputLifecycle::Retryable,
            "iteration {seed}: proven non-dispatch must publish retry before target-loss convergence"
        );
        assert!(
            matches!(fixture.binding_state(), BindingState::Stale(_)),
            "iteration {seed}"
        );

        fixture.close(server);
    }
}

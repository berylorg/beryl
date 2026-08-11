use beryl_home_store::{CommandOutcome, CursorReadLimits};
use syndic_storage::{
    AbandonStopOperation, BindingState, DeliveryRecoveryCase, InputGateState, JoinStopCause,
    SafelyReopenStopOperation, StopCause, StopOperationState, StopOperationTransitionStatus,
};

use super::stop_support::{active_stop_fixture, admit_current_draft_as_accepted, point_limit};

fn recovery_source(
    fixture: &super::stop_support::ActiveStopFixture,
) -> syndic_storage::DeliveryRecoverySource {
    let page = fixture
        .storage
        .delivery_recovery_startup_page(
            &fixture.store,
            None,
            CursorReadLimits::new(32, 1_000_000).unwrap(),
        )
        .unwrap();
    page.records()
        .iter()
        .find(|source| source.thread_id() == fixture.thread)
        .unwrap()
        .clone()
}

pub(super) fn startup_abandonment(
    fixture: &super::stop_support::ActiveStopFixture,
) -> (syndic_storage::DeliveryRecoverySource, AbandonStopOperation) {
    let source = recovery_source(fixture);
    let DeliveryRecoveryCase::Stopping(recovered) = fixture
        .storage
        .classify_delivery_recovery(&fixture.store, &source, point_limit())
        .unwrap()
    else {
        panic!("fixture did not classify as a live stop");
    };
    let stale = recovered
        .startup_stale_binding(
            "foreground process generation was lost",
            recovered.minimum_timestamp(),
        )
        .unwrap();
    let request = AbandonStopOperation::new(
        recovered.operation_id(),
        recovered.target().clone(),
        recovered.current_gate_revision(),
        recovered.stop_revision(),
        recovered.current_state_revision(),
        recovered.startup_abandonment_reason(),
        stale,
    );
    (source, request)
}

#[test]
fn classified_abandonment_converges_without_losing_queued_input() {
    let fixture = active_stop_fixture("phase65-stop-abandonment");
    fixture.admit_stop();
    admit_current_draft_as_accepted(&fixture, "queued during stop", 150, 5);
    let (old_source, request) = startup_abandonment(&fixture);
    assert_eq!(
        fixture
            .storage
            .stop_abandonment_status(&fixture.store, &request, point_limit(),)
            .unwrap(),
        StopOperationTransitionStatus::Prior
    );

    match fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_abandon_stop_operation(request.clone()),
        ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean classified stop abandonment, got {outcome:?}"),
    }

    assert_eq!(
        fixture
            .storage
            .stop_abandonment_status(&fixture.store, &request, point_limit(),)
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert!(matches!(
        fixture.stop().state(),
        StopOperationState::Abandoned(_)
    ));
    let gate = fixture.gate();
    assert_eq!(
        gate.state(),
        &InputGateState::FinalizingHistory(fixture.turn)
    );
    assert_eq!(gate.live_next_turn_count(), 1);
    assert_eq!(gate.live_steering_count(), 0);
    assert!(matches!(
        fixture
            .storage
            .current_binding(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Stale(stale) if stale == request.stale()
    ));
    fixture.store.validate_registered_domains().unwrap();

    assert!(matches!(
        fixture
            .storage
            .classify_delivery_recovery(&fixture.store, &old_source, point_limit(),),
        Err(syndic_storage::DeliveryRecoveryClassificationError::SourceDrift)
    ));
    let new_source = recovery_source(&fixture);
    assert!(matches!(
        fixture.storage.classify_delivery_recovery(
            &fixture.store,
            &new_source,
            point_limit(),
        ),
        Ok(DeliveryRecoveryCase::FinalizingHistory {
            thread_id,
            turn_id,
            ..
        }) if thread_id == fixture.thread && turn_id == fixture.turn
    ));
}

#[test]
fn cause_join_racing_abandonment_requires_the_exact_new_revision() {
    let fixture = active_stop_fixture("phase65-stop-abandonment-race");
    fixture.admit_stop();
    let (_, stale_abandonment) = startup_abandonment(&fixture);
    let join = JoinStopCause::new(
        fixture.operation_id,
        fixture.target.clone(),
        fixture.gate().revision(),
        fixture.stop().revision(),
        StopCause::DiagnosticControl,
    );
    match fixture
        .store
        .execute_current(fixture.storage.current_join_stop_cause(join.clone()))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean abandonment-race stop join, got {outcome:?}"),
    }
    assert_eq!(
        fixture
            .storage
            .stop_abandonment_status(&fixture.store, &stale_abandonment, point_limit(),)
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );

    let (_, current_abandonment) = startup_abandonment(&fixture);
    let stale_reopen = SafelyReopenStopOperation::new(
        fixture.operation_id,
        fixture.target.clone(),
        current_abandonment.expected_gate_revision(),
        current_abandonment.expected_stop_revision(),
    );
    match fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_abandon_stop_operation(current_abandonment),
        ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean current stop abandonment, got {outcome:?}"),
    }
    assert_eq!(
        fixture
            .storage
            .stop_cause_join_status(&fixture.store, &join, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(
        fixture
            .storage
            .safe_stop_reopen_status(&fixture.store, &stale_reopen, point_limit(),)
            .unwrap(),
        StopOperationTransitionStatus::Collision
    );
    fixture.store.validate_registered_domains().unwrap();
}

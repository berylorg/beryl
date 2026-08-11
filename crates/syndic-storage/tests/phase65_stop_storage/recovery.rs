use beryl_home_store::{CommandOutcome, CursorReadLimits};
use syndic_storage::{
    ClaimStopDispatch, DeliveryRecoveryCase, StopAttemptNonce, StopCause, StopOperationRevision,
    StopOperationState, StopOperationTransitionStatus,
};

use super::stop_support::{
    active_stop_fixture, admit_current_draft_as_accepted, pending_stop_fixture, point_limit,
};

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
    assert_eq!(page.records().len(), 1);
    page.records()[0].clone()
}

#[test]
fn restart_classifies_admitted_stop_without_replay_authority() {
    let fixture = active_stop_fixture("phase65-stop-recovery-admitted");
    fixture.admit_stop();
    let source = recovery_source(&fixture);
    let DeliveryRecoveryCase::Stopping(recovered) = fixture
        .storage
        .classify_delivery_recovery(&fixture.store, &source, point_limit())
        .unwrap()
    else {
        panic!("stopping gate did not classify as stop recovery");
    };
    assert_eq!(recovered.operation_id(), fixture.operation_id);
    assert_eq!(recovered.target(), &fixture.target);
    assert_eq!(recovered.state(), StopOperationState::Admitted);
    assert_eq!(recovered.attempt(), None);
    assert_eq!(
        recovered.startup_abandonment_reason(),
        syndic_storage::StopAbandonmentReason::StartupProcessGenerationLost
    );
    assert_eq!(
        recovered
            .startup_stale_binding(
                "foreground process generation was lost",
                recovered.minimum_timestamp(),
            )
            .unwrap()
            .loaded_generation(),
        Some(fixture.target.loaded_generation())
    );
}

#[test]
fn admitted_stop_authority_survives_a_physical_close_and_reopen() {
    let fixture = active_stop_fixture("phase66-stop-reopen-admitted");
    fixture.admit_stop();
    let fixture = fixture.reopen();
    assert_eq!(
        fixture
            .stop()
            .cause_first_revisions()
            .first_revision(StopCause::SelectedOperationControl),
        Some(StopOperationRevision::FIRST)
    );
    assert_eq!(
        fixture
            .storage
            .stop_admission_status(&fixture.store, &fixture.admission, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert!(matches!(
        fixture
            .storage
            .classify_delivery_recovery(&fixture.store, &recovery_source(&fixture), point_limit(),)
            .unwrap(),
        DeliveryRecoveryCase::Stopping(_)
    ));
}

#[test]
fn restart_retains_claimed_attempt_but_never_mints_another() {
    let fixture = active_stop_fixture("phase65-stop-recovery-claimed");
    fixture.admit_stop();
    let attempt = StopAttemptNonce::from_bytes([130; 16]);
    let claim = ClaimStopDispatch::new(
        fixture.operation_id,
        fixture.target.clone(),
        fixture.gate().revision(),
        fixture.stop().revision(),
        attempt,
    );
    match fixture
        .store
        .execute_current(fixture.storage.current_claim_stop_dispatch(claim))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean recovery stop claim, got {outcome:?}"),
    }
    let source = recovery_source(&fixture);

    for _ in 0..2 {
        let DeliveryRecoveryCase::Stopping(recovered) = fixture
            .storage
            .classify_delivery_recovery(&fixture.store, &source, point_limit())
            .unwrap()
        else {
            panic!("claimed stop did not remain recovery work");
        };
        assert_eq!(recovered.state(), StopOperationState::DispatchClaimed);
        assert_eq!(recovered.attempt(), Some(attempt));
    }
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn claimed_stop_reopens_then_converges_through_startup_abandonment_with_queued_work() {
    let fixture = active_stop_fixture("phase66-stop-reopen-claimed-abandon");
    fixture.admit_stop();
    let attempt = StopAttemptNonce::from_bytes([131; 16]);
    let claim = ClaimStopDispatch::new(
        fixture.operation_id,
        fixture.target.clone(),
        fixture.gate().revision(),
        fixture.stop().revision(),
        attempt,
    );
    match fixture
        .store
        .execute_current(fixture.storage.current_claim_stop_dispatch(claim.clone()))
    {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean claimed-stop setup, got {outcome:?}"),
    }
    admit_current_draft_as_accepted(&fixture, "queued before claimed recovery", 151, 5);
    let fixture = fixture.reopen();
    assert_eq!(
        fixture.stop().dispatch_claim().unwrap().source_revision(),
        StopOperationRevision::FIRST
    );
    assert_eq!(fixture.stop().dispatch_claim().unwrap().attempt(), attempt);
    assert_eq!(
        fixture
            .storage
            .stop_dispatch_claim_status(&fixture.store, &claim, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    let (_, abandonment) = super::abandonment::startup_abandonment(&fixture);
    match fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_abandon_stop_operation(abandonment.clone()),
        ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean claimed-stop abandonment, got {outcome:?}"),
    }
    let fixture = fixture.reopen();
    assert_eq!(
        fixture.stop().dispatch_claim().unwrap().source_revision(),
        StopOperationRevision::FIRST
    );
    assert_eq!(fixture.stop().dispatch_claim().unwrap().attempt(), attempt);
    assert!(matches!(
        fixture.stop().state(),
        StopOperationState::Abandoned(_)
    ));
    assert_eq!(
        fixture
            .storage
            .stop_abandonment_status(&fixture.store, &abandonment, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    assert_eq!(fixture.gate().live_next_turn_count(), 1);
    assert_eq!(fixture.gate().live_steering_count(), 0);
    fixture.store.validate_registered_domains().unwrap();
}

#[test]
fn pending_stop_without_activation_reopens_and_converges_through_startup_abandonment() {
    let fixture = pending_stop_fixture("phase66-stop-pending-recovery");
    fixture.admit_stop();
    let fixture = fixture.reopen();
    let (_, abandonment) = super::abandonment::startup_abandonment(&fixture);
    match fixture
        .store
        .execute_current(
            fixture
                .storage
                .current_abandon_stop_operation(abandonment.clone()),
        ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected clean pending-stop abandonment, got {outcome:?}"),
    }
    assert_eq!(
        fixture
            .storage
            .stop_abandonment_status(&fixture.store, &abandonment, point_limit())
            .unwrap(),
        StopOperationTransitionStatus::Exact
    );
    fixture.store.validate_registered_domains().unwrap();
}

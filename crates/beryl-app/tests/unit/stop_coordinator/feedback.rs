use super::*;
use crate::cas_projection::{ExactStopAttemptKind, ExactStopFeedbackState, ExactStopRequestError};

fn reserve(fixture: &StopFixture) -> crate::cas_projection::ExactStopFeedback {
    fixture
        .coordinator
        .reserve_feedback(
            &fixture.target,
            &fixture.proof,
            fixture.coordinator.feedback_eligibility_epoch().unwrap(),
        )
        .unwrap()
        .0
}

#[test]
fn exact_stop_feedback_bounds_retained_results_and_reclaims_last_consumer() {
    let fixture = StopFixture::new(191);
    let revision = fixture.home.home_revision().unwrap();
    let mut retained = Vec::new();
    for _ in 0..72 {
        let record = reserve(&fixture);
        record
            .inner
            .update(ExactStopFeedbackState::RequestNotAdmitted, None);
        retained.push(record);
    }
    assert!(matches!(
        fixture.coordinator.reserve_feedback(
            &fixture.target,
            &fixture.proof,
            fixture.coordinator.feedback_eligibility_epoch().unwrap()
        ),
        Err(ExactStopRequestError::Capacity)
    ));
    assert_eq!(fixture.home.home_revision().unwrap(), revision);
    retained.pop();
    let fresh = reserve(&fixture);
    assert_eq!(fresh.snapshot().state, ExactStopFeedbackState::Waiting);
    fixture.coordinator.dispose_feedback();
    assert_eq!(
        fresh.snapshot().state,
        ExactStopFeedbackState::AuthorityLost
    );
    assert!(
        retained
            .iter()
            .all(|record| record.snapshot().state == ExactStopFeedbackState::RequestNotAdmitted)
    );
}

#[test]
fn exact_stop_feedback_duplicate_reservation_and_old_epoch_cannot_repeat() {
    let fixture = StopFixture::new(192);
    let epoch = fixture.coordinator.feedback_eligibility_epoch().unwrap();
    let first = fixture
        .coordinator
        .reserve_feedback(&fixture.target, &fixture.proof, epoch)
        .unwrap();
    assert!(first.1);
    let joined = fixture
        .coordinator
        .reserve_feedback(&fixture.target, &fixture.proof, epoch)
        .unwrap();
    assert!(!joined.1);
    assert_eq!(first.0, joined.0);
    first.0.inner.update(
        ExactStopFeedbackState::DurableNondispatch,
        Some(ExactStopAttemptKind::Durable),
    );
    assert!(matches!(
        fixture
            .coordinator
            .reserve_feedback(&fixture.target, &fixture.proof, epoch),
        Err(ExactStopRequestError::Revoked)
    ));
    let fresh = reserve(&fixture);
    assert_ne!(first.0, fresh);
    fixture.coordinator.feedback_for_target(
        &fixture.target,
        ExactStopFeedbackState::Interrupted,
        None,
    );
    assert_eq!(
        first.0.snapshot().state,
        ExactStopFeedbackState::DurableNondispatch
    );
    assert_eq!(fresh.snapshot().state, ExactStopFeedbackState::Interrupted);
}

#[test]
fn exact_stop_feedback_durable_nondispatch_keeps_resolved_presentation() {
    let fixture = StopFixture::new(193);
    let feedback = reserve(&fixture);
    let owner = match fixture
        .coordinator
        .coordinate(
            &fixture.router,
            fixture.proof.clone(),
            StopCause::SelectedOperationControl,
            &fixture.runtime_source,
        )
        .unwrap()
    {
        StopOwnership::Primary(owner) => owner,
        _ => panic!("primary stop owner required"),
    };
    assert_eq!(feedback.snapshot().attempt, ExactStopAttemptKind::Durable);
    owner.settle_before_dispatch().unwrap();
    assert_eq!(
        feedback.snapshot().state,
        ExactStopFeedbackState::DurableNondispatch
    );
    assert!(!fixture.coordinator.has_local_stop_for_test(fixture.thread));
    fixture.coordinator.dispose_feedback();
    assert_eq!(
        feedback.snapshot().state,
        ExactStopFeedbackState::DurableNondispatch
    );
}

#[test]
fn exact_stop_feedback_committed_admission_followup_failure_remains_waiting() {
    let faults = beryl_home_store::test_faults::FaultController::new();
    let fixture = StopFixture::with_faults(194, faults.clone());
    let feedback = reserve(&fixture);
    let pause = fixture
        .coordinator
        .install_race_pause(StopRaceStage::AdmissionPublishedBeforeRead);
    std::thread::scope(|scope| {
        let coordinate = scope.spawn(|| {
            fixture.coordinator.coordinate(
                &fixture.router,
                fixture.proof.clone(),
                StopCause::SelectedOperationControl,
                &fixture.runtime_source,
            )
        });
        assert!(pause.wait_until_reached(Duration::from_secs(10)));
        let live = fixture
            .storage
            .stop_admission_read(&fixture.home, fixture.thread, point_limit())
            .unwrap();
        let StopAdmissionRead::Stopping(live) = live else {
            panic!("admission committed before failure");
        };
        let join = JoinStopCause::new(
            live.operation_id(),
            live.target().clone(),
            live.current_gate_revision(),
            live.stop_revision(),
            StopCause::DiagnosticControl,
        );
        faults.panic_next(beryl_home_store::test_faults::FaultPoint::BeforeCommit);
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fixture
                .home
                .execute_current(fixture.storage.current_join_stop_cause(join))
        }));
        assert!(failed.is_err());
        pause.release();
        assert!(matches!(
            coordinate.join().unwrap(),
            Err(StopCoordinationError::HomeAuthorityLost) | Err(StopCoordinationError::Read(_))
        ));
    });
    assert_eq!(feedback.snapshot().attempt, ExactStopAttemptKind::Durable);
    assert_eq!(feedback.snapshot().state, ExactStopFeedbackState::Waiting);
    fixture.coordinator.dispose_feedback();
    assert_eq!(
        feedback.snapshot().state,
        ExactStopFeedbackState::AuthorityLost
    );
}

#[test]
fn exact_stop_feedback_join_existing_durable_operation_marks_admission() {
    let fixture = StopFixture::new(195);
    let owner = fixture
        .coordinator
        .coordinate(
            &fixture.router,
            fixture.proof.clone(),
            StopCause::SelectedOperationControl,
            &fixture.runtime_source,
        )
        .unwrap();
    let feedback = reserve(&fixture);
    assert!(matches!(
        fixture
            .coordinator
            .coordinate(
                &fixture.router,
                fixture.proof.clone(),
                StopCause::SelectedOperationControl,
                &fixture.runtime_source
            )
            .unwrap(),
        StopOwnership::Joined { .. }
    ));
    assert_eq!(feedback.snapshot().attempt, ExactStopAttemptKind::Durable);
    drop(feedback);
    assert!(fixture.coordinator.has_local_stop_for_test(fixture.thread));
    drop(owner);
}

#[test]
fn exact_stop_feedback_volatile_pre_writer_proof_preserves_nondispatch() {
    use crate::cas_projection::connection::{
        PersistentFailureDriverResult as R, PersistentFailureNoDispatchReason as N,
    };
    let fixture = StopFixture::new(197);
    let feedback = reserve(&fixture);
    let pause = fixture
        .coordinator
        .install_race_pause(StopRaceStage::ElectionHeldBeforeAdmissionGate);
    let identity = failure_identity(&fixture);
    std::thread::scope(|scope| {
        let coordinate = scope.spawn(|| {
            fixture.coordinator.coordinate(
                &fixture.router,
                fixture.proof.clone(),
                StopCause::SelectedOperationControl,
                &fixture.runtime_source,
            )
        });
        assert!(pause.wait_until_reached(Duration::from_secs(10)));
        assert!(
            fixture
                .command_gate
                .elect_persistent_failure_for_test(identity.failure_generation)
                .unwrap()
        );
        pause.release();
        assert!(matches!(
            coordinate.join().unwrap(),
            Err(StopCoordinationError::HomeAuthorityLost)
        ));
    });
    assert_eq!(feedback.snapshot().attempt, ExactStopAttemptKind::Volatile);
    assert_eq!(feedback.snapshot().state, ExactStopFeedbackState::Waiting);
    fixture
        .coordinator
        .freeze_for_persistent_failure(identity)
        .unwrap();
    let candidate = fixture
        .router
        .freeze_persistent_failure_targets(identity, true)
        .unwrap()
        .into_candidates()
        .pop()
        .unwrap();
    let (witness, proof) = candidate.into_parts();
    assert!(proof.is_ok());
    fixture
        .coordinator
        .feedback_volatile_result(&witness, R::NoDispatch(N::DriverUnavailable));
    assert_eq!(
        feedback.snapshot().state,
        ExactStopFeedbackState::VolatileNondispatch
    );
    fixture.coordinator.feedback_passive_terminal(
        fixture.proof.connection_generation(),
        fixture.target.cas_thread_id(),
        fixture.target.cas_turn_id(),
        beryl_backend::NormalTurnTerminalStatus::Interrupted,
    );
    fixture.coordinator.dispose_feedback();
    assert_eq!(
        feedback.snapshot().state,
        ExactStopFeedbackState::VolatileNondispatch
    );
}

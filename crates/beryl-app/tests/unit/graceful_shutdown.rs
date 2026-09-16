use super::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

#[path = "graceful_shutdown/backlog.rs"]
mod backlog;
#[path = "graceful_shutdown/cleanup.rs"]
mod cleanup;
#[path = "graceful_shutdown/reconciliation.rs"]
mod reconciliation;

fn poll(fixture: &Fixture, id: ShutdownAttemptId) -> ShutdownProgress {
    fixture
        .service
        .poll_graceful_shutdown(&fixture.sessions, id, &ProjectionCancellationToken::new())
        .unwrap()
}

fn finish_pass(fixture: &Fixture, id: ShutdownAttemptId) -> ShutdownProgress {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let progress = poll(fixture, id);
        if progress != ShutdownProgress::Waiting {
            return progress;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "shutdown did not settle"
        );
        std::thread::yield_now();
    }
}

fn finish_failed_reopen(
    fixture: &Fixture,
    id: ShutdownAttemptId,
    reason: ShutdownFailure,
) -> ShutdownProgress {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        match poll(fixture, id) {
            ShutdownProgress::Failed {
                reason: observed,
                reopened: true,
            } => {
                assert_eq!(observed, reason);
                return ShutdownProgress::Failed {
                    reason: observed,
                    reopened: true,
                };
            }
            ShutdownProgress::Failed {
                reason: observed,
                reopened: false,
            } => assert_eq!(observed, reason),
            progress => panic!("failed shutdown changed progress before reopening: {progress:?}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "failed shutdown did not reopen"
        );
        std::thread::yield_now();
    }
}

#[test]
fn pending_preservation_joins_one_attempt_and_keeps_admission_fenced() {
    let fixture = Fixture::new();
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    assert_eq!(fixture.service.begin_graceful_shutdown(&fence).unwrap(), id);
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
    assert!(matches!(
        fixture.gate.execution_permit().reserve(),
        Err(ProcessAdmissionError::Fenced)
    ));
    let command = fixture.service.live_home_command().unwrap();
    let pending = fixture
        .service
        .storage
        .pending_dispatch_evidence(command.home(), fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(pending.turn_id(), fixture.turn);
}

#[test]
fn retained_execution_cannot_settle_before_its_flight_returns() {
    let fixture = Fixture::new();
    let flight = fixture.acquired_projection_flight(fixture.thread);
    let publisher = flight.bind_terminal_completion(fixture.turn).unwrap();
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    assert_eq!(
        fixture
            .service
            .graceful_shutdown
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .execution
            .ordinary()
            .count(),
        1
    );
    drop(publisher);
    drop(flight);
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
    assert_eq!(
        fixture
            .service
            .graceful_shutdown
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .execution
            .ordinary()
            .count(),
        1
    );
}

#[test]
fn cancelled_attempt_reopens_only_after_admission_returns_and_new_attempt_has_new_identity() {
    let fixture = Fixture::new();
    let reservation = fixture.gate.execution_permit().reserve().unwrap();
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    assert_eq!(
        fixture
            .service
            .poll_graceful_shutdown(&fixture.sessions, id, &cancellation)
            .unwrap(),
        ShutdownProgress::Failed {
            reason: ShutdownFailure::Cancelled,
            reopened: false
        }
    );
    drop(reservation);
    assert_eq!(
        finish_failed_reopen(&fixture, id, ShutdownFailure::Cancelled),
        ShutdownProgress::Failed {
            reason: ShutdownFailure::Cancelled,
            reopened: true
        }
    );
    let fresh_fence = fixture.gate.fence().unwrap();
    let fresh = fixture
        .service
        .begin_graceful_shutdown(&fresh_fence)
        .unwrap();
    assert_ne!(fresh, id);
    assert!(matches!(
        fixture.service.begin_graceful_shutdown(&fence),
        Err(ShutdownCoordinatorError::StaleAttempt)
    ));
    assert_eq!(finish_pass(&fixture, fresh), ShutdownProgress::Ready);
}

#[test]
fn foreign_fence_cannot_create_a_shutdown_attempt() {
    let fixture = Fixture::new();
    let fence = ProcessAdmissionGate::new().fence().unwrap();
    assert!(fixture.service.begin_graceful_shutdown(&fence).is_err());
    assert!(
        fixture
            .service
            .graceful_shutdown
            .lock()
            .unwrap()
            .attempt
            .is_none()
    );
    assert!(fixture.gate.execution_permit().reserve().is_ok());
}

#[test]
fn foreign_sessions_cannot_cancel_or_progress_the_current_attempt() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    assert!(matches!(
        fixture
            .service
            .poll_graceful_shutdown(&foreign.sessions, id, &cancellation),
        Err(ShutdownCoordinatorError::StaleAttempt)
    ));
    assert!(
        fixture
            .service
            .graceful_shutdown
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .failure
            .is_none()
    );
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
}

#[test]
fn settlement_sweep_accepts_only_its_own_guard_revision_changes() {
    let fixture = Fixture::new();
    let fence = fixture.gate.fence().unwrap();
    fixture.service.begin_graceful_shutdown(&fence).unwrap();
    let owner = fixture.service.graceful_shutdown.lock().unwrap();
    let attempt = owner.attempt.as_ref().unwrap();
    let mut revision = fixture
        .service
        .shutdown_work_revision(&fixture.sessions)
        .unwrap();
    let expected = revision.after_settlement_guard().unwrap();
    assert!(
        attempt
            .settle_turn_with_confirmation(
                &fixture.service,
                &fixture.sessions,
                &mut revision,
                fixture.thread,
                fixture.turn,
                &ProjectionCancellationToken::new(),
                || {}
            )
            .is_ok()
    );
    assert_eq!(revision, expected);
    let before = revision.clone();
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let outcome = attempt.settle_turn_with_confirmation(
        &fixture.service,
        &fixture.sessions,
        &mut revision,
        fixture.thread,
        fixture.turn,
        &ProjectionCancellationToken::new(),
        || {
            drop(coordinator.begin_projection(fixture.thread).unwrap());
        },
    );
    assert!(matches!(outcome, Err(StepError::Retry)));
    assert_eq!(revision, before);
}

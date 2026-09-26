use super::*;

fn observe(fixture: &Fixture) -> ShutdownWorkObservation {
    fixture
        .service
        .observe_shutdown_work(&fixture.sessions, &ProjectionCancellationToken::new())
        .unwrap()
}

#[test]
fn observed_idle_and_pending_admission_install_a_pollable_exact_fence() {
    for pending in [false, true] {
        let fixture = Fixture::with_pending(pending);
        let observation = observe(&fixture);
        let permit = fixture.gate.execution_permit();
        let id = fixture
            .service
            .try_begin_observed_shutdown(&fixture.sessions, &observation)
            .unwrap();
        assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Fenced));
        let fence = fixture
            .service
            .graceful_shutdown
            .lock()
            .unwrap()
            .attempt
            .as_ref()
            .unwrap()
            .fence
            .clone();
        assert_eq!(fixture.service.begin_graceful_shutdown(&fence).unwrap(), id);
        assert!(matches!(
            fixture
                .service
                .try_begin_observed_shutdown(&fixture.sessions, &observation),
            Err(ShutdownCoordinatorError::StaleAttempt)
        ));
        assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
        let command = fixture.service.live_home_command().unwrap();
        let evidence = fixture
            .service
            .storage
            .pending_dispatch_evidence(command.home(), fixture.thread, point_limit())
            .unwrap();
        assert_eq!(evidence.is_some(), pending);
    }
}

#[test]
fn coordinator_refusal_preserves_execution_and_attempt_identity() {
    let fixture = Fixture::idle();
    let observation = observe(&fixture);
    let permit = fixture.gate.execution_permit();
    let mut coordinator = fixture.service.graceful_shutdown.lock().unwrap();
    assert!(matches!(
        fixture
            .service
            .try_begin_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownCoordinatorError::Unavailable)
    ));
    permit.commit(|| ()).unwrap();
    assert!(coordinator.attempt.is_none());
    assert_eq!(coordinator.serial, 0);
    coordinator.serial = u64::MAX;
    drop(coordinator);
    assert!(matches!(
        fixture
            .service
            .try_begin_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownCoordinatorError::Unavailable)
    ));
    permit.commit(|| ()).unwrap();
    assert!(
        fixture
            .service
            .graceful_shutdown
            .lock()
            .unwrap()
            .attempt
            .is_none()
    );
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = fixture.service.graceful_shutdown.lock().unwrap();
        panic!("poison shutdown coordinator");
    }));
    assert!(matches!(
        fixture
            .service
            .try_begin_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownCoordinatorError::Unavailable)
    ));
    permit.commit(|| ()).unwrap();
    let coordinator = fixture
        .service
        .graceful_shutdown
        .lock()
        .err()
        .expect("poisoned coordinator")
        .into_inner();
    assert!(coordinator.attempt.is_none());
    assert_eq!(coordinator.serial, u64::MAX);
}

#[test]
fn stale_and_foreign_observations_do_not_install_or_consume_an_attempt() {
    let fixture = Fixture::idle();
    let foreign = Fixture::idle();
    let observation = observe(&fixture);
    let other = observe(&foreign);
    let permit = fixture.gate.execution_permit();
    drop(fixture.service.connection_work_boundary().begin_change());
    for rejected in [&observation, &other] {
        assert!(matches!(
            fixture
                .service
                .try_begin_observed_shutdown(&fixture.sessions, rejected),
            Err(ShutdownCoordinatorError::Work(_))
        ));
        permit.commit(|| ()).unwrap();
        let coordinator = fixture.service.graceful_shutdown.lock().unwrap();
        assert!(coordinator.attempt.is_none());
        assert_eq!(coordinator.serial, 0);
    }
    let id = fixture
        .service
        .try_begin_observed_shutdown(&fixture.sessions, &observe(&fixture))
        .unwrap();
    assert_eq!(id.serial, 1);
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
}

#[test]
fn capture_preparation_failure_leaves_no_fence_or_attempt() {
    let mut fixture = Fixture::idle();
    let observation = observe(&fixture);
    let permit = fixture.gate.execution_permit();
    let compaction = fixture.service.context_compaction.take();
    assert!(matches!(
        fixture
            .service
            .try_begin_observed_shutdown(&fixture.sessions, &observation),
        Err(ShutdownCoordinatorError::Capture(_))
    ));
    permit.commit(|| ()).unwrap();
    let coordinator = fixture.service.graceful_shutdown.lock().unwrap();
    assert!(coordinator.attempt.is_none());
    assert_eq!(coordinator.serial, 0);
    drop(coordinator);
    fixture.service.context_compaction = compaction;
    let id = fixture
        .service
        .try_begin_observed_shutdown(&fixture.sessions, &observe(&fixture))
        .unwrap();
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
}

#[test]
fn observed_cancellation_reopens_and_accepts_only_a_new_attempt() {
    let fixture = Fixture::idle();
    let permit = fixture.gate.execution_permit();
    let id = fixture
        .service
        .try_begin_observed_shutdown(&fixture.sessions, &observe(&fixture))
        .unwrap();
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    let progress = fixture
        .service
        .poll_graceful_shutdown(&fixture.sessions, id, &cancellation)
        .unwrap();
    assert!(matches!(
        progress,
        ShutdownProgress::Failed {
            reason: ShutdownFailure::Cancelled,
            ..
        }
    ));
    finish_failed_reopen(&fixture, id, ShutdownFailure::Cancelled);
    assert_eq!(permit.commit(|| ()), Err(ProcessAdmissionError::Stale));
    fixture.gate.execution_permit().commit(|| ()).unwrap();
    let fresh = fixture
        .service
        .try_begin_observed_shutdown(&fixture.sessions, &observe(&fixture))
        .unwrap();
    assert_ne!(fresh, id);
    assert_eq!(finish_pass(&fixture, fresh), ShutdownProgress::Ready);
}

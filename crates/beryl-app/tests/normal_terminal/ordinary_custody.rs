use super::*;
use beryl_app::{
    cas_projection::{
        AdmittedProjectionSession, LoadedCasProjection, OrdinaryTurnExecutionError,
        OrdinaryTurnExecutionFailure, ProjectionCancellationToken,
        test_faults::{
            AcquisitionBarrierStage, TerminalHistoryBarrierStage, install_acquisition_barrier,
            install_terminal_history_barrier,
        },
    },
    process_admission::ProcessAdmissionError,
};
use syndic_storage::{PendingDispatchEvidence, SyndicPointReadLimit, TurnLifecycle};

fn prepare(
    fixture: &Fixture,
    server: &NormalTerminalServer,
    admission_event: bool,
) -> (AdmittedProjectionSession, LoadedCasProjection) {
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let mut session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution_binding().runtime_id(),
            CasProcessGeneration::new(47_139).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    if admission_event {
        server.wait_for_admission();
    }
    let coordinator = CasProjectionCoordinator::for_healthy_home(&fixture.home()).unwrap();
    let projection = coordinator
        .obtain_projection(
            &fixture.home(),
            &fixture.storage,
            &mut session,
            &beryl_app::cas_projection::CasProjectionRequest::new(
                fixture.thread,
                fixture.selected_path(fixture.thread),
                execution_binding(),
                ThreadStartOptions::persistent(),
                Some(2_000_000),
                SyndicTimestamp::from_unix_millis(37_000),
                TIMEOUT,
            ),
            &fixture.cancellation,
        )
        .unwrap();
    server.wait_for_projection();
    let deadline = std::time::Instant::now() + TIMEOUT;
    while fixture.store.worker_pool_diagnostics().active() != 2 {
        assert!(std::time::Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
    (session, projection)
}

fn execute(
    fixture: &Fixture,
    projection: LoadedCasProjection,
    cancellation: &ProjectionCancellationToken,
) -> Result<OrdinaryTurnExecutionOutcome, OrdinaryTurnExecutionFailure> {
    let mut lifecycle = NoopLifecycle::default();
    let mut branch = NoopBranch::default();
    CasProjectionCoordinator::for_healthy_home(&fixture.home())
        .unwrap()
        .execute_ordinary_turn(
            &fixture.home(),
            &fixture.storage,
            &fixture.state.assets(),
            None,
            projection,
            cancellation,
            &OrdinaryTurnExecutionRequest::new(TurnStartOptions::default(), TIMEOUT),
            OrdinaryDynamicToolHandlers::new(&mut lifecycle, &mut branch),
        )
}

fn pending(fixture: &Fixture) -> PendingDispatchEvidence {
    fixture
        .storage
        .pending_dispatch_evidence(
            &fixture.home(),
            fixture.thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap()
}

fn refused(
    fixture: &Fixture,
    projection: LoadedCasProjection,
    cancellation: &ProjectionCancellationToken,
    expected: impl FnOnce(&OrdinaryTurnExecutionError) -> bool,
) -> LoadedCasProjection {
    let generation = projection.loaded_session_generation();
    let binding = projection.binding_revision();
    let before = pending(fixture);
    let Err(OrdinaryTurnExecutionFailure::PreActivation { projection, source }) =
        execute(fixture, projection, cancellation)
    else {
        panic!("ordinary admission did not refuse before activation");
    };
    assert!(expected(&source), "unexpected refusal: {source:?}");
    assert_eq!(projection.loaded_session_generation(), generation);
    assert_eq!(projection.binding_revision(), binding);
    assert!(projection.is_live().unwrap());
    assert_eq!(pending(fixture), before);
    assert!(
        CasProjectionCoordinator::for_healthy_home(&fixture.home())
            .unwrap()
            .terminal_completion_for_test(fixture.thread)
            .unwrap()
            .is_none()
    );
    *projection
}

fn close(
    fixture: Fixture,
    session: AdmittedProjectionSession,
    projection: Option<LoadedCasProjection>,
    server: NormalTerminalServer,
) {
    session.invalidate_connection();
    assert_eq!(fixture.store.worker_pool_diagnostics().active(), 0);
    drop(projection);
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}

#[test]
fn direct_refusal_preserves_pending_projection_and_releases_capacity() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new_with_worker_capacity(140, 4);
    fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_projection_only();
    let (session, projection) = prepare(&fixture, &server, true);
    let release = fixture.store.reserve_scheduled_ordinary_worker_for_test();
    let projection = refused(&fixture, projection, &fixture.cancellation, |error| {
        matches!(
            error,
            OrdinaryTurnExecutionError::Coordinator(
                ProjectionCoordinatorError::OrdinaryWorkerCapacityFull { available: 1 }
            )
        )
    });
    release();
    let fence = fixture.process_admission.test_fence().unwrap();
    let projection = refused(&fixture, projection, &fixture.cancellation, |error| {
        matches!(
            error,
            OrdinaryTurnExecutionError::Coordinator(ProjectionCoordinatorError::AcquisitionFenced(
                ProcessAdmissionError::Fenced
            ))
        )
    });
    assert_eq!(fixture.store.worker_pool_diagnostics().active(), 2);
    fence
        .try_reopen(fixture.home().pending_reconciliations().is_empty())
        .unwrap();
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    let projection = refused(&fixture, projection, &cancellation, |error| {
        matches!(
            error,
            OrdinaryTurnExecutionError::ProjectionExecution(
                beryl_app::cas_projection::ProjectionExecutionError::Cancelled
            )
        )
    });
    assert_eq!(fixture.store.worker_pool_diagnostics().active(), 2);
    let release = fixture.store.reserve_scheduled_ordinary_worker_for_test();
    release();
    projection.release().unwrap();
    close(fixture, session, None, server);
}

#[test]
fn direct_admission_winner_retains_capacity_across_fence_before_flight() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new_with_worker_capacity(141, 4);
    fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn_projection_only();
    let (session, projection) = prepare(&fixture, &server, true);
    let before = pending(&fixture);
    let barrier = install_acquisition_barrier(
        fixture.store.service_generation(),
        AcquisitionBarrierStage::OrdinaryExecutionAdmitted,
    );
    let cancellation = ProjectionCancellationToken::new();
    let (projection, fence) = thread::scope(|scope| {
        let execution = scope.spawn(|| execute(&fixture, projection, &cancellation));
        barrier.wait();
        assert_eq!(fixture.store.worker_pool_diagnostics().active(), 3);
        let fence = fixture.process_admission.test_fence().unwrap();
        assert_eq!(
            fence.try_reopen(true),
            Err(ProcessAdmissionError::Unsettled)
        );
        assert_eq!(pending(&fixture), before);
        cancellation.cancel();
        barrier.release();
        let Err(OrdinaryTurnExecutionFailure::PreActivation { projection, source }) =
            execution.join().unwrap()
        else {
            panic!("cancelled admission winner activated the turn");
        };
        assert!(matches!(
            source,
            OrdinaryTurnExecutionError::ProjectionExecution(
                beryl_app::cas_projection::ProjectionExecutionError::Cancelled
            )
        ));
        (*projection, fence)
    });
    assert_eq!(pending(&fixture), before);
    assert_eq!(fixture.store.worker_pool_diagnostics().active(), 2);
    fence
        .try_reopen(fixture.home().pending_reconciliations().is_empty())
        .unwrap();
    projection.release().unwrap();
    close(fixture, session, None, server);
}

#[test]
fn direct_completion_handoff_retains_worker_and_counted_admission() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut fixture = Fixture::new_with_worker_capacity(142, 4);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = NormalTerminalServer::spawn();
    let (session, projection) = prepare(&fixture, &server, false);
    let barrier = install_terminal_history_barrier(
        fixture.thread,
        TerminalHistoryBarrierStage::AfterGateRelease,
    );
    let (projection, fence, observer) = thread::scope(|scope| {
        let execution = scope.spawn(|| execute(&fixture, projection, &fixture.cancellation));
        barrier.wait();
        assert_eq!(fixture.store.worker_pool_diagnostics().active(), 3);
        assert_eq!(fixture.store.worker_pool_diagnostics().available(), 1);
        let fence = fixture.process_admission.test_fence().unwrap();
        assert_eq!(
            fence.try_reopen(true),
            Err(ProcessAdmissionError::Unsettled)
        );
        let observer = CasProjectionCoordinator::for_healthy_home(&fixture.home())
            .unwrap()
            .terminal_completion_for_test(fixture.thread)
            .unwrap()
            .unwrap();
        assert_eq!(observer.turn_id(), submitted.turn);
        assert_eq!(observer.lifecycle(), Some(TurnLifecycle::Complete));
        barrier.release();
        let OrdinaryTurnExecutionOutcome::Terminal { projection, status } =
            execution.join().unwrap().unwrap()
        else {
            panic!("ordinary execution did not complete");
        };
        assert_durable_success(&fixture, submitted.turn, status);
        (*projection, fence, observer)
    });
    assert_eq!(fixture.store.worker_pool_diagnostics().active(), 2);
    let release = fixture.store.reserve_scheduled_ordinary_worker_for_test();
    release();
    fence
        .try_reopen(fixture.home().pending_reconciliations().is_empty())
        .unwrap();
    assert_eq!(observer.lifecycle(), Some(TurnLifecycle::Complete));
    assert!(projection.is_live().unwrap());
    close(fixture, session, Some(projection), server);
}

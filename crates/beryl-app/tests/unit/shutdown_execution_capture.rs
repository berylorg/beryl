use super::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

#[test]
fn accepted_execution_survives_stale_refresh_and_flight_release() {
    let fixture = Fixture::new();
    let flight = fixture.acquired_projection_flight(fixture.thread);
    let publisher = flight.bind_terminal_completion(fixture.turn).unwrap();
    let fence = fixture.gate.fence().unwrap();
    let mut capture = fixture
        .service
        .begin_shutdown_execution_capture(&fence)
        .unwrap();
    capture
        .refresh(&fixture.service, &ProjectionCancellationToken::new())
        .unwrap();
    let before = capture.ordinary.clone();
    assert_eq!(before.len(), 1);
    assert!(fixture.read(&fence).unwrap().is_none());
    let result = capture.refresh_with_confirmation(
        &fixture.service,
        &ProjectionCancellationToken::new(),
        || drop(flight),
    );
    assert!(matches!(
        result,
        Err(ShutdownExecutionCaptureError::Work(
            ProcessWorkError::StaleRevision
        ))
    ));
    assert_eq!(capture.ordinary, before);
    capture
        .refresh(&fixture.service, &ProjectionCancellationToken::new())
        .unwrap();
    assert_eq!(capture.ordinary, before);
    drop(publisher);
    assert_eq!(capture.ordinary().next().unwrap().1.turn_id(), fixture.turn);
    assert!(fixture.read(&fence).unwrap().is_some());
    drop(capture);
    fence.reopen_if(true).unwrap();
}

#[test]
fn late_winning_slot_is_captured_without_waiting_for_earlier_preparation() {
    let fixture = Fixture::new();
    let coordinator =
        CasProjectionCoordinator::for_healthy_home(fixture.service.home.as_deref().unwrap())
            .unwrap();
    let preparation = coordinator
        .begin_projection(SyndicThreadId::from_bytes([1; 16]))
        .unwrap();
    let flight = fixture.acquired_projection_flight(fixture.thread);
    let fence = fixture.gate.fence().unwrap();
    let mut capture = fixture
        .service
        .begin_shutdown_execution_capture(&fence)
        .unwrap();
    capture
        .refresh(&fixture.service, &ProjectionCancellationToken::new())
        .unwrap();
    assert!(capture.ordinary.is_empty());
    assert!(fixture.read(&fence).unwrap().is_none());
    let publisher = flight.bind_terminal_completion(fixture.turn).unwrap();
    capture
        .refresh(&fixture.service, &ProjectionCancellationToken::new())
        .unwrap();
    assert_eq!(capture.ordinary.len(), 1);
    assert_eq!(capture.ordinary().next().unwrap().0, fixture.thread);
    assert!(fixture.read(&fence).unwrap().is_none());
    drop(publisher);
    drop(flight);
    drop(preparation);
    assert!(fixture.read(&fence).unwrap().is_some());
    drop(capture);
    fence.reopen_if(true).unwrap();
}

#[test]
fn distinct_pre_fence_attempts_of_the_same_turn_retain_both_exact_observers() {
    let fixture = Fixture::new();
    let first = fixture.acquired_projection_flight(fixture.thread);
    let first_publisher = first.bind_terminal_completion(fixture.turn).unwrap();
    let next_acquisition = crate::cas_projection::acquisition::ProjectionAcquisition::admit(
        &fixture.service.live_command_authorizer(),
    )
    .unwrap();
    let fence = fixture.gate.fence().unwrap();
    let mut capture = fixture
        .service
        .begin_shutdown_execution_capture(&fence)
        .unwrap();
    capture
        .refresh(&fixture.service, &ProjectionCancellationToken::new())
        .unwrap();
    let first_observer = capture.ordinary[0].clone();
    drop(first_publisher);
    drop(first);
    let coordinator = CasProjectionCoordinator::for_healthy_home(
        fixture.service.live_home_command().unwrap().home(),
    )
    .unwrap();
    let next = coordinator
        .begin_projection(fixture.thread)
        .unwrap()
        .with_acquisition(next_acquisition);
    let next_publisher = next.bind_terminal_completion(fixture.turn).unwrap();
    capture
        .refresh(&fixture.service, &ProjectionCancellationToken::new())
        .unwrap();
    assert_eq!(capture.ordinary.len(), 2);
    assert_eq!(capture.ordinary[0], first_observer);
    assert_ne!(capture.ordinary[0], capture.ordinary[1]);
    assert_eq!(capture.ordinary[0].turn_id(), capture.ordinary[1].turn_id());
    capture
        .refresh(&fixture.service, &ProjectionCancellationToken::new())
        .unwrap();
    assert_eq!(capture.ordinary.len(), 2);
    assert_eq!(
        capture
            .ordinary_completion(&fixture.service, fixture.thread, fixture.turn)
            .unwrap(),
        None
    );
    drop(next_publisher);
    drop(next);
    assert!(fixture.read(&fence).unwrap().is_some());
    drop(capture);
    fence.reopen_if(true).unwrap();
}

#[test]
fn published_compaction_identity_without_durable_admission_remains_preparation() {
    use beryl_model::{
        BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration,
        CasProcessGeneration, CasThreadId, SyndicExecutionSnapshotId,
    };
    use syndic_storage::{
        CompactionAttemptNonce, CompactionOperationNonce, CompactionOperationTarget,
    };
    let fixture = Fixture::new();
    let source = &fixture.service.stop_coordinator.compaction_custody.source;
    let handle = source.begin(fixture.thread, None);
    let observation = handle.command_observation();
    let operation = CompactionOperationId::new(
        fixture.thread,
        CompactionOperationNonce::from_bytes([201; 16]),
    );
    handle.operation(
        operation,
        CompactionAttemptNonce::from_bytes([202; 16]),
        &CompactionOperationTarget::new(
            fixture.thread,
            operation.provider_turn_id(),
            SyndicExecutionSnapshotId::from_bytes([203; 16]),
            BindingRevision::new(1).unwrap(),
            RuntimeId::from_bytes([71; 16]),
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(1).unwrap(),
                CasLoadedThreadGeneration::new(1).unwrap(),
            ),
            CasThreadId::new("compaction-preparation").unwrap(),
        ),
    );
    let reservation = fixture.gate.execution_permit().reserve().unwrap();
    let fence = fixture.gate.fence().unwrap();
    let mut capture = fixture
        .service
        .begin_shutdown_execution_capture(&fence)
        .unwrap();
    capture
        .refresh(&fixture.service, &ProjectionCancellationToken::new())
        .unwrap();
    assert_eq!(capture.compactions().count(), 0);
    assert_eq!(capture.ordinary().count(), 0);
    let revision = fixture.service.compaction_work_revision().unwrap();
    let page = fixture
        .service
        .compaction_work_page(
            &revision,
            None,
            CompactionWorkPageLimits::new(1, CAPTURE_PAGE_BYTES).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 1);
    assert!(fixture.read(&fence).unwrap().is_none());
    drop(observation);
    drop(handle);
    drop(reservation);
    assert!(fixture.read(&fence).unwrap().is_some());
    drop(capture);
    fence.reopen_if(true).unwrap();
}

#[test]
fn foreign_reopened_cancelled_and_over_bound_refresh_preserve_accepted_state() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    let flight = fixture.acquired_projection_flight(fixture.thread);
    let publisher = flight.bind_terminal_completion(fixture.turn).unwrap();
    let fence = fixture.gate.fence().unwrap();
    let foreign_fence = foreign.gate.fence().unwrap();
    assert!(matches!(
        fixture
            .service
            .begin_shutdown_execution_capture(&foreign_fence),
        Err(ShutdownExecutionCaptureError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    let mut capture = fixture
        .service
        .begin_shutdown_execution_capture(&fence)
        .unwrap();
    capture.ordinary_limit = 0;
    assert!(matches!(
        capture.refresh(&fixture.service, &ProjectionCancellationToken::new()),
        Err(ShutdownExecutionCaptureError::Work(
            ProcessWorkError::SourceBoundExceeded
        ))
    ));
    assert!(capture.ordinary.is_empty());
    capture.ordinary_limit = usize::from(fixture.service.config.worker_capacity());
    capture
        .refresh(&fixture.service, &ProjectionCancellationToken::new())
        .unwrap();
    let before = capture.ordinary.clone();
    assert!(matches!(
        capture.refresh(&foreign.service, &ProjectionCancellationToken::new()),
        Err(ShutdownExecutionCaptureError::Work(
            ProcessWorkError::ForeignSources
        ))
    ));
    let cancellation = ProjectionCancellationToken::new();
    cancellation.cancel();
    assert!(matches!(
        capture.refresh(&fixture.service, &cancellation),
        Err(ShutdownExecutionCaptureError::Work(
            ProcessWorkError::Cancelled
        ))
    ));
    assert_eq!(capture.ordinary, before);
    drop(publisher);
    drop(flight);
    fence.reopen_if(true).unwrap();
    assert!(matches!(
        capture.refresh(&fixture.service, &ProjectionCancellationToken::new()),
        Err(ShutdownExecutionCaptureError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    assert_eq!(capture.ordinary, before);
    foreign_fence.reopen_if(true).unwrap();
}

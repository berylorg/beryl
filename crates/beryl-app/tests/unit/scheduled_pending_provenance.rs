use super::*;
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));

fn original(fixture: &Fixture) -> syndic_storage::RecoveredPendingSource {
    let live = fixture.service.live_home_command().unwrap();
    fixture
        .service
        .storage
        .recovered_pending_page(
            live.home(),
            fixture.service.storage.revision(live.home()).unwrap(),
            None,
            beryl_home_store::CursorReadLimits::new(
                8,
                syndic_storage::DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
            )
            .unwrap(),
            point_limit(),
        )
        .unwrap()
        .records()[0]
}

fn admission(
    service: &ProjectionConnectionService,
    thread: SyndicThreadId,
    source: syndic_storage::RecoveredPendingSource,
) -> Result<
    crate::cas_projection::ScheduledOrdinaryAdmission,
    crate::cas_projection::ScheduledOrdinaryAdmissionError,
> {
    let live = service.live_home_command().unwrap();
    let execution = service
        .storage
        .thread_execution(live.home(), source.thread_id(), point_limit())
        .unwrap()
        .unwrap();
    let acquisition = crate::cas_projection::acquisition::ProjectionAcquisition::admit(
        &service.live_command_authorizer(),
    )
    .unwrap();
    let flight = CasProjectionCoordinator::for_healthy_home(live.home())
        .unwrap()
        .begin_projection(thread)
        .unwrap()
        .with_acquisition(acquisition);
    crate::cas_projection::ScheduledOrdinaryAdmission::new(
        service.home_id,
        service.home_generation,
        service.service_generation(),
        thread,
        execution.execution().clone(),
        Some(source),
        service.try_acquire_scheduled_ordinary_worker().unwrap(),
        service
            .persistent_failure
            .as_ref()
            .unwrap()
            .terminal_disposer(service.home_id, service.home_generation),
        flight,
    )
}

#[test]
fn original_pending_admission_retains_full_provenance_and_counted_custody() {
    let fixture = Fixture::new();
    let source = original(&fixture);
    let admitted = admission(&fixture.service, fixture.thread, source).unwrap();
    assert_eq!(admitted.pending_source(), Some(source));
    assert_eq!(
        admitted
            .pending_source()
            .unwrap()
            .pending_dispatch_evidence()
            .turn_id(),
        fixture.turn
    );
    let fence = fixture.gate.fence().unwrap();
    assert_eq!(
        fence.reopen_if(true),
        Err(crate::process_admission::ProcessAdmissionError::Unsettled)
    );
    drop(admitted);
    fence.reopen_if(true).unwrap();
}

#[test]
fn pending_provenance_cannot_cross_home_or_thread_and_releases_failed_admission() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    let source = original(&fixture);
    assert!(matches!(
        admission(&foreign.service, foreign.thread, source),
        Err(crate::cas_projection::ScheduledOrdinaryAdmissionError::LeaseMismatch { .. })
    ));
    assert!(matches!(
        admission(
            &fixture.service,
            SyndicThreadId::from_bytes([79; 16]),
            source
        ),
        Err(crate::cas_projection::ScheduledOrdinaryAdmissionError::LeaseMismatch { .. })
    ));
    let fence = fixture.gate.fence().unwrap();
    fence.reopen_if(true).unwrap();
    let fence = foreign.gate.fence().unwrap();
    fence.reopen_if(true).unwrap();
}

#[test]
fn recovered_home_rejects_original_pending_provenance_from_the_retired_generation() {
    let fixture = Fixture::new();
    let source = original(&fixture);
    let Fixture {
        mut service,
        faults,
        _directory,
        ..
    } = fixture;
    let home = service.owned_home.take().unwrap();
    let storage = service.storage.clone();
    service.close().unwrap();
    faults.fail_next(beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation);
    assert!(
        storage
            .thread(&home, source.thread_id(), point_limit())
            .is_err()
    );
    let candidate = home.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    BerylState::reacquire_candidate(&candidate).unwrap();
    let home = candidate.publish().unwrap();
    let (provider, _) = ProcessScheduledExecutionProvider::new();
    let service = ProjectionConnectionService::new(
        ProcessAdmissionGate::new(),
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(provider),
    )
    .unwrap();
    assert_eq!(
        source.pending_dispatch_evidence().home_id(),
        service.home_id
    );
    assert_ne!(
        source.pending_dispatch_evidence().home_generation(),
        service.home_generation
    );
    assert!(matches!(
        admission(&service, source.thread_id(), source),
        Err(crate::cas_projection::ScheduledOrdinaryAdmissionError::LeaseMismatch { .. })
    ));
    service.close().unwrap();
}

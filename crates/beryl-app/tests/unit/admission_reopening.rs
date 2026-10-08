use crate::process_admission::{ProcessAdmissionError, ProcessAdmissionReopenError};
use beryl_home_store::HomeCoherenceError;

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_support.rs"
));
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_binding.rs"
));

fn assert_fenced(fixture: &Fixture) {
    assert!(matches!(
        fixture.gate.execution_permit().reserve(),
        Err(ProcessAdmissionError::Fenced)
    ));
}

#[test]
fn reopening_requires_returned_admission_and_never_revives_old_execution_permits() {
    let fixture = Fixture::idle();
    let old = fixture.gate.execution_permit();
    let reservation = old.reserve().unwrap();
    let fence = fixture.gate.fence().unwrap();
    assert_eq!(
        fixture.service.try_reopen_shutdown_admission(&fence),
        Err(ProcessAdmissionReopenError::Process(
            ProcessAdmissionError::Unsettled
        ))
    );
    assert_fenced(&fixture);
    drop(reservation);
    fixture
        .service
        .try_reopen_shutdown_admission(&fence)
        .unwrap();
    assert!(matches!(old.reserve(), Err(ProcessAdmissionError::Stale)));
    let fresh = fixture.gate.execution_permit().reserve().unwrap();
    drop(fresh);
    let next = fixture.gate.fence().unwrap();
    assert!(!fence.same_attempt(&next));
    assert_eq!(
        fixture.service.try_reopen_shutdown_admission(&fence),
        Err(ProcessAdmissionReopenError::Process(
            ProcessAdmissionError::Stale
        ))
    );
    assert_fenced(&fixture);
    fixture
        .service
        .try_reopen_shutdown_admission(&next)
        .unwrap();
}

#[test]
fn foreign_fence_and_closed_service_cannot_reopen_process_admission() {
    let fixture = Fixture::idle();
    let own = fixture.gate.fence().unwrap();
    let foreign = ProcessAdmissionGate::new().fence().unwrap();
    assert_eq!(
        fixture.service.try_reopen_shutdown_admission(&foreign),
        Err(ProcessAdmissionReopenError::Process(
            ProcessAdmissionError::Stale
        ))
    );
    assert_fenced(&fixture);
    fixture.service.command_gate.close_for_local_failure();
    assert_eq!(
        fixture.service.try_reopen_shutdown_admission(&own),
        Err(ProcessAdmissionReopenError::Service(
            LiveCommandAdmissionError::Closed
        ))
    );
    assert_fenced(&fixture);
}

#[test]
fn active_mutation_and_returned_indeterminate_custody_keep_reopening_fenced_until_resolution() {
    let fixture = Fixture::new();
    let cancellation = activate(&fixture);
    let fence = fixture.gate.fence().unwrap();
    let home = fixture.service.home.as_deref().unwrap();
    let storage = &fixture.service.storage;
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.cancel_binding_activation(storage.revision(home).unwrap(), cancellation))
        .unwrap();
    let pause = fixture
        .faults
        .block_next(FaultPoint::AfterCommitBeforePersist);
    let (active_result, outcome) = std::thread::scope(|scope| {
        let worker = scope.spawn(|| home.execute(command));
        let reached = pause.wait_until_reached(std::time::Duration::from_secs(10));
        let result = fixture.service.try_reopen_shutdown_admission(&fence);
        pause.release_with_error(std::io::ErrorKind::Other);
        let outcome = worker.join().unwrap();
        assert!(reached);
        (result, outcome)
    });
    assert_eq!(
        active_result,
        Err(ProcessAdmissionReopenError::Home(HomeCoherenceError::Busy))
    );
    assert_fenced(&fixture);
    let CommandOutcome::Indeterminate { reconciliation, .. } = outcome else {
        panic!("expected retained indeterminate custody");
    };
    assert!(home.pending_reconciliations().is_empty());
    assert_eq!(
        fixture.service.try_reopen_shutdown_admission(&fence),
        Err(ProcessAdmissionReopenError::Home(
            HomeCoherenceError::ReconciliationPending
        ))
    );
    assert_fenced(&fixture);
    let handle = reconciliation.install_and_handle();
    assert_eq!(
        fixture.service.try_reopen_shutdown_admission(&fence),
        Err(ProcessAdmissionReopenError::Home(
            HomeCoherenceError::ReconciliationPending
        ))
    );
    assert!(matches!(
        home.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    fixture
        .service
        .try_reopen_shutdown_admission(&fence)
        .unwrap();
    drop(fixture.gate.execution_permit().reserve().unwrap());
}

#[test]
fn home_failure_preserves_the_process_fence() {
    let fixture = Fixture::idle();
    let fence = fixture.gate.fence().unwrap();
    fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(
        fixture
            .service
            .home
            .as_deref()
            .unwrap()
            .home_revision()
            .is_err()
    );
    assert!(matches!(
        fixture.service.try_reopen_shutdown_admission(&fence),
        Err(ProcessAdmissionReopenError::Home(
            HomeCoherenceError::Unhealthy(_)
        )) | Err(ProcessAdmissionReopenError::Service(
            LiveCommandAdmissionError::Closed
        ))
    ));
    assert_fenced(&fixture);
}

#[test]
fn live_commands_and_home_election_share_master_then_process_lock_order() {
    let fixture = Fixture::idle();
    let fence = fixture.gate.fence().unwrap();
    let ready = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let authorizer = fixture.service.live_command_authorizer();
        let ready = &ready;
        let commands = scope.spawn(move || {
            ready.wait();
            for _ in 0..64 {
                let permit = authorizer.authorize().unwrap();
                permit.commit_if_current(|| ()).unwrap();
                let _ = permit.commit_execution_if_current(|| ());
            }
        });
        ready.wait();
        fixture
            .service
            .try_reopen_shutdown_admission(&fence)
            .unwrap();
        commands.join().unwrap();
    });
    drop(fixture.gate.execution_permit().reserve().unwrap());
}

#[test]
fn home_election_failure_retains_original_selection_exclusion_until_one_successful_reopening() {
    use crate::window_acquisition::RuntimeBackedWindowProcessRegistry;
    use std::{cell::Cell, sync::Arc};

    let directory = tempfile::tempdir().unwrap();
    let faults = beryl_home_store::test_faults::FaultController::new();
    let mut opening = beryl_home_store::HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    BerylState::register(&mut opening).unwrap();
    SyndicStorage::register(&mut opening).unwrap();
    let home = opening
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let original_generation = home.health().generation().unwrap();
    let gate = ProcessAdmissionGate::new();
    let registry = RuntimeBackedWindowProcessRegistry::new(gate.clone());
    let window = beryl_model::WindowId::from_bytes([207; 16]);
    let reservation = registry.reserve_main_window(window).unwrap();
    let lease = Arc::new(registry.admit_selection(&[window], window).unwrap());
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let retired = lease
        .retire_failed_home(&home, original_generation, window)
        .ok()
        .unwrap();
    let fence = gate.fence().unwrap();
    let mut candidate = home.recover_same_home().unwrap();
    retired
        .validate_candidate(&candidate.recovery_access().unwrap())
        .unwrap();
    BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let home = candidate.publish().unwrap();
    let (provider, _sessions) = ProcessScheduledExecutionProvider::new();
    let service = ProjectionConnectionService::new(
        gate.clone(),
        home,
        storage,
        ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(provider),
    )
    .unwrap();
    let home = service.home.as_deref().unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(
            service.storage.create_thread(
                service.storage.revision(home).unwrap(),
                CreateThread::ordinary(
                    SyndicThreadId::from_bytes([208; 16]),
                    SyndicDraftId::from_bytes([209; 16]),
                    ExecutionBinding::new(
                        RuntimeId::from_bytes([210; 16]),
                        RootId::from_bytes([211; 16]),
                        RuntimeNativePath::from_admitted(
                            RuntimeMode::host(),
                            PathFlavor::Windows,
                            r"C:\work\beryl",
                        )
                        .unwrap(),
                    ),
                    SyndicTimestamp::from_unix_millis(1),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ),
        )
        .unwrap();
    let prepared = Cell::new(0);
    let applied = Cell::new(0);
    let pause = faults.block_next(FaultPoint::AfterCommitBeforePersist);
    let refused = std::thread::scope(|scope| {
        let worker = scope.spawn(|| home.execute(command));
        assert!(pause.wait_until_reached(std::time::Duration::from_secs(10)));
        let result = service.try_reopen_shutdown_admission_with(
            &fence,
            || {
                let guard = retired.prepare_coherent_release()?;
                prepared.set(prepared.get() + 1);
                Ok::<_, String>(guard)
            },
            |guard| {
                guard.apply();
                applied.set(applied.get() + 1);
            },
        );
        pause.release();
        assert!(matches!(
            worker.join().unwrap(),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        result
    });
    assert_eq!(
        refused,
        Err(ProcessAdmissionReopenError::Home(HomeCoherenceError::Busy))
    );
    assert_eq!((prepared.get(), applied.get()), (1, 0));
    retired.validate_owner().unwrap();
    assert!(registry.selection_pending());
    assert!(registry.test_close_is_blocked(&[window]));
    assert!(registry.test_acquisition_is_blocked(beryl_model::WindowId::from_bytes([212; 16])));
    assert!(matches!(
        gate.execution_permit().reserve(),
        Err(ProcessAdmissionError::Fenced)
    ));
    service
        .try_reopen_shutdown_admission_with(
            &fence,
            || {
                let guard = retired.prepare_coherent_release()?;
                prepared.set(prepared.get() + 1);
                Ok::<_, String>(guard)
            },
            |guard| {
                guard.apply();
                applied.set(applied.get() + 1);
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!((prepared.get(), applied.get()), (2, 1));
    assert!(!registry.selection_pending());
    drop(gate.execution_permit().reserve().unwrap());
    assert!(matches!(
        service.try_reopen_shutdown_admission_with(
            &fence,
            || {
                let guard = retired.prepare_coherent_release()?;
                prepared.set(prepared.get() + 1);
                Ok::<_, String>(guard)
            },
            |guard| {
                guard.apply();
                applied.set(applied.get() + 1);
            }
        ),
        Err(ProcessAdmissionReopenError::Process(
            ProcessAdmissionError::Stale
        ))
    ));
    assert_eq!((prepared.get(), applied.get()), (2, 1));
    drop(retired);
    drop(reservation);
    let _ = service.close().unwrap();
}

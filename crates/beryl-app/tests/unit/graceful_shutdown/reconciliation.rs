include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/unit/shutdown_binding.rs"
));

#[test]
fn uncertain_activation_fails_without_claiming_pending_preservation_and_can_recover_later() {
    let fixture = Fixture::new();
    let cancellation = activate(&fixture);
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    assert_eq!(
        finish_pass(&fixture, id),
        ShutdownProgress::Failed {
            reason: ShutdownFailure::UnprovenExecution {
                thread: fixture.thread,
                turn: fixture.turn
            },
            reopened: true,
        }
    );
    let home = fixture.service.home.as_deref().unwrap();
    assert!(
        fixture
            .service
            .storage
            .pending_dispatch_evidence(home, fixture.thread, point_limit())
            .unwrap()
            .is_none()
    );
    execute(
        home,
        fixture.service.storage.cancel_binding_activation(
            fixture.service.storage.revision(home).unwrap(),
            cancellation,
        ),
    );
    let fresh = fixture
        .service
        .begin_graceful_shutdown(&fixture.gate.fence().unwrap())
        .unwrap();
    assert_ne!(fresh, id);
    assert_eq!(finish_pass(&fixture, fresh), ShutdownProgress::Ready);
}

#[test]
fn reconciliation_prevents_success_and_failure_reopening_until_exact_resolution() {
    let fixture = Fixture::new();
    let cancellation = activate(&fixture);
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    let home = fixture.service.home.as_deref().unwrap();
    let storage = &fixture.service.storage;
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.cancel_binding_activation(storage.revision(home).unwrap(), cancellation))
        .unwrap();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } = home.execute(command) else {
        panic!("expected reconciliation");
    };
    assert!(home.pending_reconciliations().is_empty());
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    let stop = ProjectionCancellationToken::new();
    stop.cancel();
    assert_eq!(
        fixture
            .service
            .poll_graceful_shutdown(&fixture.sessions, id, &stop)
            .unwrap(),
        ShutdownProgress::Failed {
            reason: ShutdownFailure::Cancelled,
            reopened: false
        }
    );
    let handle = reconciliation.install_and_handle();
    assert_eq!(
        poll(&fixture, id),
        ShutdownProgress::Failed {
            reason: ShutdownFailure::Cancelled,
            reopened: false
        }
    );
    assert!(matches!(
        home.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(
        poll(&fixture, id),
        ShutdownProgress::Failed {
            reason: ShutdownFailure::Cancelled,
            reopened: true
        }
    );
    let fresh = fixture
        .service
        .begin_graceful_shutdown(&fixture.gate.fence().unwrap())
        .unwrap();
    assert_eq!(finish_pass(&fixture, fresh), ShutdownProgress::Ready);
}

#[test]
fn uninstalled_custody_blocks_ready_and_exact_resolution_completes_the_same_attempt() {
    let fixture = Fixture::new();
    let cancellation = activate(&fixture);
    let home = fixture.service.home.as_deref().unwrap();
    let storage = &fixture.service.storage;
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(storage.cancel_binding_activation(storage.revision(home).unwrap(), cancellation))
        .unwrap();
    fixture
        .faults
        .fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } = home.execute(command) else {
        panic!("expected retained reconciliation");
    };
    let fence = fixture.gate.fence().unwrap();
    let id = fixture.service.begin_graceful_shutdown(&fence).unwrap();
    assert!(home.pending_reconciliations().is_empty());
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    let handle = reconciliation.install_and_handle();
    assert_eq!(poll(&fixture, id), ShutdownProgress::Waiting);
    assert!(matches!(
        home.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(finish_pass(&fixture, id), ShutdownProgress::Ready);
}

#[test]
fn scan_revision_rejects_commits_on_either_side_of_home_election() {
    for commit_before_election in [true, false] {
        let fixture = Fixture::new();
        let cancellation = activate(&fixture);
        let home = fixture.service.home.as_deref().unwrap();
        let revision = fixture
            .service
            .shutdown_work_revision(&fixture.sessions)
            .unwrap();
        let commit = || {
            execute(
                home,
                fixture.service.storage.cancel_binding_activation(
                    fixture.service.storage.revision(home).unwrap(),
                    cancellation,
                ),
            );
        };
        let result = if commit_before_election {
            commit();
            fixture
                .service
                .validate_shutdown_scan(&fixture.sessions, &revision)
        } else {
            fixture.service.validate_shutdown_scan_with_confirmation(
                &fixture.sessions,
                &revision,
                commit,
            )
        };
        assert!(matches!(result, Err(StepError::Retry)));
    }
}

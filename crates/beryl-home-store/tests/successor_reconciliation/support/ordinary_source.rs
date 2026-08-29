use super::*;

#[test]
fn source_only_success_preserves_original_receipt_and_releases_scope() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 1))),
    );

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store.execute_current(source.current_command(SourcePut {
        key: 1,
        value: 2,
        source: SourceHook,
    })) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    committed(
        store
            .execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 42))),
    );
    let foreign_directory = tempdir().unwrap();
    let foreign = HomeStore::open(HomeOpenOptions::new(
        foreign_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    assert!(foreign.reconcile(&handle).is_err());
    foreign.close().unwrap();
    let receipt = match store.reconcile(&handle).unwrap() {
        ReconciliationResolution::ExactSuccessor { receipt } => receipt,
        other => panic!("expected exact successor, got {other:?}"),
    };
    assert_eq!(receipt.home_revision().get(), 3);
    assert!(store.pending_reconciliations().is_empty());
    store.close().unwrap();
}

#[test]
fn declared_successor_protocol_preserves_unanimous_exact_old() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 1))),
    );

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store.execute_current(source.current_command(SourcePut {
        key: 1,
        value: 2,
        source: SourceHook,
    })) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 1))),
    );
    FAIL_SOURCE.store(true, Ordering::SeqCst);
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactOld
    );
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
    assert!(store.pending_reconciliations().is_empty());
    store.close().unwrap();
}

#[test]
fn declared_successor_protocol_preserves_unanimous_exact_new() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 1))),
    );

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store.execute_current(source.current_command(SourcePut {
        key: 1,
        value: 2,
        source: SourceHook,
    })) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    FAIL_SOURCE.store(true, Ordering::SeqCst);
    assert!(matches!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
    assert!(store.pending_reconciliations().is_empty());
    store.close().unwrap();
}

#[test]
fn ineligible_exact_old_role_skips_successor_hook_and_seals_collision() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let passive = store.register_domain::<PassiveDomain>().unwrap();
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 1))),
    );
    committed(
        store.execute_current(
            passive.current_command(Put::<PassiveDomain, PassiveRecord>::new(9, 1)),
        ),
    );

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(source.contribution(
            store.domain_revision(&source).unwrap(),
            SourcePut {
                key: 1,
                value: 2,
                source: SourceHook,
            },
        ))
        .unwrap();
    command
        .add(passive.contribution(
            store.domain_revision(&passive).unwrap(),
            Put::<PassiveDomain, PassiveRecord>::new(9, 2),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = match store.execute(command) {
        CommandOutcome::Indeterminate { reconciliation, .. } => reconciliation.install_and_handle(),
        other => panic!("expected indeterminate outcome, got {other:?}"),
    };
    committed(
        store.execute_current(source.current_command(Put::<SourceDomain, SourceRecord>::new(1, 1))),
    );
    FAIL_SOURCE.store(true, Ordering::SeqCst);
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
    store.close().unwrap();
}

#[test]
fn cancelled_successor_command_never_reserves_or_invokes_protocol() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let outcome = store.execute_current(
        source
            .current_command(SourcePut {
                key: 1,
                value: 2,
                source: SourceHook,
            })
            .with_cancellation(cancellation),
    );
    assert!(matches!(
        outcome,
        CommandOutcome::NotCommitted {
            evidence: CommandError::CancelledBeforeAdmission
        }
    ));
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
    assert!(store.pending_reconciliations().is_empty());
    assert_eq!(store.home_revision().unwrap().get(), 1);
    store.close().unwrap();
}

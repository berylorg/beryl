use super::*;

#[test]
fn successor_flight_is_joined_and_worker_failure_retains_retryable_custody() {
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

    BLOCK_SOURCE.store(true, Ordering::SeqCst);
    let store = Arc::new(store);
    let first_store = Arc::clone(&store);
    let first_handle = handle.clone();
    let first = thread::spawn(move || first_store.reconcile(&first_handle));
    while SOURCE_CALLS.load(Ordering::SeqCst) == 0 {
        thread::yield_now();
    }
    let second_store = Arc::clone(&store);
    let second_handle = handle.clone();
    let second = thread::spawn(move || second_store.reconcile(&second_handle));
    thread::yield_now();
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 1);
    RELEASE_SOURCE.store(true, Ordering::SeqCst);
    assert!(matches!(
        first.join().unwrap().unwrap(),
        ReconciliationResolution::ExactSuccessor { .. }
    ));
    assert!(matches!(
        second.join().unwrap().unwrap(),
        ReconciliationResolution::ExactSuccessor { .. }
    ));
    let store = Arc::into_inner(store).unwrap();
    store.close().unwrap();

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
    FAIL_SOURCE.store(true, Ordering::SeqCst);
    assert!(store.reconcile(&handle).is_err());
    assert_eq!(store.pending_reconciliations().len(), 1);
    FAIL_SOURCE.store(false, Ordering::SeqCst);
    let retry = store.pending_reconciliations().pop().unwrap();
    assert!(matches!(
        store.retry_reconciliation(&retry).unwrap(),
        ReconciliationResolution::ExactSuccessor { .. }
    ));
    store.close().unwrap();
}

#[test]
fn successor_charge_is_reserved_before_writer_admission() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let directory = tempdir().unwrap();
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let outcome = store.execute_current(source.current_command(SourcePut {
        key: 1,
        value: 2,
        source: HugeSource,
    }));
    assert!(matches!(
        outcome,
        CommandOutcome::NotCommitted {
            evidence: CommandError::ReconciliationDescriptorTooLarge { .. }
        }
    ));
    assert_eq!(store.home_revision().unwrap().get(), 1);
    assert!(store.pending_reconciliations().is_empty());

    let outcome = store.execute_current(source.current_command(NearLimitPut));
    assert!(matches!(
        outcome,
        CommandOutcome::NotCommitted {
            evidence: CommandError::ReconciliationDescriptorTooLarge { .. }
        }
    ));
    assert_eq!(store.home_revision().unwrap().get(), 1);
    assert!(store.pending_reconciliations().is_empty());
    store.close().unwrap();
}

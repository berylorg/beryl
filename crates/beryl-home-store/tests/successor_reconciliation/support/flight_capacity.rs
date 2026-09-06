use super::*;

fn marker_handle<S>(
    store: &mut HomeStore,
    faults: &FaultController,
    source: &beryl_home_store::DomainHandle<SourceDomain>,
) -> beryl_home_store::ReconciliationHandle
where
    S: FirstAcceptancePromotionSource<SourceDomain>,
{
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    installed(
        store.execute_current(source.current_command(SourcePromotion::<S>::new(
            FirstAcceptancePromotionAdmission::MarkerFree,
        ))),
    )
}

#[test]
fn fixed_successor_flights_join_and_memoized_failures_retrigger_from_retained_custody() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    let handle = marker_handle::<BlockingMarkerSource>(&mut store, &faults, &source);
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 42)),
    ));
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
    let (_directory, faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    let handle = marker_handle::<FailingMarkerSource>(&mut store, &faults, &source);
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 42)),
    ));
    FAIL_SOURCE.store(true, Ordering::SeqCst);
    let first = store.reconcile(&handle).unwrap_err().to_string();
    let second = store.reconcile(&handle).unwrap_err().to_string();
    assert_eq!(first, second);
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 1);
    FAIL_SOURCE.store(false, Ordering::SeqCst);
    assert!(matches!(
        store.retry_reconciliation(&handle).unwrap(),
        ReconciliationResolution::ExactSuccessor { .. }
    ));
    assert!(store.pending_reconciliations().is_empty());
    store.close().unwrap();
}

#[test]
fn fixed_descriptor_limits_reject_before_writer_and_charge_retained_scopes() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, _faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let asset = store.register_domain::<AssetDomain>().unwrap();
    let mut too_large = HomeCommand::new(store.home_revision().unwrap());
    too_large
        .add(source.contribution(
            store.domain_revision(&source).unwrap(),
            SourcePromotion::<AssetSource>::new(
                FirstAcceptancePromotionAdmission::AssetTransferRequired,
            ),
        ))
        .unwrap();
    too_large
        .add(asset.contribution(
            store.domain_revision(&asset).unwrap(),
            AssetPromotion::<TooLargeAsset, TooLargeAssetRecord>::new(seed(proof(1))),
        ))
        .unwrap();
    assert!(
        matches!(store.execute(too_large), CommandOutcome::NotCommitted { evidence: CommandError::ReconciliationDescriptorTooLarge { limit, .. } } if limit == 64 * 1024 * 1024)
    );
    assert_eq!(store.home_revision().unwrap().get(), 1);

    let faults = FaultController::new();
    drop(store);
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let asset = store.register_domain::<AssetDomain>().unwrap();
    for _ in 0..4 {
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(source.contribution(
                store.domain_revision(&source).unwrap(),
                SourcePromotion::<AssetSource>::new(
                    FirstAcceptancePromotionAdmission::AssetTransferRequired,
                ),
            ))
            .unwrap();
        command
            .add(asset.contribution(
                store.domain_revision(&asset).unwrap(),
                AssetPromotion::<LargeAsset, LargeAssetRecord>::new(seed(proof(1))),
            ))
            .unwrap();
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        match store.execute(command) {
            CommandOutcome::Indeterminate { reconciliation, .. } => {
                reconciliation.install();
            }
            other => panic!("expected retained indeterminate scope, got {other:?}"),
        }
    }
    let mut saturated = HomeCommand::new(store.home_revision().unwrap());
    saturated
        .add(source.contribution(
            store.domain_revision(&source).unwrap(),
            SourcePromotion::<AssetSource>::new(
                FirstAcceptancePromotionAdmission::AssetTransferRequired,
            ),
        ))
        .unwrap();
    saturated
        .add(asset.contribution(
            store.domain_revision(&asset).unwrap(),
            AssetPromotion::<LargeAsset, LargeAssetRecord>::new(seed(proof(1))),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(saturated),
        CommandOutcome::NotCommitted {
            evidence: CommandError::ReconciliationCapacity
        }
    ));
    let close = store.close().unwrap_err();
    assert_eq!(close.pending_reconciliation_scopes(), Some(4));
}

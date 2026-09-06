use super::*;

#[test]
fn marker_free_success_reconstructs_original_receipt_and_releases_custody() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle =
        installed(store.execute_current(source.current_command(
            SourcePromotion::<MarkerSource>::new(FirstAcceptancePromotionAdmission::MarkerFree),
        )));
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 42)),
    ));
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
    assert_eq!(receipt.generation(), store.health().generation().unwrap());
    assert_eq!(
        store
            .receipt_domain_revision(&receipt, &source)
            .unwrap()
            .unwrap()
            .get(),
        3
    );
    assert!(store.pending_reconciliations().is_empty());
    store.close().unwrap();
}

#[test]
fn fixed_admission_rejects_missing_unexpected_duplicate_and_orphan_roles_before_writer() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, _faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let asset = store.register_domain::<AssetDomain>().unwrap();
    let missing =
        store.execute_current(source.current_command(SourcePromotion::<AssetSource>::new(
            FirstAcceptancePromotionAdmission::AssetTransferRequired,
        )));
    assert!(matches!(
        missing,
        CommandOutcome::NotCommitted {
            evidence: CommandError::InvalidFirstAcceptancePromotionAdmission
        }
    ));
    let unexpected =
        store.execute_current(asset.current_command(
            AssetPromotion::<NormalAsset, AssetRecord>::new(seed(proof(1))),
        ));
    assert!(matches!(
        unexpected,
        CommandOutcome::NotCommitted {
            evidence: CommandError::InvalidFirstAcceptancePromotionAdmission
        }
    ));
    let mut marker_with_asset = HomeCommand::new(store.home_revision().unwrap());
    marker_with_asset
        .add(source.contribution(
            store.domain_revision(&source).unwrap(),
            SourcePromotion::<MarkerSource>::new(FirstAcceptancePromotionAdmission::MarkerFree),
        ))
        .unwrap();
    marker_with_asset
        .add(asset.contribution(
            store.domain_revision(&asset).unwrap(),
            AssetPromotion::<NormalAsset, AssetRecord>::new(seed(proof(1))),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(marker_with_asset),
        CommandOutcome::NotCommitted {
            evidence: CommandError::InvalidFirstAcceptancePromotionAdmission
        }
    ));
    let duplicate =
        store.execute_current(source.current_command(SourcePromotion::<MarkerSource> {
            admission: FirstAcceptancePromotionAdmission::MarkerFree,
            duplicate: true,
            _source: PhantomData,
        }));
    assert!(matches!(
        duplicate,
        CommandOutcome::NotCommitted {
            evidence: CommandError::ContributorReservation { .. }
        }
    ));
    assert_eq!(store.home_revision().unwrap().get(), 1);
    assert!(store.pending_reconciliations().is_empty());
    store.close().unwrap();
}

#[test]
fn ordinary_exact_sides_and_passive_ineligibility_take_precedence_over_source_hooks() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let passive = store.register_domain::<PassiveDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let old =
        installed(store.execute_current(source.current_command(
            SourcePromotion::<MarkerSource>::new(FirstAcceptancePromotionAdmission::MarkerFree),
        )));
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    FAIL_SOURCE.store(true, Ordering::SeqCst);
    assert_eq!(
        store.reconcile(&old).unwrap(),
        ReconciliationResolution::ExactOld
    );
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
    FAIL_SOURCE.store(false, Ordering::SeqCst);
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    committed(store.execute_current(
        passive.current_command(Put::<PassiveDomain, PassiveRecord>::new(PASSIVE_KEY, 1)),
    ));
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(source.contribution(
            store.domain_revision(&source).unwrap(),
            SourcePromotion::<MarkerSource>::new(FirstAcceptancePromotionAdmission::MarkerFree),
        ))
        .unwrap();
    command
        .add(passive.contribution(
            store.domain_revision(&passive).unwrap(),
            Put::<PassiveDomain, PassiveRecord>::new(PASSIVE_KEY, 2),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let mixed = installed(store.execute(command));
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    FAIL_SOURCE.store(true, Ordering::SeqCst);
    assert_eq!(
        store.reconcile(&mixed).unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
    store.close().unwrap();
}

#[test]
fn cancellation_prevents_fixed_successor_admission() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, _faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    let outcome = store.execute_current(
        source
            .current_command(SourcePromotion::<MarkerSource>::new(
                FirstAcceptancePromotionAdmission::MarkerFree,
            ))
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
    store.close().unwrap();
}

#[test]
fn all_new_and_passive_non_new_classification_precede_fixed_source_authentication() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let passive = store.register_domain::<PassiveDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let all_new =
        installed(store.execute_current(source.current_command(
            SourcePromotion::<MarkerSource>::new(FirstAcceptancePromotionAdmission::MarkerFree),
        )));
    FAIL_SOURCE.store(true, Ordering::SeqCst);
    assert!(matches!(
        store.reconcile(&all_new).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
    FAIL_SOURCE.store(false, Ordering::SeqCst);
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    committed(store.execute_current(
        passive.current_command(Put::<PassiveDomain, PassiveRecord>::new(PASSIVE_KEY, 1)),
    ));
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(source.contribution(
            store.domain_revision(&source).unwrap(),
            SourcePromotion::<MarkerSource>::new(FirstAcceptancePromotionAdmission::MarkerFree),
        ))
        .unwrap();
    command
        .add(passive.contribution(
            store.domain_revision(&passive).unwrap(),
            Put::<PassiveDomain, PassiveRecord>::new(PASSIVE_KEY, 2),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let mixed = installed(store.execute(command));
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 42)),
    ));
    committed(store.execute_current(
        passive.current_command(Put::<PassiveDomain, PassiveRecord>::new(PASSIVE_KEY, 3)),
    ));
    FAIL_SOURCE.store(true, Ordering::SeqCst);
    assert_eq!(
        store.reconcile(&mixed).unwrap(),
        ReconciliationResolution::Collision
    );
    assert_eq!(SOURCE_CALLS.load(Ordering::SeqCst), 0);
    store.close().unwrap();
}

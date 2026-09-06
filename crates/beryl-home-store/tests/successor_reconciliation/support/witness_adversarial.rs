use super::*;

fn resolve_asset<S, A>(
    submitted_value: Option<u64>,
    original_draft: bool,
    original_accepted: bool,
) -> ReconciliationResolution
where
    S: FirstAcceptancePromotionSource<SourceDomain>,
    A: FirstAcceptancePromotionAssetAdapter<AssetDomain, OwnerHead = AssetRecord>,
{
    reset_hooks();
    let (_directory, faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let asset = store.register_domain::<AssetDomain>().unwrap();
    let passive = store.register_domain::<PassiveDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    committed(store.execute_current(
        asset.current_command(Put::<AssetDomain, AssetRecord>::new(ORDINARY_ASSET_KEY, 1)),
    ));
    if let Some(submitted_value) = submitted_value {
        committed(store.execute_current(asset.current_command(
            Put::<AssetDomain, AssetRecord>::new(SUBMITTED_KEY, submitted_value),
        )));
    }
    if original_draft {
        committed(store.execute_current(
            asset.current_command(Put::<AssetDomain, AssetRecord>::new(ORIGINAL_DRAFT_KEY, 1)),
        ));
    }
    if original_accepted {
        committed(store.execute_current(asset.current_command(
            Put::<AssetDomain, AssetRecord>::new(ORIGINAL_ACCEPTED_KEY, 1),
        )));
    }
    committed(store.execute_current(
        passive.current_command(Put::<PassiveDomain, PassiveRecord>::new(PASSIVE_KEY, 1)),
    ));
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(source.contribution(
            store.domain_revision(&source).unwrap(),
            SourcePromotion::<S>::new(FirstAcceptancePromotionAdmission::AssetTransferRequired),
        ))
        .unwrap();
    command
        .add(asset.contribution(
            store.domain_revision(&asset).unwrap(),
            AssetPromotion::<A, AssetRecord>::new(seed(proof(1))),
        ))
        .unwrap();
    command
        .add(passive.contribution(
            store.domain_revision(&passive).unwrap(),
            Put::<PassiveDomain, PassiveRecord>::new(PASSIVE_KEY, 2),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = installed(store.execute(command));
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 42)),
    ));
    let resolution = store.reconcile(&handle).unwrap();
    store.close().unwrap();
    resolution
}

#[test]
fn asset_transfer_checks_three_fixed_heads_and_restores_exact_successor_receipt() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert!(matches!(
        resolve_asset::<AssetSource, NormalAsset>(Some(42), false, false),
        ReconciliationResolution::ExactSuccessor { .. }
    ));
    assert_eq!(
        resolve_asset::<AssetSource, NormalAsset>(Some(42), true, false),
        ReconciliationResolution::Collision
    );
    assert_eq!(
        resolve_asset::<AssetSource, NormalAsset>(Some(42), false, true),
        ReconciliationResolution::Collision
    );
    assert_eq!(
        resolve_asset::<AssetSource, NormalAsset>(Some(43), false, false),
        ReconciliationResolution::Collision
    );
    assert_eq!(
        resolve_asset::<AssetSource, NormalAsset>(None, false, false),
        ReconciliationResolution::Collision
    );
}

#[test]
fn full_proof_mismatch_and_unregistered_adapter_are_rejected() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        resolve_asset::<WrongAssetSource, NormalAsset>(Some(42), false, false),
        ReconciliationResolution::Collision
    );
    reset_hooks();
    let (_directory, _faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let asset = store.register_domain::<AssetDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    committed(store.execute_current(
        asset.current_command(Put::<AssetDomain, AssetRecord>::new(ORDINARY_ASSET_KEY, 1)),
    ));
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
            AssetPromotion::<ForeignAsset, ForeignAssetRecord>::new(seed(proof(1))),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::NotCommitted {
            evidence: CommandError::ContributorAssembly { .. }
        }
    ));
    store.close().unwrap();
}

#[test]
fn invalid_oversized_and_decoded_derived_material_follow_fixed_collision_rules() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        resolve_asset::<AssetSource, InvalidKeyAsset>(Some(42), false, false),
        ReconciliationResolution::Collision
    );
    assert_eq!(
        resolve_asset::<AssetSource, OversizedKeyAsset>(Some(42), false, false),
        ReconciliationResolution::Collision
    );
    assert_eq!(
        resolve_asset::<AssetSource, RejectedAsset>(Some(42), false, false),
        ReconciliationResolution::Collision
    );
    assert_eq!(
        resolve_asset::<AssetSource, InvalidExpectedAsset>(Some(42), false, false),
        ReconciliationResolution::Collision
    );
    assert!(matches!(
        resolve_asset::<AssetSource, DecodedLimitAsset>(Some(42), false, false),
        ReconciliationResolution::ExactSuccessor { .. }
    ));
}

#[test]
fn current_stored_version_failure_retains_typed_read_provenance() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let asset = store.register_domain::<AssetDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    committed(store.execute_current(
        asset.current_command(Put::<AssetDomain, AssetRecord>::new(ORDINARY_ASSET_KEY, 1)),
    ));
    committed(store.execute_current(
        asset.current_command(Put::<AssetDomain, AssetRecord>::new(SUBMITTED_KEY, 42)),
    ));
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
            AssetPromotion::<NormalAsset, AssetRecord>::new(seed(proof(1))),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = installed(store.execute(command));
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 42)),
    ));
    store
        .inject_persisted_corrupt_record::<AssetDomain, AssetRecord>(
            &asset,
            &SUBMITTED_KEY.to_be_bytes(),
            &2_u32.to_be_bytes(),
        )
        .unwrap();
    let error = store.reconcile(&handle).unwrap_err();
    assert!(matches!(
        error
            .source()
            .and_then(|source| source.downcast_ref::<DomainCallbackSource>()),
        Some(DomainCallbackSource::Read(
            ReadError::UnsupportedRecordVersion { found: 2, .. }
        ))
    ));
}

#[test]
fn invalid_expected_rejects_before_a_malformed_current_head_is_acquired() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let asset = store.register_domain::<AssetDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    committed(store.execute_current(
        asset.current_command(Put::<AssetDomain, AssetRecord>::new(ORDINARY_ASSET_KEY, 1)),
    ));
    committed(store.execute_current(
        asset.current_command(Put::<AssetDomain, AssetRecord>::new(SUBMITTED_KEY, 42)),
    ));
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
            AssetPromotion::<DuplicateInvalidAsset, AssetRecord>::new(seed(proof(1))),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = installed(store.execute(command));
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 42)),
    ));
    store
        .inject_persisted_corrupt_record::<AssetDomain, AssetRecord>(
            &asset,
            &SUBMITTED_KEY.to_be_bytes(),
            &2_u32.to_be_bytes(),
        )
        .unwrap();
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    store.close().unwrap();
}

#[test]
fn current_decoded_limit_rejects_a_present_required_absent_head_after_expected_fits() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_hooks();
    let (_directory, faults, mut store) = open();
    let source = store.register_domain::<SourceDomain>().unwrap();
    let asset = store.register_domain::<AssetDomain>().unwrap();
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 1)),
    ));
    committed(store.execute_current(
        asset.current_command(Put::<AssetDomain, AssetRecord>::new(ORDINARY_ASSET_KEY, 1)),
    ));
    committed(store.execute_current(
        asset.current_command(Put::<AssetDomain, AssetRecord>::new(SUBMITTED_KEY, 42)),
    ));
    committed(
        store.execute_current(asset.current_command(Put::<AssetDomain, AssetRecord> {
            key: ORIGINAL_DRAFT_KEY,
            value: vec![7; 9],
            _typed: PhantomData,
        })),
    );
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
            AssetPromotion::<CurrentRejectedAsset, AssetRecord>::new(seed(proof(1))),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let handle = installed(store.execute(command));
    committed(store.execute_current(
        source.current_command(Put::<SourceDomain, SourceRecord>::new(SOURCE_KEY, 42)),
    ));
    assert_eq!(
        store.reconcile(&handle).unwrap(),
        ReconciliationResolution::Collision
    );
    store.close().unwrap();
}

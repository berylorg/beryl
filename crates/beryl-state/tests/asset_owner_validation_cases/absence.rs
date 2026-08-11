use super::*;

#[test]
fn marker_free_absence_guard_is_atomic_with_a_real_foreign_domain_mutation() {
    let directory = tempdir().unwrap();
    let (mut store, state) = support::open(directory.path());
    let probe = store.register_domain::<ProbeDomain>().unwrap();
    let source = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([1; 16]));
    let destination =
        AssetOwner::AcceptedInput(beryl_model::SyndicAcceptedInputId::from_bytes([1; 16]));
    let home_before = store.home_revision().unwrap();
    let assets_before = state.assets().revision(&store).unwrap();
    let probe_before = store.domain_revision(probe).unwrap();

    let mut command = HomeCommand::new(home_before);
    command
        .add_validation(
            state.assets().validate_owner_heads(
                assets_before,
                ValidateAssetOwnerHeads::new(Box::from([
                    AssetOwnerHeadAssertion::new(source, None),
                    AssetOwnerHeadAssertion::new(destination, None),
                ]))
                .unwrap(),
            ),
        )
        .unwrap()
        .add(probe.contribution(probe_before, PutProbe { key: 1, value: 9 }))
        .unwrap();
    let receipt = match store.execute(command) {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
        } => receipt,
        outcome => panic!("expected committed absent-owner command, got {outcome:?}"),
    };

    assert_eq!(state.assets().revision(&store).unwrap(), assets_before);
    assert_eq!(
        state.assets().committed_revision(&store, &receipt).unwrap(),
        None
    );
    assert_eq!(
        store.receipt_domain_revision(&receipt, probe).unwrap(),
        Some(probe_before.checked_next().unwrap())
    );
    assert_eq!(
        store
            .read_point::<ProbeDomain, ProbeRecord>(
                probe,
                &1,
                beryl_home_store::PointReadLimit::new(5).unwrap(),
            )
            .unwrap(),
        Some(9)
    );

    store.close().unwrap();
    let mut reopened = HomeStore::open(beryl_home_store::HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let reopened_state = BerylState::register(&mut reopened).unwrap();
    let reopened_probe = reopened.register_domain::<ProbeDomain>().unwrap();
    assert_eq!(
        reopened_state.assets().revision(&reopened).unwrap(),
        assets_before
    );
    assert!(reopened_state
        .assets()
        .owner_head(&reopened, source)
        .unwrap()
        .is_none());
    assert!(reopened_state
        .assets()
        .owner_head(&reopened, destination)
        .unwrap()
        .is_none());
    assert_eq!(
        reopened
            .read_point::<ProbeDomain, ProbeRecord>(
                reopened_probe,
                &1,
                beryl_home_store::PointReadLimit::new(5).unwrap(),
            )
            .unwrap(),
        Some(9)
    );
}

#[test]
fn absence_only_owner_head_mutation_is_rejected_before_home_store_assembly() {
    let owner = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([2; 16]));
    assert!(matches!(
        UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
            owner, None, None,
        )])),
        Err(AssetOwnerHeadUpdateError::NoEffect)
    ));
    assert!(matches!(
        UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::assert(owner, None)])),
        Err(AssetOwnerHeadUpdateError::NoEffect)
    ));
    assert!(matches!(
        ValidateAssetOwnerHeads::new(Box::new([])),
        Err(AssetOwnerHeadValidationError::Empty)
    ));
    assert!(matches!(
        ValidateAssetOwnerHeads::new(Box::from([
            AssetOwnerHeadAssertion::new(owner, None),
            AssetOwnerHeadAssertion::new(owner, None),
        ])),
        Err(AssetOwnerHeadValidationError::DuplicateOwner(actual)) if actual == owner
    ));
    let assertions = (0_u8..5)
        .map(|byte| {
            AssetOwnerHeadAssertion::new(
                AssetOwner::AcceptedInput(beryl_model::SyndicAcceptedInputId::from_bytes(
                    [byte; 16],
                )),
                None,
            )
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    assert!(matches!(
        ValidateAssetOwnerHeads::new(assertions),
        Err(AssetOwnerHeadValidationError::TooMany { actual: 5 })
    ));
}

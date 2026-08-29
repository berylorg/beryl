use super::*;

#[test]
fn validator_failure_is_atomic_and_present_transition_advances_exact_revision() {
    for stray_source in [true, false] {
        let directory = tempdir().unwrap();
        let (mut store, state) = support::open(directory.path());
        let probe = store.register_domain::<ProbeDomain>().unwrap();
        let proof = sealed_set(&store, &state, publish_asset(&store, &state));
        let source = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([3; 16]));
        let destination =
            AssetOwner::AcceptedInput(beryl_model::SyndicAcceptedInputId::from_bytes([3; 16]));
        let owner = if stray_source { source } else { destination };
        execute_asset(
            &store,
            state.assets().update_owner_heads(
                state.assets().revision(&store).unwrap(),
                UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                    owner,
                    None,
                    Some(proof),
                )]))
                .unwrap(),
            ),
        );
        let head = state.assets().owner_head(&store, owner).unwrap().unwrap();
        let other = if stray_source { destination } else { source };
        let assets_before_present_validation = state.assets().revision(&store).unwrap();
        let probe_before_present_validation = store.domain_revision(&probe).unwrap();
        let present_receipt = {
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add_validation(
                    state.assets().validate_owner_heads(
                        assets_before_present_validation,
                        ValidateAssetOwnerHeads::new(Box::from([
                            AssetOwnerHeadAssertion::new(owner, Some(head.expectation())),
                            AssetOwnerHeadAssertion::new(other, None),
                        ]))
                        .unwrap(),
                    ),
                )
                .unwrap()
                .add(probe.clone().contribution(
                    probe_before_present_validation,
                    PutProbe { key: 3, value: 8 },
                ))
                .unwrap();
            match store.execute(command) {
                CommandOutcome::Committed {
                    receipt,
                    later_failure: None,
                } => receipt,
                outcome => panic!("expected committed owner-validation command, got {outcome:?}"),
            }
        };
        assert_eq!(
            state.assets().revision(&store).unwrap(),
            assets_before_present_validation
        );
        assert_eq!(
            state
                .assets()
                .committed_revision(&store, &present_receipt)
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .read_point::<ProbeDomain, ProbeRecord>(
                    &probe,
                    &3,
                    beryl_home_store::PointReadLimit::new(5).unwrap(),
                )
                .unwrap(),
            Some(8)
        );

        let home_before = store.home_revision().unwrap();
        let assets_before = state.assets().revision(&store).unwrap();
        let probe_before = store.domain_revision(&probe).unwrap();

        let mut rejected = HomeCommand::new(home_before);
        rejected
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
            .add(
                probe
                    .clone()
                    .contribution(probe_before, PutProbe { key: 2, value: 7 }),
            )
            .unwrap();
        let error = match store.execute(rejected) {
            CommandOutcome::NotCommitted { evidence } => evidence,
            outcome => panic!("expected rejected owner-validation command, got {outcome:?}"),
        };
        assert!(matches!(
            error,
            CommandError::ContributorValidation {
                domain: "beryl-assets",
                ..
            }
        ));
        assert_eq!(store.home_revision().unwrap(), home_before);
        assert_eq!(state.assets().revision(&store).unwrap(), assets_before);
        assert_eq!(store.domain_revision(&probe).unwrap(), probe_before);
        assert_eq!(
            state.assets().owner_head(&store, owner).unwrap(),
            Some(head.clone())
        );
        assert_eq!(
            store
                .read_point::<ProbeDomain, ProbeRecord>(
                    &probe,
                    &2,
                    beryl_home_store::PointReadLimit::new(5).unwrap(),
                )
                .unwrap(),
            None
        );

        let expected_revision = state.assets().revision(&store).unwrap();
        let receipt = {
            let mut command = HomeCommand::new(store.home_revision().unwrap());
            command
                .add(
                    state.assets().update_owner_heads(
                        expected_revision,
                        UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                            owner,
                            Some(head.expectation()),
                            Some(proof),
                        )]))
                        .unwrap(),
                    ),
                )
                .unwrap();
            match store.execute(command) {
                CommandOutcome::Committed {
                    receipt,
                    later_failure: None,
                } => receipt,
                outcome => panic!("expected committed owner replacement command, got {outcome:?}"),
            }
        };
        let replaced = state.assets().owner_head(&store, owner).unwrap().unwrap();
        assert_eq!(
            replaced.owner_revision(),
            RecordRevision::new(head.owner_revision().get() + 1).unwrap()
        );
        assert_eq!(
            state.assets().committed_revision(&store, &receipt).unwrap(),
            Some(expected_revision.checked_next().unwrap())
        );

        let stale_home = store.home_revision().unwrap();
        let stale_assets = state.assets().revision(&store).unwrap();
        let stale_probe = store.domain_revision(&probe).unwrap();
        let mut stale_command = HomeCommand::new(stale_home);
        stale_command
            .add_validation(
                state.assets().validate_owner_heads(
                    stale_assets,
                    ValidateAssetOwnerHeads::new(Box::from([AssetOwnerHeadAssertion::new(
                        owner,
                        Some(head.expectation()),
                    )]))
                    .unwrap(),
                ),
            )
            .unwrap()
            .add(
                probe
                    .clone()
                    .contribution(stale_probe, PutProbe { key: 4, value: 6 }),
            )
            .unwrap();
        assert!(matches!(
            store.execute(stale_command),
            CommandOutcome::NotCommitted {
                evidence: CommandError::ContributorValidation {
                    domain: "beryl-assets",
                    ..
                },
            }
        ));
        assert_eq!(store.home_revision().unwrap(), stale_home);
        assert_eq!(state.assets().revision(&store).unwrap(), stale_assets);
        assert_eq!(store.domain_revision(&probe).unwrap(), stale_probe);
        assert_eq!(
            store
                .read_point::<ProbeDomain, ProbeRecord>(
                    &probe,
                    &4,
                    beryl_home_store::PointReadLimit::new(5).unwrap(),
                )
                .unwrap(),
            None
        );
        assert_eq!(
            state.assets().owner_head(&store, owner).unwrap(),
            Some(replaced.clone())
        );

        store.close().unwrap();
        let mut reopened = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let reopened_state = BerylState::register(&mut reopened).unwrap();
        assert_eq!(
            reopened_state
                .assets()
                .owner_head(&reopened, owner)
                .unwrap(),
            Some(replaced)
        );
    }
}

#[test]
fn mutation_participant_asserts_unchanged_head_while_publishing_another() {
    let directory = tempdir().unwrap();
    let (store, state) = support::open(directory.path());
    let proof = sealed_set(&store, &state, publish_asset(&store, &state));
    let historical = AssetOwner::SubmittedTurnItem(SyndicItemId::from_bytes([31; 16]));
    let draft = AssetOwner::CurrentDraft(SyndicDraftId::from_bytes([32; 16]));
    execute_asset(
        &store,
        state.assets().update_owner_heads(
            state.assets().revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                historical,
                None,
                Some(proof),
            )]))
            .unwrap(),
        ),
    );
    let historical_before = state
        .assets()
        .owner_head(&store, historical)
        .unwrap()
        .unwrap();
    let revision = state.assets().revision(&store).unwrap();
    execute_asset(
        &store,
        state.assets().update_owner_heads(
            revision,
            UpdateAssetOwnerHeads::new(Box::from([
                AssetOwnerHeadUpdate::assert(historical, Some(historical_before.expectation())),
                AssetOwnerHeadUpdate::replace(draft, None, Some(proof)),
            ]))
            .unwrap(),
        ),
    );
    assert_eq!(
        state.assets().owner_head(&store, historical).unwrap(),
        Some(historical_before.clone())
    );
    assert_eq!(
        state
            .assets()
            .owner_head(&store, draft)
            .unwrap()
            .unwrap()
            .set(),
        proof
    );

    execute_asset(
        &store,
        state.assets().update_owner_heads(
            state.assets().revision(&store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                historical,
                Some(historical_before.expectation()),
                Some(proof),
            )]))
            .unwrap(),
        ),
    );
    let destination =
        AssetOwner::AcceptedInput(beryl_model::SyndicAcceptedInputId::from_bytes([33; 16]));
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            state.assets().update_owner_heads(
                state.assets().revision(&store).unwrap(),
                UpdateAssetOwnerHeads::new(Box::from([
                    AssetOwnerHeadUpdate::assert(historical, Some(historical_before.expectation())),
                    AssetOwnerHeadUpdate::replace(destination, None, Some(proof)),
                ]))
                .unwrap(),
            ),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::NotCommitted {
            evidence: CommandError::ContributorValidation {
                domain: "beryl-assets",
                ..
            },
        }
    ));
    assert!(
        state
            .assets()
            .owner_head(&store, destination)
            .unwrap()
            .is_none()
    );
}

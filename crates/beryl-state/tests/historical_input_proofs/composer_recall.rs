use std::sync::Arc;

use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeProofCommand, HomeSchemaVersion,
    ProofCommandBuildError, ProofCompositionError, ProofReceiptError, ProofWitnessContribution,
    test_faults::FaultPoint,
};
use beryl_model::{
    ComposerRecallMarkerAssociationV1, ComposerRecallMarkerPageV1, ComposerRecallSourceOwnerV1,
    SyndicAcceptedInputId, SyndicDraftId, SyndicItemId, SyndicTurnId,
    composer_recall_marker_correlation,
};
use beryl_state::{
    AssetComposerRecallError, AssetOwner, AssetOwnerHeadUpdate, BerylState,
    ComposerRecallMarkerProofProtocolV1, HistoricalInputMarkerProofProtocolV1,
    UpdateAssetOwnerHeads,
};

use super::{
    fixture::{Fixture, execute, label, marker, publish_asset, seal_set},
    source::{self, SourceInput},
};

fn page(
    fixture: &Fixture,
    owner: ComposerRecallSourceOwnerV1,
    ordinal: u64,
) -> ComposerRecallMarkerPageV1 {
    ComposerRecallMarkerPageV1::new(
        SyndicDraftId::from_bytes([9; 16]),
        [10; 16],
        [11; 16],
        ordinal,
        [12; 32],
        b"component-proof-recall-header".to_vec(),
        SyndicDraftId::from_bytes([20; 16]),
        [21; 16],
        [22; 16],
        [23; 32],
        owner,
        fixture.proof,
        (ordinal - 1) * 25,
        ordinal,
        ComposerRecallMarkerAssociationV1::new(
            ordinal,
            marker(ordinal as u8),
            label(7),
            fixture.asset,
        )
        .unwrap(),
    )
    .unwrap()
}

fn submitted_owner(fixture: &Fixture) -> ComposerRecallSourceOwnerV1 {
    ComposerRecallSourceOwnerV1::SubmittedTurnItem {
        turn_id: SyndicTurnId::from_bytes([24; 16]),
        item_id: fixture.item,
    }
}

fn witness(
    fixture: &Fixture,
    page: ComposerRecallMarkerPageV1,
) -> ProofWitnessContribution<ComposerRecallMarkerProofProtocolV1> {
    fixture
        .state
        .assets()
        .composer_recall_marker_witness_with_verified_sidecar(
            &fixture.store,
            page,
            fixture.verified(),
        )
        .unwrap()
}

fn prepare(
    fixture: &Fixture,
    page: &ComposerRecallMarkerPageV1,
    witness: ProofWitnessContribution<ComposerRecallMarkerProofProtocolV1>,
) -> HomeProofCommand<ComposerRecallMarkerProofProtocolV1> {
    let source = fixture
        .source
        .proof_source::<ComposerRecallMarkerProofProtocolV1>(
            fixture.store.domain_revision(&fixture.source).unwrap(),
            SourceInput {
                correlation: composer_recall_marker_correlation(page),
                wrong_protocol: false,
                recall_protocol: true,
            },
        );
    let mut command = HomeProofCommand::new(
        fixture.store.health().generation().unwrap(),
        fixture.store.home_revision().unwrap(),
        source,
    )
    .unwrap();
    command.add_witness(witness).unwrap();
    command
}

fn compose(
    fixture: &Fixture,
    page: &ComposerRecallMarkerPageV1,
) -> Result<(), ProofCompositionError> {
    let (command, consumer) = prepare(fixture, page, witness(fixture, page.clone()))
        .seal()
        .unwrap();
    let receipt = fixture.store.compose_proof(command)?;
    fixture
        .store
        .consume_proof_receipt(consumer, receipt)
        .unwrap();
    Ok(())
}

fn assert_rejected(fixture: &Fixture, page: &ComposerRecallMarkerPageV1) {
    fixture.assert_unchanged(|| {
        assert!(matches!(
            compose(fixture, page),
            Err(ProofCompositionError::Callback { .. })
        ));
    });
}

fn replace_source(
    page: &ComposerRecallMarkerPageV1,
    owner: ComposerRecallSourceOwnerV1,
    proof: beryl_model::SealedAssetReferenceSetProof,
    association: ComposerRecallMarkerAssociationV1,
) -> ComposerRecallMarkerPageV1 {
    ComposerRecallMarkerPageV1::new(
        page.draft_id(),
        page.session_id(),
        page.operation_id(),
        page.receipt_ordinal(),
        page.receipt_digest(),
        page.canonical_header().to_vec(),
        page.source_draft_id(),
        page.source_session_id(),
        page.source_operation_id(),
        page.acceptance_digest(),
        owner,
        proof,
        page.encoded_start(),
        page.source_piece(),
        association,
    )
    .unwrap()
}

#[test]
fn ordinary_recall_supports_submitted_and_accepted_owners_without_writes() {
    let fixture = Fixture::new();
    let submitted = page(&fixture, submitted_owner(&fixture), 1);
    fixture.assert_unchanged(|| compose(&fixture, &submitted).unwrap());
    let accepted_id = SyndicAcceptedInputId::from_bytes([30; 16]);
    execute(
        &fixture.store,
        fixture.state.assets().update_owner_heads(
            fixture.state.assets().revision(&fixture.store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                AssetOwner::AcceptedInput(accepted_id),
                None,
                Some(fixture.proof),
            )]))
            .unwrap(),
        ),
    );
    let accepted = page(
        &fixture,
        ComposerRecallSourceOwnerV1::AcceptedInput(accepted_id),
        1,
    );
    fixture.assert_unchanged(|| compose(&fixture, &accepted).unwrap());
}

#[test]
fn promoted_ownership_requires_a_new_exact_window() {
    let fixture = Fixture::new();
    let accepted_id = SyndicAcceptedInputId::from_bytes([31; 16]);
    let submitted_head = fixture
        .state
        .assets()
        .owner_head(&fixture.store, AssetOwner::SubmittedTurnItem(fixture.item))
        .unwrap()
        .unwrap();
    execute(
        &fixture.store,
        fixture.state.assets().update_owner_heads(
            fixture.state.assets().revision(&fixture.store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([
                AssetOwnerHeadUpdate::replace(
                    AssetOwner::SubmittedTurnItem(fixture.item),
                    Some(submitted_head.expectation()),
                    None,
                ),
                AssetOwnerHeadUpdate::replace(
                    AssetOwner::AcceptedInput(accepted_id),
                    None,
                    Some(fixture.proof),
                ),
            ]))
            .unwrap(),
        ),
    );
    let accepted = page(
        &fixture,
        ComposerRecallSourceOwnerV1::AcceptedInput(accepted_id),
        1,
    );
    compose(&fixture, &accepted).unwrap();
    let old_attempt = prepare(&fixture, &accepted, witness(&fixture, accepted.clone()))
        .seal()
        .unwrap();
    let accepted_head = fixture
        .state
        .assets()
        .owner_head(&fixture.store, AssetOwner::AcceptedInput(accepted_id))
        .unwrap()
        .unwrap();
    execute(
        &fixture.store,
        fixture.state.assets().update_owner_heads(
            fixture.state.assets().revision(&fixture.store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([
                AssetOwnerHeadUpdate::replace(
                    AssetOwner::AcceptedInput(accepted_id),
                    Some(accepted_head.expectation()),
                    None,
                ),
                AssetOwnerHeadUpdate::replace(
                    AssetOwner::SubmittedTurnItem(fixture.item),
                    None,
                    Some(fixture.proof),
                ),
            ]))
            .unwrap(),
        ),
    );
    fixture.assert_unchanged(|| assert!(fixture.store.compose_proof(old_attempt.0).is_err()));
    assert_rejected(&fixture, &accepted);
    fixture.assert_unchanged(|| {
        compose(&fixture, &page(&fixture, submitted_owner(&fixture), 2)).unwrap()
    });
}

#[test]
fn source_head_set_and_each_ordered_association_are_exact() {
    let fixture = Fixture::new();
    let original = page(&fixture, submitted_owner(&fixture), 1);
    let missing =
        ComposerRecallSourceOwnerV1::AcceptedInput(SyndicAcceptedInputId::from_bytes([99; 16]));
    assert_rejected(
        &fixture,
        &replace_source(&original, missing, original.proof(), original.association()),
    );
    let missing = ComposerRecallSourceOwnerV1::SubmittedTurnItem {
        turn_id: SyndicTurnId::from_bytes([24; 16]),
        item_id: SyndicItemId::from_bytes([99; 16]),
    };
    assert_rejected(
        &fixture,
        &replace_source(&original, missing, original.proof(), original.association()),
    );
    let other_set = seal_set(
        &fixture.store,
        &fixture.state,
        2,
        &[(marker(1), label(7), fixture.asset)],
    );
    assert_rejected(
        &fixture,
        &replace_source(
            &original,
            original.source_owner(),
            other_set,
            original.association(),
        ),
    );
    for association in [
        ComposerRecallMarkerAssociationV1::new(2, marker(1), label(7), fixture.asset).unwrap(),
        ComposerRecallMarkerAssociationV1::new(1, marker(99), label(7), fixture.asset).unwrap(),
        ComposerRecallMarkerAssociationV1::new(1, marker(1), label(8), fixture.asset).unwrap(),
    ] {
        assert_rejected(
            &fixture,
            &replace_source(
                &original,
                original.source_owner(),
                original.proof(),
                association,
            ),
        );
    }
    let (other_asset, _) = publish_asset(&fixture.store, &fixture.state, b"different recall asset");
    let substituted = replace_source(
        &original,
        original.source_owner(),
        original.proof(),
        ComposerRecallMarkerAssociationV1::new(1, marker(1), label(7), other_asset).unwrap(),
    );
    let available = Arc::new(
        fixture
            .state
            .assets()
            .verify_sidecar(&fixture.store, other_asset)
            .unwrap(),
    );
    let contribution = fixture
        .state
        .assets()
        .composer_recall_marker_witness_with_verified_sidecar(
            &fixture.store,
            substituted.clone(),
            available,
        )
        .unwrap();
    let (command, _) = prepare(&fixture, &substituted, contribution)
        .seal()
        .unwrap();
    fixture.assert_unchanged(|| {
        assert!(matches!(
            fixture.store.compose_proof(command),
            Err(ProofCompositionError::Callback { .. })
        ))
    });
}

#[test]
fn original_receipt_consumer_and_recall_protocol_cannot_be_substituted() {
    let fixture = Fixture::new();
    fixture.assert_unchanged(|| {
        let first = page(&fixture, submitted_owner(&fixture), 1);
        let second = page(&fixture, submitted_owner(&fixture), 2);
        let (command, _) = prepare(&fixture, &first, witness(&fixture, first.clone()))
            .seal()
            .unwrap();
        let (_, foreign_consumer) = prepare(&fixture, &second, witness(&fixture, second.clone()))
            .seal()
            .unwrap();
        let receipt = fixture.store.compose_proof(command).unwrap();
        assert!(matches!(
            fixture
                .store
                .consume_proof_receipt(foreign_consumer, receipt),
            Err(ProofReceiptError::SourceFenceMismatch)
        ));
        let source = fixture
            .source
            .proof_source::<HistoricalInputMarkerProofProtocolV1>(
                fixture.store.domain_revision(&fixture.source).unwrap(),
                SourceInput {
                    correlation: composer_recall_marker_correlation(&first),
                    wrong_protocol: false,
                    recall_protocol: true,
                },
            );
        assert!(matches!(
            HomeProofCommand::new(
                fixture.store.health().generation().unwrap(),
                fixture.store.home_revision().unwrap(),
                source,
            ),
            Err(ProofCommandBuildError::ProtocolMismatch)
        ));
    });
}

#[test]
fn witness_correlation_cannot_replace_original_acceptance_or_promoted_turn() {
    let fixture = Fixture::new();
    let original = page(&fixture, submitted_owner(&fixture), 1);
    for changed in 0..5 {
        let other = ComposerRecallMarkerPageV1::new(
            original.draft_id(),
            original.session_id(),
            original.operation_id(),
            original.receipt_ordinal(),
            original.receipt_digest(),
            original.canonical_header().to_vec(),
            if changed == 0 {
                SyndicDraftId::from_bytes([99; 16])
            } else {
                original.source_draft_id()
            },
            if changed == 1 {
                [99; 16]
            } else {
                original.source_session_id()
            },
            if changed == 2 {
                [99; 16]
            } else {
                original.source_operation_id()
            },
            if changed == 3 {
                [99; 32]
            } else {
                original.acceptance_digest()
            },
            if changed == 4 {
                ComposerRecallSourceOwnerV1::SubmittedTurnItem {
                    turn_id: SyndicTurnId::from_bytes([99; 16]),
                    item_id: fixture.item,
                }
            } else {
                original.source_owner()
            },
            original.proof(),
            original.encoded_start(),
            original.source_piece(),
            original.association(),
        )
        .unwrap();
        let (command, _) = prepare(&fixture, &original, witness(&fixture, other))
            .seal()
            .unwrap();
        fixture.assert_unchanged(|| {
            assert!(matches!(
                fixture.store.compose_proof(command),
                Err(ProofCompositionError::Disagreement { .. })
            ))
        });
    }
}

#[test]
fn dropping_uncomposed_proof_releases_verified_bytes_without_state_changes() {
    let fixture = Fixture::new();
    let page = page(&fixture, submitted_owner(&fixture), 1);
    let available = fixture.verified();
    fixture.assert_unchanged(|| {
        let witness = fixture
            .state
            .assets()
            .composer_recall_marker_witness_with_verified_sidecar(
                &fixture.store,
                page.clone(),
                Arc::clone(&available),
            )
            .unwrap();
        assert_eq!(Arc::strong_count(&available), 2);
        let (command, consumer) = prepare(&fixture, &page, witness).seal().unwrap();
        drop(command);
        drop(consumer);
        assert_eq!(Arc::strong_count(&available), 1);
    });
}

#[test]
fn repeated_windows_reuse_same_scope_sidecar_without_reopening_bytes() {
    let fixture = Fixture::new();
    let verified = fixture.verified();
    fixture.faults.fail_next_with_kind(
        FaultPoint::BeforeSidecarVerification,
        std::io::ErrorKind::NotFound,
    );
    fixture.assert_unchanged(|| {
        for ordinal in 1..=2 {
            let page = page(&fixture, submitted_owner(&fixture), ordinal);
            let witness = fixture
                .state
                .assets()
                .composer_recall_marker_witness_with_verified_sidecar(
                    &fixture.store,
                    page.clone(),
                    Arc::clone(&verified),
                )
                .unwrap();
            let (command, consumer) = prepare(&fixture, &page, witness).seal().unwrap();
            let receipt = fixture.store.compose_proof(command).unwrap();
            fixture
                .store
                .consume_proof_receipt(consumer, receipt)
                .unwrap();
        }
    });
    assert!(matches!(
        fixture.state.assets().composer_recall_marker_witness(
            &fixture.store,
            page(&fixture, submitted_owner(&fixture), 1),
        ),
        Err(AssetComposerRecallError::Sidecar(_))
    ));
}

#[test]
fn foreign_home_sidecar_does_not_grant_recall_witness_authority() {
    let fixture = Fixture::new();
    let foreign = Fixture::new();
    fixture.assert_unchanged(|| {
        assert!(matches!(
            fixture
                .state
                .assets()
                .composer_recall_marker_witness_with_verified_sidecar(
                    &fixture.store,
                    page(&fixture, submitted_owner(&fixture), 1),
                    foreign.verified(),
                ),
            Err(AssetComposerRecallError::Rejected)
        ))
    });
}

#[test]
fn reopening_does_not_requalify_retired_sidecar_authority() {
    let fixture = Fixture::new();
    let verified = fixture.verified();
    let captured = page(&fixture, submitted_owner(&fixture), 1);
    fixture.store.close().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        &fixture.path,
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    candidate.register_domain::<source::SourceDomain>().unwrap();
    let store = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .with_domain::<source::SourceDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let revision = store.home_revision().unwrap();
    assert!(matches!(
        state
            .assets()
            .composer_recall_marker_witness_with_verified_sidecar(&store, captured, verified),
        Err(AssetComposerRecallError::Rejected)
    ));
    assert_eq!(store.home_revision().unwrap(), revision);
}

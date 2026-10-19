#![cfg(feature = "test-faults")]

#[path = "historical_input_proofs/composer_recall.rs"]
mod composer_recall;
#[path = "historical_input_proofs/fixture.rs"]
mod fixture;
#[path = "historical_input_proofs/sidecar_cases.rs"]
mod sidecar_cases;
#[path = "historical_input_proofs/source.rs"]
mod source;

use std::sync::Arc;

use beryl_home_store::{
    HomeProofCommand, ProofCommandBuildError, ProofCompositionError, ProofReceiptError,
    test_faults::FaultPoint,
};
use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, HistoricalInputMarkerAssociationV1,
    HistoricalInputMarkerPageV1, SealedAssetReferenceSetProof, SyndicItemId,
};
use beryl_state::{
    AssetHistoricalInputError, AssetOwner, AssetOwnerHeadUpdate,
    AssetReferenceSetManifestCorruption, HistoricalInputMarkerProofProtocolV1,
    UpdateAssetOwnerHeads,
};

use fixture::{
    Fixture, execute, label, marker, publish_asset, seal_set, with_association,
    with_owner_and_proof,
};
use source::{SourceInput, correlation};

#[test]
fn local_historical_image_proof_is_read_only() {
    let fixture = Fixture::new();
    let page = fixture.page(1);
    fixture.assert_unchanged(|| {
        let witness = fixture
            .state
            .assets()
            .historical_input_marker_witness(&fixture.store, page.clone())
            .unwrap();
        fixture.compose(&page, witness).unwrap();
    });
}

#[test]
fn repeated_marker_windows_reuse_verified_bytes_without_reopening_the_sidecar() {
    let fixture = Fixture::new();
    let verified = fixture.verified();
    fixture.faults.fail_next_with_kind(
        FaultPoint::BeforeSidecarVerification,
        std::io::ErrorKind::NotFound,
    );
    fixture.assert_unchanged(|| {
        for ordinal in 1..=2 {
            let page = fixture.page(ordinal);
            let witness = fixture.witness(page.clone(), Arc::clone(&verified));
            fixture.compose(&page, witness).unwrap();
        }
    });
    assert!(matches!(
        fixture
            .state
            .assets()
            .historical_input_marker_witness(&fixture.store, fixture.page(1)),
        Err(AssetHistoricalInputError::Sidecar(_))
    ));
}

#[test]
fn wrong_submitted_item_head_rejects_without_mutation() {
    let fixture = Fixture::new();
    let page = fixture.page(1);
    fixture.assert_callback_rejected(with_owner_and_proof(
        &page,
        SyndicItemId::from_bytes([99; 16]),
        page.proof(),
        page.association(),
    ));
}

#[test]
fn detached_sealed_set_does_not_substitute_for_the_submitted_item_head() {
    let fixture = Fixture::new();
    let other = seal_set(
        &fixture.store,
        &fixture.state,
        2,
        &[
            (marker(1), label(7), fixture.asset),
            (marker(2), label(7), fixture.asset),
        ],
    );
    let page = fixture.page(1);
    fixture.assert_callback_rejected(with_owner_and_proof(
        &page,
        page.item_id(),
        other,
        page.association(),
    ));
}

#[test]
fn submitted_head_requires_the_exact_sealed_chain_digest() {
    let fixture = Fixture::new();
    let page = fixture.page(1);
    let proof = page.proof();
    let substituted = SealedAssetReferenceSetProof::new(
        proof.set_id(),
        proof.sequential(),
        proof.ordered_assets(),
        proof.entry_frontier(),
        AssetReferenceSetDigest::from_bytes([99; 32]),
    )
    .unwrap();
    fixture.assert_callback_rejected(with_owner_and_proof(
        &page,
        page.item_id(),
        substituted,
        page.association(),
    ));
}

#[test]
fn ordinal_marker_and_label_must_match_the_ordered_sealed_entry() {
    let fixture = Fixture::new();
    let page = fixture.page(1);
    for association in [
        HistoricalInputMarkerAssociationV1::new(2, marker(1), label(7), fixture.asset).unwrap(),
        HistoricalInputMarkerAssociationV1::new(1, marker(99), label(7), fixture.asset).unwrap(),
        HistoricalInputMarkerAssociationV1::new(1, marker(1), label(8), fixture.asset).unwrap(),
    ] {
        fixture.assert_callback_rejected(with_association(&page, association));
    }
}

#[test]
fn another_available_asset_cannot_replace_the_sealed_association() {
    let fixture = Fixture::new();
    let (other, _) = publish_asset(&fixture.store, &fixture.state, b"different local image");
    let page = with_association(
        &fixture.page(1),
        HistoricalInputMarkerAssociationV1::new(1, marker(1), label(7), other).unwrap(),
    );
    let verified = Arc::new(
        fixture
            .state
            .assets()
            .verify_sidecar(&fixture.store, other)
            .unwrap(),
    );
    fixture.assert_unchanged(|| {
        let witness = fixture.witness(page.clone(), verified);
        assert!(matches!(fixture.compose(&page, witness), Err(ProofCompositionError::Callback { source, .. })
            if source.is::<beryl_state::AssetDraftMarkerLabelReadinessError>()));
    });
}

#[test]
fn changed_source_window_disagrees_with_the_exact_witness_correlation() {
    let fixture = Fixture::new();
    let page = fixture.page(1);
    for changed in 0..9 {
        let other = HistoricalInputMarkerPageV1::new(
            if changed == 0 {
                beryl_model::SyndicDraftId::from_bytes([99; 16])
            } else {
                page.draft_id()
            },
            if changed == 1 {
                [99; 16]
            } else {
                page.session_id()
            },
            if changed == 2 {
                [99; 16]
            } else {
                page.operation_id()
            },
            if changed == 3 {
                99
            } else {
                page.receipt_ordinal()
            },
            if changed == 4 {
                [99; 32]
            } else {
                page.receipt_digest()
            },
            if changed == 5 {
                b"substituted header".to_vec()
            } else {
                page.canonical_header().to_vec()
            },
            page.item_id(),
            page.proof(),
            if changed == 6 {
                25
            } else {
                page.encoded_start()
            },
            if changed == 7 { 2 } else { page.source_piece() },
            if changed == 8 {
                fixture.page(2).association()
            } else {
                page.association()
            },
        )
        .unwrap();
        fixture.assert_unchanged(|| {
            let witness = fixture.witness(other, fixture.verified());
            assert!(matches!(
                fixture.compose(&page, witness),
                Err(ProofCompositionError::Disagreement { .. })
            ));
        });
    }
}

#[test]
fn protocol_mismatch_rejects_before_composition_without_mutation() {
    let fixture = Fixture::new();
    fixture.assert_unchanged(|| {
        let source = fixture
            .source
            .proof_source::<HistoricalInputMarkerProofProtocolV1>(
                fixture.store.domain_revision(&fixture.source).unwrap(),
                SourceInput {
                    correlation: correlation(&fixture.page(1)),
                    wrong_protocol: true,
                    recall_protocol: false,
                },
            );
        assert!(matches!(
            HomeProofCommand::new(
                fixture.store.health().generation().unwrap(),
                fixture.store.home_revision().unwrap(),
                source
            ),
            Err(ProofCommandBuildError::ProtocolMismatch)
        ));
    });
}

#[test]
fn opaque_receipt_cannot_be_consumed_by_another_marker_attempt() {
    let fixture = Fixture::new();
    fixture.assert_unchanged(|| {
        let mut attempts = Vec::new();
        for ordinal in 1..=2 {
            let page = fixture.page(ordinal);
            let source = fixture
                .source
                .proof_source::<HistoricalInputMarkerProofProtocolV1>(
                    fixture.store.domain_revision(&fixture.source).unwrap(),
                    SourceInput {
                        correlation: correlation(&page),
                        wrong_protocol: false,
                        recall_protocol: false,
                    },
                );
            let mut command = HomeProofCommand::new(
                fixture.store.health().generation().unwrap(),
                fixture.store.home_revision().unwrap(),
                source,
            )
            .unwrap();
            command
                .add_witness(fixture.witness(page, fixture.verified()))
                .unwrap();
            attempts.push(command.seal().unwrap());
        }
        let (command, _) = attempts.remove(0);
        let (_, consumer) = attempts.remove(0);
        let receipt = fixture.store.compose_proof(command).unwrap();
        assert!(matches!(
            fixture.store.consume_proof_receipt(consumer, receipt),
            Err(ProofReceiptError::SourceFenceMismatch)
        ));
    });
}

#[test]
fn missing_completion_evidence_rejects_even_with_verified_bytes() {
    let fixture = Fixture::new();
    let verified = fixture.verified();
    execute(
        &fixture.store,
        fixture
            .state
            .assets()
            .remove_reference_set_completion_evidence_for_test(
                fixture.state.assets().revision(&fixture.store).unwrap(),
                fixture.proof.set_id(),
            ),
    );
    fixture.assert_unchanged(|| {
        let page = fixture.page(1);
        let witness = fixture.witness(page.clone(), verified);
        assert!(matches!(
            fixture.compose(&page, witness),
            Err(ProofCompositionError::Callback { .. })
        ));
    });
}

#[test]
fn incomplete_or_substituted_manifest_rejects_even_with_verified_bytes() {
    for corruption in [
        AssetReferenceSetManifestCorruption::Lifecycle,
        AssetReferenceSetManifestCorruption::Sequential,
        AssetReferenceSetManifestCorruption::OrderedAssets,
        AssetReferenceSetManifestCorruption::EntryFrontier,
        AssetReferenceSetManifestCorruption::AssetChain,
    ] {
        let fixture = Fixture::new();
        execute(
            &fixture.store,
            fixture
                .state
                .assets()
                .corrupt_reference_set_manifest_for_test(
                    fixture.state.assets().revision(&fixture.store).unwrap(),
                    fixture.proof.set_id(),
                    corruption,
                ),
        );
        fixture.assert_callback_rejected(fixture.page(1));
    }
}

#[test]
fn committed_metadata_is_still_required_after_byte_verification() {
    let fixture = Fixture::new();
    let verified = fixture.verified();
    execute(
        &fixture.store,
        fixture.state.assets().remove_metadata_for_test(
            fixture.state.assets().revision(&fixture.store).unwrap(),
            fixture.asset,
        ),
    );
    fixture.assert_unchanged(|| {
        let page = fixture.page(1);
        let witness = fixture.witness(page.clone(), verified);
        assert!(matches!(
            fixture.compose(&page, witness),
            Err(ProofCompositionError::Callback { .. })
        ));
    });
}

#[test]
fn owner_change_after_witness_construction_rejects_the_stale_revision() {
    let fixture = Fixture::new();
    let page = fixture.page(1);
    let witness = fixture.witness(page.clone(), fixture.verified());
    let owner = AssetOwner::SubmittedTurnItem(fixture.item);
    let head = fixture
        .state
        .assets()
        .owner_head(&fixture.store, owner)
        .unwrap()
        .unwrap();
    execute(
        &fixture.store,
        fixture.state.assets().update_owner_heads(
            fixture.state.assets().revision(&fixture.store).unwrap(),
            UpdateAssetOwnerHeads::new(Box::from([AssetOwnerHeadUpdate::replace(
                owner,
                Some(head.expectation()),
                None,
            )]))
            .unwrap(),
        ),
    );
    fixture.assert_unchanged(|| {
        assert!(matches!(
            fixture.compose(&page, witness),
            Err(ProofCompositionError::Conflict { .. })
        ));
    });
}

#[test]
fn absent_manifest_is_not_replaced_by_a_descriptive_proof() {
    let fixture = Fixture::new();
    let page = fixture.page(1);
    let proof = page.proof();
    let absent = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([99; 16]),
        proof.sequential(),
        proof.ordered_assets(),
        proof.entry_frontier(),
        proof.asset_chain_digest(),
    )
    .unwrap();
    let owner = AssetOwner::SubmittedTurnItem(fixture.item);
    let head = fixture
        .state
        .assets()
        .owner_head(&fixture.store, owner)
        .unwrap()
        .unwrap();
    execute(
        &fixture.store,
        fixture.state.assets().corrupt_owner_head_proof_for_test(
            fixture.state.assets().revision(&fixture.store).unwrap(),
            owner,
            head.expectation(),
            absent,
        ),
    );
    fixture.assert_callback_rejected(with_owner_and_proof(
        &page,
        page.item_id(),
        absent,
        page.association(),
    ));
}

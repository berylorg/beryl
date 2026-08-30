#![cfg(feature = "test-faults")]

include!("phase154_durable_builder/support.rs");

use std::num::NonZeroU64;

use beryl_model::{
    AssetId, AssetReferenceSetId, ContentRevision, DraftRevision, InputGateRevision,
    OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof, SequentialMarkerSummaryV1,
    ThreadRevision, advance_ordered_marker_asset_digest, advance_sequential_marker_digest,
    ordered_marker_asset_digest_seed, sequential_marker_digest_seed,
};
use beryl_state::{
    AppendAssetReferencePage, AssetMediaType, AssetReferencePageEntry,
    AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, BerylState, PublishAssetMetadata,
    SealAssetReferenceSet,
};
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};
use syndic_storage::{
    AcceptedInputAdmissionProof, AcceptedInputOrdinal, AcceptedInputRecord,
    AcceptedRouteGeneration, ComposerAtom, ComposerPayload, DraftImageLabelProtectionHeadV1,
    DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionOperationIdV1,
    DraftMarkerAdmissionOwnerV1, DraftMarkerReadinessAcceptedSourceV1,
    DraftMarkerReadinessCandidateSourceV1, DraftMarkerReadinessSourceAssociationV1,
    DraftMarkerReadinessSourceErrorV1, DraftMarkerReadinessSourceSelectorV1,
    DraftMarkerReadinessWitnessFactoryV1, ImageLabelAuthorityHeadV1, ImageLabelFrontier,
    ImageLabelOriginOwner, ImageLabelOriginSpanRecord, PreparedContent, SelectedPathProof,
    ThreadLineageDepth, ThreadLineageProof, ThreadRecord, child_thread_lineage_digest,
    empty_selected_path_digest,
};

#[path = "phase217_draft_marker_readiness_accepted_proof/support.rs"]
mod phase217_support;
#[path = "support/mod.rs"]
mod support;

use phase217_support::*;

fn readiness_owner(
    session: &DraftEditorCandidateSessionV1,
    seed: u8,
) -> DraftMarkerAdmissionOwnerV1 {
    DraftMarkerAdmissionOwnerV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes([seed; 16]),
    )
}

#[test]
fn accepted_local_and_inherited_pages_share_the_fixed_vector_without_mutation() {
    let fixture = AcceptedFixture::new("phase217-valid", 1);
    let home_revision = fixture.store.home_revision().unwrap();
    let syndic_revision = fixture.storage.revision(&fixture.store).unwrap();
    let asset_revision = fixture.state.assets().revision(&fixture.store).unwrap();
    let entry = manual_accepted_entry(fixture.proof, fixture.label, fixture.asset_id);
    assert_eq!(entry.len(), 194);

    for (index, source_thread) in [fixture.thread, fixture.child].into_iter().enumerate() {
        let ordinal = NonZeroU64::new(index as u64 + 1).unwrap();
        let mut attempt = fixture
            .storage
            .prepare_draft_marker_label_readiness_page_for_test(
                &fixture.store,
                readiness_owner(&fixture.session, 50),
                DraftMarkerAdmissionCommandIdV1::from_bytes([60 + index as u8; 16]),
                ordinal,
                true,
                Box::new([fixture.association(70 + index as u8, source_thread)]),
                Some(fixture.factory()),
            )
            .unwrap();
        assert_eq!(
            attempt.expected_source_correlation_for_test(),
            manual_correlation(ordinal, true, std::slice::from_ref(&entry))
        );
        let receipt = fixture
            .store
            .compose_proof(attempt.take_command().unwrap())
            .unwrap();
        let _proven = attempt.consume(&fixture.store, receipt).unwrap();
    }

    assert_eq!(fixture.store.home_revision().unwrap(), home_revision);
    assert_eq!(
        fixture.storage.revision(&fixture.store).unwrap(),
        syndic_revision
    );
    assert_eq!(
        fixture.state.assets().revision(&fixture.store).unwrap(),
        asset_revision
    );
}

#[test]
fn accepted_occurrence_multiplicity_and_count_boundary_are_exact() {
    let fixture = AcceptedFixture::new("phase217-count", 2);
    let mut associations = Vec::new();
    for index in 0_u16..256 {
        let mut target = [0_u8; 16];
        target[..2].copy_from_slice(&index.to_le_bytes());
        associations.push(DraftMarkerReadinessSourceAssociationV1::new(
            SyndicDraftMarkerId::from_bytes(target),
            DraftMarkerReadinessSourceSelectorV1::Accepted(
                DraftMarkerReadinessAcceptedSourceV1::new(
                    fixture.thread,
                    fixture.proof,
                    fixture.label,
                    fixture.asset_id,
                ),
            ),
        ));
    }
    let entry = manual_accepted_entry(fixture.proof, fixture.label, fixture.asset_id);
    let entries = vec![entry; 256];
    assert_eq!(entries.iter().map(Vec::len).sum::<usize>(), 49_664);
    let ordinal = NonZeroU64::MIN;
    let mut attempt = fixture
        .storage
        .prepare_draft_marker_label_readiness_page_for_test(
            &fixture.store,
            readiness_owner(&fixture.session, 80),
            DraftMarkerAdmissionCommandIdV1::from_bytes([81; 16]),
            ordinal,
            true,
            associations.clone().into_boxed_slice(),
            Some(fixture.factory()),
        )
        .unwrap();
    assert_eq!(
        attempt.expected_source_correlation_for_test(),
        manual_correlation(ordinal, true, &entries)
    );
    let receipt = fixture
        .store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    let _proven = attempt.consume(&fixture.store, receipt).unwrap();

    associations.push(fixture.association(99, fixture.thread));
    assert!(matches!(
        fixture
            .storage
            .prepare_draft_marker_label_readiness_page_for_test(
                &fixture.store,
                readiness_owner(&fixture.session, 82),
                DraftMarkerAdmissionCommandIdV1::from_bytes([83; 16]),
                ordinal,
                true,
                associations.into_boxed_slice(),
                Some(fixture.factory()),
            ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
}

#[test]
fn empty_mixed_duplicate_and_missing_witness_pages_are_rejected() {
    let fixture = AcceptedFixture::new("phase217-shape", 3);
    let foreign = AcceptedFixture::new("phase217-foreign-witness", 33);
    let owner = readiness_owner(&fixture.session, 90);
    let ordinal = NonZeroU64::MIN;
    assert!(matches!(
        fixture
            .storage
            .prepare_draft_marker_label_readiness_page_for_test(
                &fixture.store,
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([91; 16]),
                ordinal,
                true,
                Box::new([]),
                None,
            ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    let accepted = fixture.association(92, fixture.thread);
    assert!(matches!(
        fixture
            .storage
            .prepare_draft_marker_label_readiness_page_for_test(
                &fixture.store,
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([93; 16]),
                ordinal,
                true,
                Box::new([accepted]),
                None,
            ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    let candidate = DraftMarkerReadinessSourceAssociationV1::new(
        SyndicDraftMarkerId::from_bytes([94; 16]),
        DraftMarkerReadinessSourceSelectorV1::Candidate(
            DraftMarkerReadinessCandidateSourceV1::new(
                fixture.session.draft_id(),
                fixture.session.session_id(),
                fixture.session.newest_candidate_generation(),
                fixture.session.newest_root(),
                SyndicDraftMarkerId::from_bytes([95; 16]),
            ),
        ),
    );
    assert!(matches!(
        fixture
            .storage
            .prepare_draft_marker_label_readiness_page_for_test(
                &fixture.store,
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([96; 16]),
                ordinal,
                true,
                Box::new([accepted, candidate]),
                Some(fixture.factory()),
            ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    assert!(matches!(
        fixture
            .storage
            .prepare_draft_marker_label_readiness_page_for_test(
                &fixture.store,
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([97; 16]),
                ordinal,
                true,
                Box::new([accepted, accepted]),
                Some(fixture.factory()),
            ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
    assert!(matches!(
        fixture
            .storage
            .prepare_draft_marker_label_readiness_page_for_test(
                &fixture.store,
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes([98; 16]),
                ordinal,
                true,
                Box::new([accepted]),
                Some(foreign.factory()),
            ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));
}

#[test]
fn missing_disagreeing_and_stale_cross_domain_evidence_rejects() {
    let fixture = AcceptedFixture::new("phase217-reject", 4);
    let ordinal = NonZeroU64::MIN;
    let missing = DraftMarkerReadinessSourceAssociationV1::new(
        SyndicDraftMarkerId::from_bytes([100; 16]),
        DraftMarkerReadinessSourceSelectorV1::Accepted(DraftMarkerReadinessAcceptedSourceV1::new(
            SyndicThreadId::from_bytes([101; 16]),
            fixture.proof,
            fixture.label,
            fixture.asset_id,
        )),
    );
    assert!(matches!(
        fixture
            .storage
            .prepare_draft_marker_label_readiness_page_for_test(
                &fixture.store,
                readiness_owner(&fixture.session, 102),
                DraftMarkerAdmissionCommandIdV1::from_bytes([103; 16]),
                ordinal,
                true,
                Box::new([missing]),
                Some(fixture.factory()),
            ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));

    let wrong_proof = SealedAssetReferenceSetProof::new(
        AssetReferenceSetId::from_bytes([102; 16]),
        fixture.proof.sequential(),
        fixture.proof.ordered_assets(),
        fixture.proof.entry_frontier(),
        fixture.proof.asset_chain_digest(),
    )
    .unwrap();
    let wrong_source = DraftMarkerReadinessSourceAssociationV1::new(
        SyndicDraftMarkerId::from_bytes([103; 16]),
        DraftMarkerReadinessSourceSelectorV1::Accepted(DraftMarkerReadinessAcceptedSourceV1::new(
            fixture.thread,
            wrong_proof,
            fixture.label,
            fixture.asset_id,
        )),
    );
    assert!(matches!(
        fixture
            .storage
            .prepare_draft_marker_label_readiness_page_for_test(
                &fixture.store,
                readiness_owner(&fixture.session, 104),
                DraftMarkerAdmissionCommandIdV1::from_bytes([105; 16]),
                ordinal,
                true,
                Box::new([wrong_source]),
                Some(fixture.factory()),
            ),
        Err(DraftMarkerReadinessSourceErrorV1::Rejected)
    ));

    let wrong_asset = AssetId::sha256_v1(
        [104; 32],
        NonZeroU64::new(fixture.asset_id.length().get()).unwrap(),
    );
    let disagreeing = DraftMarkerReadinessSourceAssociationV1::new(
        SyndicDraftMarkerId::from_bytes([105; 16]),
        DraftMarkerReadinessSourceSelectorV1::Accepted(DraftMarkerReadinessAcceptedSourceV1::new(
            fixture.thread,
            fixture.proof,
            fixture.label,
            wrong_asset,
        )),
    );
    let mut disagreeing_attempt = fixture
        .storage
        .prepare_draft_marker_label_readiness_page_for_test(
            &fixture.store,
            readiness_owner(&fixture.session, 106),
            DraftMarkerAdmissionCommandIdV1::from_bytes([107; 16]),
            ordinal,
            true,
            Box::new([disagreeing]),
            Some(fixture.factory()),
        )
        .unwrap();
    assert!(
        fixture
            .store
            .compose_proof(disagreeing_attempt.take_command().unwrap())
            .is_err()
    );

    let mut stale = fixture
        .storage
        .prepare_draft_marker_label_readiness_page_for_test(
            &fixture.store,
            readiness_owner(&fixture.session, 108),
            DraftMarkerAdmissionCommandIdV1::from_bytes([109; 16]),
            ordinal,
            true,
            Box::new([fixture.association(110, fixture.thread)]),
            Some(fixture.factory()),
        )
        .unwrap();
    advance_asset_revision(&fixture, 111);
    assert!(
        fixture
            .store
            .compose_proof(stale.take_command().unwrap())
            .is_err()
    );

    let mut stale_source = fixture
        .storage
        .prepare_draft_marker_label_readiness_page_for_test(
            &fixture.store,
            readiness_owner(&fixture.session, 112),
            DraftMarkerAdmissionCommandIdV1::from_bytes([113; 16]),
            ordinal,
            true,
            Box::new([fixture.association(114, fixture.thread)]),
            Some(fixture.factory()),
        )
        .unwrap();
    advance_syndic_revision(&fixture);
    assert!(
        fixture
            .store
            .compose_proof(stale_source.take_command().unwrap())
            .is_err()
    );
}

#[test]
fn accepted_attempt_receipt_and_consumer_substitution_is_rejected() {
    let fixture = AcceptedFixture::new("phase217-substitution", 5);
    let ordinal = NonZeroU64::MIN;
    let mut first = fixture
        .storage
        .prepare_draft_marker_label_readiness_page_for_test(
            &fixture.store,
            readiness_owner(&fixture.session, 112),
            DraftMarkerAdmissionCommandIdV1::from_bytes([113; 16]),
            ordinal,
            true,
            Box::new([fixture.association(114, fixture.thread)]),
            Some(fixture.factory()),
        )
        .unwrap();
    let mut second = fixture
        .storage
        .prepare_draft_marker_label_readiness_page_for_test(
            &fixture.store,
            readiness_owner(&fixture.session, 117),
            DraftMarkerAdmissionCommandIdV1::from_bytes([115; 16]),
            ordinal,
            true,
            Box::new([fixture.association(116, fixture.thread)]),
            Some(fixture.factory()),
        )
        .unwrap();
    let _first_command = first.take_command().unwrap();
    let second_receipt = fixture
        .store
        .compose_proof(second.take_command().unwrap())
        .unwrap();
    drop(second);
    assert!(matches!(
        first.consume(&fixture.store, second_receipt),
        Err(DraftMarkerReadinessSourceErrorV1::Receipt(_))
    ));
}

#![cfg(feature = "test-faults")]

include!("durable_builder/support.rs");

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
    DraftMarkerAdmissionOwnerV1, DraftMarkerLabelReadinessDispositionV1,
    DraftMarkerReadinessAcceptedSourceV1, DraftMarkerReadinessCandidateSourceV1,
    DraftMarkerReadinessSourceAssociationV1, DraftMarkerReadinessSourceErrorV1,
    DraftMarkerReadinessSourceSelectorV1, DraftMarkerReadinessWitnessFactoryV1,
    ImageLabelAuthorityHeadV1, ImageLabelFrontier, ImageLabelOriginOwner,
    ImageLabelOriginSpanRecord, PreparedContent, SelectedPathProof, ThreadLineageDepth,
    ThreadLineageProof, ThreadRecord, child_thread_lineage_digest, empty_selected_path_digest,
};

#[path = "draft_marker_writer_admission/support.rs"]
mod writer_support;

#[path = "draft_marker_readiness_accepted_proof/support.rs"]
mod accepted_support;
#[path = "support/mod.rs"]
mod support;

use accepted_support::*;

use syndic_storage::{
    DraftMarkerLabelAssignmentOutcomeV1, DraftMarkerLabelReadinessDispositionV1 as Disposition,
    DraftMarkerLabelReadinessPageRequestV1, DraftMarkerLabelReadinessPageSubmissionOutcomeV1,
    DraftMarkerLabelReadinessProofV1,
};

#[path = "draft_marker_fresh_readiness/support.rs"]
mod fresh_support;
#[path = "draft_marker_fresh_readiness/lifecycle.rs"]
mod lifecycle;
#[path = "draft_marker_fresh_readiness/resolution.rs"]
mod resolution;
#[path = "draft_marker_fresh_readiness/writer_cleanup.rs"]
mod writer_cleanup;

use fresh_support::*;
#[test]
fn fresh_raw_vector_preserves_duplicate_occurrences_and_lexical_byte_order() {
    let fixture = AcceptedFixture::new("fresh-vector", 110);
    let other = publish_metadata(&fixture.store, &fixture.state, &[4; 256]);
    let operation = owner(&fixture.session, 155);
    let mut attempt = fixture
        .storage
        .prepare_draft_marker_label_readiness_page(
            &fixture.store,
            request(
                operation,
                156,
                1,
                true,
                Disposition::Allocate,
                vec![
                    fresh(157, other),
                    fresh(158, fixture.asset_id),
                    fresh(159, other),
                ],
                Some(fresh_factory(&fixture)),
            ),
        )
        .unwrap();
    let raw = |asset: AssetId| {
        let mut bytes = vec![2, 1];
        bytes.extend_from_slice(&asset.digest());
        bytes.extend_from_slice(&asset.length().get().to_le_bytes());
        assert_eq!(bytes.len(), 42);
        bytes
    };
    let mut entries = vec![raw(other), raw(fixture.asset_id), raw(other)];
    entries.sort();
    assert_eq!(
        attempt.expected_source_correlation_for_test(),
        manual_correlation(NonZeroU64::MIN, true, &entries)
    );
    let receipt = fixture
        .store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    attempt.consume(&fixture.store, receipt).unwrap();
}

#[test]
fn mixed_groups_preserve_existing_labels_and_allocate_once_per_foreign_or_fresh_group() {
    let fixture = AcceptedFixture::new("fresh-mixed", 120);
    let operation = owner(&fixture.session, 170);
    ingest(
        &fixture,
        operation,
        171,
        1,
        false,
        vec![fixture.association(180, fixture.thread)],
        || Some(fixture.factory()),
    );
    ingest(
        &fixture,
        operation,
        172,
        2,
        false,
        vec![fresh(181, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    ingest(
        &fixture,
        operation,
        173,
        3,
        false,
        vec![
            fixture.association(182, fixture.child),
            fixture.association(183, fixture.child),
        ],
        || Some(fixture.factory()),
    );
    ingest(
        &fixture,
        operation,
        174,
        4,
        false,
        vec![fresh(184, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    let before = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    let before = before.head().unwrap();
    assert_eq!(before.allocating_occurrence_count(), 4);
    assert_eq!(before.occurrence_count(), 5);
    ingest(&fixture, operation, 175, 5, true, vec![], || None);
    let frozen = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    let frozen = frozen.head().unwrap();
    assert_eq!(frozen.source_root(), before.source_root());
    assert_eq!(frozen.target_root(), before.target_root());
    assert_eq!(frozen.allocating_occurrence_count(), 4);
    assert_eq!(frozen.occurrence_count(), 5);
    let before_replay = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    let home_revision = fixture.store.home_revision().unwrap();
    for _ in 0..2 {
        let mut attempt = fixture
            .storage
            .prepare_draft_marker_label_readiness_page(
                &fixture.store,
                request(operation, 175, 5, true, Disposition::Allocate, vec![], None),
            )
            .unwrap();
        let receipt = fixture
            .store
            .compose_proof(attempt.take_command().unwrap())
            .unwrap();
        let flight = attempt
            .into_submission_flight(&fixture.store, receipt)
            .unwrap();
        assert!(matches!(
            fixture
                .storage
                .submit_draft_marker_label_readiness_page(&fixture.store, flight),
            DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Replayed
        ));
    }
    let after_replay = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    assert_eq!(fixture.store.home_revision().unwrap(), home_revision);
    assert_eq!(before_replay.head(), after_replay.head());
    assert_eq!(before_replay.receipt(), after_replay.receipt());
    assert_eq!(before_replay.capacity(), after_replay.capacity());
    let mut changed = fixture
        .storage
        .prepare_draft_marker_label_readiness_page(
            &fixture.store,
            request(
                operation,
                175,
                5,
                true,
                Disposition::Allocate,
                vec![fresh(185, fixture.asset_id)],
                Some(fresh_factory(&fixture)),
            ),
        )
        .unwrap();
    let receipt = fixture
        .store
        .compose_proof(changed.take_command().unwrap())
        .unwrap();
    let flight = changed
        .into_submission_flight(&fixture.store, receipt)
        .unwrap();
    assert!(matches!(
        fixture
            .storage
            .submit_draft_marker_label_readiness_page(&fixture.store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Collision
    ));
    let rejected = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    assert_eq!(before_replay.head(), rejected.head());
    assert_eq!(before_replay.receipt(), rejected.receipt());
    assert_eq!(before_replay.capacity(), rejected.capacity());
    let proof = assign(&fixture, operation, 190, 5);
    let range = proof.allocation_range().unwrap();
    assert_eq!(range.count(), 4);
    assert!(range.first().get() > fixture.label.get());
    assert_eq!(label(&fixture, &proof, 180), fixture.label);
    assert_eq!(label(&fixture, &proof, 182), range.first());
    assert_eq!(label(&fixture, &proof, 183), range.first());
    assert_eq!(
        label(&fixture, &proof, 181),
        range.first().checked_next().unwrap()
    );
    assert_eq!(label(&fixture, &proof, 184), label(&fixture, &proof, 181));
    let ready = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    assert_eq!(ready.head().unwrap().allocating_occurrence_count(), 4);
    assert_eq!(ready.head().unwrap().source_root().count(), 0);
}

#[test]
fn fresh_and_foreign_reuse_missing_or_wrong_shape_witnesses_are_rejected() {
    let fixture = AcceptedFixture::new("fresh-reject", 130);
    let operation = owner(&fixture.session, 180);
    for (disposition, entries, factory) in [
        (
            Disposition::Reuse,
            vec![fresh(181, fixture.asset_id)],
            Some(fresh_factory(&fixture)),
        ),
        (
            Disposition::Reuse,
            vec![fixture.association(181, fixture.child)],
            Some(fixture.factory()),
        ),
        (
            Disposition::Allocate,
            vec![fresh(181, fixture.asset_id)],
            None,
        ),
        (
            Disposition::Allocate,
            vec![fresh(181, fixture.asset_id)],
            Some(fixture.factory()),
        ),
        (
            Disposition::Allocate,
            vec![fixture.association(181, fixture.thread)],
            Some(fresh_factory(&fixture)),
        ),
        (
            Disposition::Allocate,
            vec![
                fresh(181, fixture.asset_id),
                fixture.association(182, fixture.thread),
            ],
            Some(fresh_factory(&fixture)),
        ),
        (
            Disposition::Allocate,
            vec![fresh(181, fixture.asset_id), fresh(181, fixture.asset_id)],
            Some(fresh_factory(&fixture)),
        ),
    ] {
        assert!(
            fixture
                .storage
                .prepare_draft_marker_label_readiness_page(
                    &fixture.store,
                    request(operation, 183, 1, true, disposition, entries, factory)
                )
                .is_err()
        );
    }
}

#[test]
fn fresh_witness_substitution_and_missing_metadata_cannot_prove_a_page() {
    let fixture = AcceptedFixture::new("fresh-substitution", 140);
    let operation = owner(&fixture.session, 185);
    let unknown = AssetId::sha256_v1([99; 32], NonZeroU64::MIN);
    let result = fixture.storage.prepare_draft_marker_label_readiness_page(
        &fixture.store,
        request(
            operation,
            186,
            1,
            true,
            Disposition::Allocate,
            vec![fresh(187, unknown)],
            Some(fresh_factory(&fixture)),
        ),
    );
    if let Ok(mut attempt) = result {
        assert!(
            fixture
                .store
                .compose_proof(attempt.take_command().unwrap())
                .is_err()
        );
    }
    let factory = fixture
        .state
        .assets()
        .draft_marker_fresh_asset_readiness_witness_factory();
    let substitute = publish_metadata(&fixture.store, &fixture.state, &[42; 10]);
    let wrong = DraftMarkerReadinessWitnessFactoryV1::fresh(move |store, ordinal, eof, _| {
        factory(store, ordinal, eof, vec![substitute])
    });
    let mut attempt = fixture
        .storage
        .prepare_draft_marker_label_readiness_page(
            &fixture.store,
            request(
                operation,
                188,
                1,
                true,
                Disposition::Allocate,
                vec![fresh(189, fixture.asset_id)],
                Some(wrong),
            ),
        )
        .unwrap();
    assert!(
        fixture
            .store
            .compose_proof(attempt.take_command().unwrap())
            .is_err()
    );
}

#[test]
fn zero_allocation_empty_eof_requires_no_range_even_at_label_exhaustion() {
    let fixture = AcceptedFixture::new("fresh-zero-allocation", 150);
    fixture
        .storage
        .seed_draft_marker_label_allocation_frontier_for_test(
            &fixture.store,
            fixture.thread,
            ImageLabelOrdinal::new(u64::MAX).unwrap(),
        )
        .unwrap();
    let operation = owner(&fixture.session, 200);
    ingest(
        &fixture,
        operation,
        201,
        1,
        false,
        vec![fixture.association(202, fixture.thread)],
        || Some(fixture.factory()),
    );
    ingest(&fixture, operation, 203, 2, true, vec![], || None);
    let proof = assign(&fixture, operation, 204, 1);
    assert!(proof.allocation_range().is_none());
    assert_eq!(label(&fixture, &proof, 202), fixture.label);
}

#[test]
fn concurrent_fresh_operations_reserve_separate_ranges_without_global_asset_deduplication() {
    let fixture = AcceptedFixture::new("fresh-ranges", 160);
    let first = owner(&fixture.session, 210);
    let second = owner(&fixture.session, 211);
    ingest(
        &fixture,
        first,
        212,
        1,
        true,
        vec![fresh(213, fixture.asset_id), fresh(214, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    ingest(
        &fixture,
        second,
        215,
        1,
        true,
        vec![fresh(216, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    let first = assign(&fixture, first, 217, 2);
    let second = assign(&fixture, second, 219, 1);
    let first_range = first.allocation_range().unwrap();
    let second_range = second.allocation_range().unwrap();
    assert_eq!(first_range.count(), 2);
    assert_eq!(second_range.count(), 1);
    assert!(first_range.last() < second_range.first());
    assert_eq!(label(&fixture, &first, 213), label(&fixture, &first, 214));
    assert_ne!(label(&fixture, &first, 213), label(&fixture, &second, 216));
}

#[test]
fn equal_foreign_labels_from_distinct_source_threads_receive_distinct_assignments() {
    let fixture = AcceptedFixture::new("foreign-thread-groups", 165);
    let other = publish_inheriting_child(
        &fixture.store,
        fixture.storage.clone(),
        fixture.thread,
        fixture.label,
        166,
    );
    let operation = owner(&fixture.session, 215);
    ingest(
        &fixture,
        operation,
        216,
        1,
        true,
        vec![
            fixture.association(217, fixture.child),
            fixture.association(218, other),
        ],
        || Some(fixture.factory()),
    );
    let proof = assign(&fixture, operation, 219, 2);
    assert_eq!(proof.allocation_range().unwrap().count(), 2);
    assert_ne!(label(&fixture, &proof, 217), label(&fixture, &proof, 218));
}

#[test]
fn fresh_exhaustion_at_separate_eof_preserves_the_ingested_prefix() {
    let fixture = AcceptedFixture::new("fresh-exhaustion", 170);
    let operation = owner(&fixture.session, 220);
    ingest(
        &fixture,
        operation,
        221,
        1,
        false,
        vec![fresh(222, fixture.asset_id)],
        || Some(fresh_factory(&fixture)),
    );
    let before = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    fixture
        .storage
        .seed_draft_marker_label_allocation_frontier_for_test(
            &fixture.store,
            fixture.thread,
            ImageLabelOrdinal::new(u64::MAX).unwrap(),
        )
        .unwrap();
    assert!(
        fixture
            .storage
            .prepare_draft_marker_label_readiness_page(
                &fixture.store,
                request(operation, 223, 2, true, Disposition::Allocate, vec![], None)
            )
            .is_err()
    );
    let after = fixture
        .storage
        .draft_marker_admission_publication_snapshot_for_test(&fixture.store, operation, &[])
        .unwrap();
    assert_eq!(before.head().unwrap(), after.head().unwrap());
    assert!(!after.head().unwrap().evidence_eof());
}

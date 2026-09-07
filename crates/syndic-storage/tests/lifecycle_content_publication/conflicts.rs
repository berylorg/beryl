use beryl_home_store::{CursorReadLimits, HomeHealthState};
use beryl_model::{ContentRevision, SyndicItemId};
use syndic_storage::{
    ContentAppend, ContentByteSpanRecord, ContentChunkOrdinal, ContentChunkRecord, ContentEncoding,
    ContentLifecycle, ContentManifestRecord, ContentPieceOrdinal, ContentPieceRecord,
    ContentTextSpanRecord, SyndicMutationError, SyndicPointReadLimit,
    prepare_lifecycle_continuation_content,
    test_faults::{FixtureRecord, lifecycle_content_manifest_with_owner},
};

use super::content_support::*;

fn reject_preserving(store: &beryl_home_store::HomeStore, storage: &syndic_storage::SyndicStorage) {
    let before = snapshot(store, storage);
    let home_revision = store.home_revision().unwrap();
    let domain_revision = storage.revision(store).unwrap();
    let error = rejected_error(
        store.execute_current(storage.current_publish_lifecycle_continuation_content()),
    );
    assert!(
        matches!(
            typed_error(&error),
            SyndicMutationError::ContentManifestConflict
                | SyndicMutationError::ContentChunkConflict
        ),
        "unexpected rejection: {error:?}"
    );
    assert_eq!(snapshot(store, storage), before);
    assert_eq!(store.home_revision().unwrap(), home_revision);
    assert_eq!(storage.revision(store).unwrap(), domain_revision);
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
    assert!(store.pending_reconciliations().is_empty());
}

#[test]
fn missing_manifest_or_any_child_rejects_without_repair() {
    for missing in 0..5 {
        let (_home, store, storage) = fixture(&format!("missing-{missing}"));
        let mut records = exact_records(ContentRevision::new(1).unwrap());
        records.remove(missing);
        seed(&store, &storage, records);
        reject_preserving(&store, &storage);
        store.close().unwrap();
    }
}

#[test]
fn any_orphan_child_prevents_fresh_publication() {
    for orphan in 1..5 {
        let (_home, store, storage) = fixture(&format!("orphan-{orphan}"));
        let records = exact_records(ContentRevision::new(1).unwrap());
        seed(&store, &storage, vec![records[orphan].clone()]);
        reject_preserving(&store, &storage);
        store.close().unwrap();
    }
}

#[test]
fn ownerless_building_objects_remain_unreadable_and_unchanged() {
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    for complete in [false, true] {
        let (_home, store, storage) = fixture(&format!("building-{complete}"));
        let manifest = if complete {
            ContentAppend::prepare(&prepared.building_manifest(), &prepared)
                .unwrap()
                .unwrap()
                .next_manifest()
                .clone()
        } else {
            prepared.building_manifest()
        };
        let mut records = if complete {
            exact_records(ContentRevision::new(1).unwrap())
        } else {
            vec![FixtureRecord::ContentManifest(manifest.clone())]
        };
        records[0] = FixtureRecord::ContentManifest(manifest);
        seed(&store, &storage, records);
        for _ in 0..2 {
            assert!(
                storage
                    .content_manifest(
                        &store,
                        prepared.id(),
                        SyndicPointReadLimit::new(65_536).unwrap()
                    )
                    .is_err()
            );
            assert!(
                storage
                    .content_chunks(
                        &store,
                        prepared.id(),
                        None,
                        CursorReadLimits::new(2, 65_536).unwrap()
                    )
                    .is_err()
            );
            reject_preserving(&store, &storage);
        }
        store.close().unwrap();
    }
}

#[test]
fn conflicting_manifest_metadata_and_ownership_never_count_as_reuse() {
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    let revision = ContentRevision::new(1).unwrap();
    let canonical = prepared.sealed_manifest(revision);
    let alternative = syndic_storage::PreparedContent::utf8("different").unwrap();
    let manifests = [
        lifecycle_content_manifest_with_owner(&canonical, SyndicItemId::from_bytes([17; 16])),
        ContentManifestRecord::new(
            prepared.id(),
            revision,
            ContentEncoding::Utf8V1,
            ContentLifecycle::Sealed,
            1,
            canonical.encoded_bytes(),
            canonical.chain_digest(),
            canonical.expected(),
        ),
        ContentManifestRecord::new(
            prepared.id(),
            revision,
            canonical.encoding(),
            ContentLifecycle::Sealed,
            2,
            canonical.encoded_bytes(),
            canonical.chain_digest(),
            canonical.expected(),
        ),
        ContentManifestRecord::new(
            prepared.id(),
            revision,
            canonical.encoding(),
            ContentLifecycle::Sealed,
            1,
            canonical.encoded_bytes() + 1,
            canonical.chain_digest(),
            canonical.expected(),
        ),
        ContentManifestRecord::new(
            prepared.id(),
            revision,
            canonical.encoding(),
            ContentLifecycle::Sealed,
            1,
            canonical.encoded_bytes(),
            alternative.summary().digest(),
            canonical.expected(),
        ),
        ContentManifestRecord::new(
            prepared.id(),
            revision,
            canonical.encoding(),
            ContentLifecycle::Sealed,
            1,
            canonical.encoded_bytes(),
            canonical.chain_digest(),
            alternative.summary(),
        ),
    ];
    for (index, manifest) in manifests.into_iter().enumerate() {
        let (_home, store, storage) = fixture(&format!("metadata-{index}"));
        let mut records = exact_records(revision);
        records[0] = FixtureRecord::ContentManifest(manifest);
        seed(&store, &storage, records);
        reject_preserving(&store, &storage);
        store.close().unwrap();
    }
}

fn changed_span(ordinal: u64, start: u64) -> ContentTextSpanRecord {
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    let text = prepared.text_spans()[0];
    ContentTextSpanRecord::new(
        prepared.id(),
        ContentPieceOrdinal::new(ordinal).unwrap(),
        text.chunk_ordinal(),
        text.chunk_start(),
        start,
        start + text.len(),
        text.encoded_start(),
        text.encoded_end(),
        !text.break_before(),
        text.digest(),
    )
    .unwrap()
}

#[test]
fn each_conflicting_canonical_child_rejects_despite_an_exact_manifest() {
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    let chunk = &prepared.chunks()[0];
    let replacements = [
        FixtureRecord::ContentChunk(
            ContentChunkRecord::new(
                prepared.id(),
                chunk.ordinal(),
                b"conflicting bytes".as_slice(),
            )
            .unwrap(),
        ),
        FixtureRecord::ContentByteSpan(
            ContentByteSpanRecord::new(
                prepared.id(),
                chunk.ordinal(),
                0,
                prepared.summary().encoded_bytes(),
                [19; 32],
            )
            .unwrap(),
        ),
        FixtureRecord::ContentTextSpan(changed_span(1, 0)),
        FixtureRecord::ContentPiece(ContentPieceRecord::text(changed_span(1, 0))),
    ];
    for (index, replacement) in replacements.into_iter().enumerate() {
        let (_home, store, storage) = fixture(&format!("child-conflict-{index}"));
        let mut records = exact_records(ContentRevision::new(1).unwrap());
        records[index + 1] = replacement;
        seed(&store, &storage, records);
        reject_preserving(&store, &storage);
        store.close().unwrap();
    }
}

#[test]
fn extra_owner_records_reject_even_when_every_expected_record_matches() {
    let prepared = prepare_lifecycle_continuation_content().unwrap();
    let chunk = &prepared.chunks()[0];
    let extras = [
        FixtureRecord::ContentChunk(
            ContentChunkRecord::new(
                prepared.id(),
                ContentChunkOrdinal::new(2).unwrap(),
                b"extra".as_slice(),
            )
            .unwrap(),
        ),
        FixtureRecord::ContentByteSpan(ContentByteSpanRecord::for_chunk(chunk, 100).unwrap()),
        FixtureRecord::ContentTextSpan(changed_span(2, 100)),
        FixtureRecord::ContentPiece(ContentPieceRecord::text(changed_span(2, 100))),
    ];
    for (index, extra) in extras.into_iter().enumerate() {
        let (_home, store, storage) = fixture(&format!("extra-{index}"));
        let mut records = exact_records(ContentRevision::new(1).unwrap());
        records.push(extra);
        seed(&store, &storage, records);
        reject_preserving(&store, &storage);
        store.close().unwrap();
    }
}

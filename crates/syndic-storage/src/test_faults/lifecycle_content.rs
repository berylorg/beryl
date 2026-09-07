use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::SyndicItemId;

use crate::{codec::*, domain::SyndicDomain, *};

pub fn lifecycle_content_manifest_with_owner(
    manifest: &ContentManifestRecord,
    owner: SyndicItemId,
) -> ContentManifestRecord {
    ContentManifestRecord::with_owner(
        manifest.id(),
        Some(owner),
        manifest.revision(),
        manifest.encoding(),
        manifest.lifecycle(),
        manifest.chunk_count(),
        manifest.encoded_bytes(),
        manifest.chain_digest(),
        manifest.expected(),
    )
}

pub fn lifecycle_content_canonical_records(
    store: &HomeStore,
    storage: &SyndicStorage,
) -> Vec<(&'static str, Vec<u8>, Vec<u8>)> {
    let id = prepare_lifecycle_continuation_content().unwrap().id();
    let mut records = Vec::new();
    let manifest = storage
        .point::<ContentManifestsFamily>(store, id, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap();
    if let Some(manifest) = manifest {
        records.push((
            ContentManifestsFamily::NAME,
            ContentManifestsFamily::encode_key(&id).unwrap(),
            ContentManifestsFamily::encode_value(&manifest).unwrap(),
        ));
    }
    append::<ContentChunksFamily>(
        store,
        storage,
        CursorRange::closed(
            ContentChunkKey {
                owner: id,
                ordinal: ContentChunkOrdinal::FIRST,
            },
            ContentChunkKey {
                owner: id,
                ordinal: ContentChunkOrdinal::new(u64::MAX).unwrap(),
            },
        ),
        &mut records,
    );
    append::<ContentByteSpansFamily>(
        store,
        storage,
        CursorRange::closed(
            ContentByteSpanKey {
                owner: id,
                start: 0,
            },
            ContentByteSpanKey {
                owner: id,
                start: u64::MAX,
            },
        ),
        &mut records,
    );
    append::<ContentTextSpansFamily>(
        store,
        storage,
        CursorRange::closed(
            ContentTextSpanKey {
                owner: id,
                logical_start: 0,
            },
            ContentTextSpanKey {
                owner: id,
                logical_start: u64::MAX,
            },
        ),
        &mut records,
    );
    append::<ContentPiecesFamily>(
        store,
        storage,
        CursorRange::closed(
            ContentPieceKey {
                owner: id,
                ordinal: ContentPieceOrdinal::FIRST,
            },
            ContentPieceKey {
                owner: id,
                ordinal: ContentPieceOrdinal::new(u64::MAX).unwrap(),
            },
        ),
        &mut records,
    );
    records
}

fn append<F: Family>(
    store: &HomeStore,
    storage: &SyndicStorage,
    range: CursorRange<F::Key>,
    records: &mut Vec<(&'static str, Vec<u8>, Vec<u8>)>,
) {
    let page = store
        .read_cursor::<SyndicDomain, ExactCodec<F>>(
            &storage.handle,
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(16, 1_048_576).unwrap(),
        )
        .unwrap();
    assert!(!page.has_more(), "fixed-content fixture must stay bounded");
    for record in page.records() {
        records.push((
            F::NAME,
            F::encode_key(record.key()).unwrap(),
            F::encode_value(record.value()).unwrap(),
        ));
    }
}

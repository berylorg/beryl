#![cfg(feature = "test-faults")]

mod support;

use std::{sync::Arc, thread, time::Duration};

use beryl_home_store::{
    test_faults::{FaultController, FaultPoint},
    CursorReadLimits, HomeOpenOptions, HomeSchemaVersion, HomeStore,
};
use beryl_model::{ContentRevision, SyndicContentId, SyndicDraftMarkerId};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};
use syndic_storage::{
    ComposerAtom, ComposerPayload, ContentEncoding, ContentReference, ImageLabelOrdinal,
    PreparedContent, SyndicReadError, SyndicStorage,
};

use support::{batch, commit, open, prepared_content_records, TestHome};

struct Fixture {
    store: HomeStore,
    _home: TestHome,
    storage: SyndicStorage,
    content: ContentReference,
}

fn composer(text: &str) -> PreparedContent {
    PreparedContent::composer(
        &ComposerPayload::new(vec![ComposerAtom::text(text).unwrap()]).unwrap(),
    )
    .unwrap()
}

fn seed(name: &str, prepared: &PreparedContent) -> Fixture {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (content, records) = prepared_content_records(prepared);
    commit(&store, storage, batch(records));
    Fixture {
        store,
        _home: home,
        storage,
        content,
    }
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn marker_free_composer_pages_directly_into_one_utf8_string() {
    let source = "a💎é".repeat(20_000);
    let fixture = seed("phase13-content-text-pages", &composer(&source));
    assert!(fixture.content.summary().chunk_count() >= 3);

    let mut observed = String::new();
    let mut offset = 0_u64;
    loop {
        let page = fixture
            .storage
            .sealed_content_text_range(&fixture.store, fixture.content, offset, 4_093)
            .unwrap()
            .unwrap();
        assert_eq!(page.content(), fixture.content);
        assert_eq!(page.start(), offset);
        assert!(!page.text().is_empty());
        assert!(page.text().len() <= 4_093);
        assert!(page.stored_bytes() > page.text().len());
        assert!(page.stored_bytes() < fixture.content.summary().encoded_bytes() as usize);
        observed.push_str(page.text());
        match page.next_offset() {
            Some(next) => {
                assert_eq!(next, offset + page.text().len() as u64);
                assert!(source.is_char_boundary(next as usize));
                offset = next;
            }
            None => break,
        }
    }
    assert_eq!(observed, source);
}

#[test]
fn pages_trim_at_utf8_boundaries_and_reject_nonprogress() {
    let fixture = seed("phase13-content-text-utf8", &composer("a💎b"));
    let first = fixture
        .storage
        .sealed_content_text_range(&fixture.store, fixture.content, 0, 4)
        .unwrap()
        .unwrap();
    assert_eq!(first.text(), "a");
    assert_eq!(first.next_offset(), Some(1));

    let second = fixture
        .storage
        .sealed_content_text_range(&fixture.store, fixture.content, 1, 4)
        .unwrap()
        .unwrap();
    assert_eq!(second.text(), "💎");
    assert_eq!(second.next_offset(), Some(5));

    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_range(&fixture.store, fixture.content, 2, 4),
        Err(SyndicReadError::InvalidContentTextOffset {
            content_bytes: 6,
            offset: 2,
        })
    ));
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_range(&fixture.store, fixture.content, 1, 3),
        Err(SyndicReadError::ContentTextReadLimitTooSmall {
            offset: 1,
            actual: 3,
        })
    ));
}

#[test]
fn consuming_page_transfers_exact_bounded_utf8_without_copying() {
    let fixture = seed(
        "phase13-content-text-consuming-page",
        &composer("a\u{1f48e}\u{e9}z"),
    );
    let page = fixture
        .storage
        .sealed_content_text_range(&fixture.store, fixture.content, 1, 6)
        .unwrap()
        .unwrap();
    assert_eq!(page.text(), "\u{1f48e}\u{e9}");
    assert_eq!(page.text().len(), 6);
    assert_eq!(page.next_offset(), Some(7));

    let borrowed_pointer = page.text().as_ptr();
    let text = page.into_text();
    assert_eq!(&*text, "\u{1f48e}\u{e9}");
    assert_eq!(text.as_ptr(), borrowed_pointer);
}

#[test]
fn page_byte_counts_include_span_and_chunk_cursors() {
    let fixture = seed("phase13-content-text-accounting", &composer("accounted"));
    let spans = fixture
        .storage
        .content_text_spans(
            &fixture.store,
            fixture.content.id(),
            None,
            CursorReadLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    let chunks = fixture
        .storage
        .content_chunks(
            &fixture.store,
            fixture.content.id(),
            None,
            CursorReadLimits::new(256, 131_072).unwrap(),
        )
        .unwrap();
    let page = fixture
        .storage
        .sealed_content_text_range(&fixture.store, fixture.content, 0, 64)
        .unwrap()
        .unwrap();
    assert_eq!(
        page.stored_bytes(),
        spans.stored_bytes() + chunks.stored_bytes()
    );
    assert_eq!(
        page.decoded_bytes(),
        spans.decoded_bytes() + chunks.decoded_bytes()
    );
}

#[test]
fn empty_content_and_exact_end_return_one_terminal_empty_page() {
    let prepared = PreparedContent::composer(&ComposerPayload::default()).unwrap();
    let fixture = seed("phase13-content-text-empty", &prepared);
    let page = fixture
        .storage
        .sealed_content_text_range(&fixture.store, fixture.content, 0, 1)
        .unwrap()
        .unwrap();
    assert_eq!(page.text(), "");
    assert_eq!(page.next_offset(), None);
    assert_eq!(page.stored_bytes(), 0);
    assert_eq!(page.decoded_bytes(), 0);

    let fixture = seed("phase13-content-text-exact-end", &composer("done"));
    let page = fixture
        .storage
        .sealed_content_text_range(&fixture.store, fixture.content, 4, 1)
        .unwrap()
        .unwrap();
    assert_eq!(page.text(), "");
    assert_eq!(page.next_offset(), None);
}

#[test]
fn text_only_boundary_rejects_marker_bearing_content() {
    let payload = ComposerPayload::new(vec![
        ComposerAtom::text("before").unwrap(),
        ComposerAtom::image_marker(
            SyndicDraftMarkerId::from_bytes([7; 16]),
            ImageLabelOrdinal::FIRST,
        ),
        ComposerAtom::text("after").unwrap(),
    ])
    .unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    let fixture = seed("phase13-content-text-marker", &prepared);
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_range(&fixture.store, fixture.content, 0, 64),
        Err(SyndicReadError::ContentTextContainsImageMarkers { actual: 1 })
    ));
}

#[test]
fn missing_and_inexact_content_references_are_distinct() {
    let home = TestHome::new("phase13-content-text-missing");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let missing = composer("missing").reference(ContentRevision::new(1).unwrap());
    assert!(storage
        .sealed_content_text_range(&store, missing, 0, 64)
        .unwrap()
        .is_none());

    let prepared = composer("exact");
    let (content, records) = prepared_content_records(&prepared);
    commit(&store, storage, batch(records));
    let wrong_revision = ContentReference::new(
        content.id(),
        ContentRevision::new(2).unwrap(),
        content.encoding(),
        content.summary(),
    );
    assert!(matches!(
        storage.sealed_content_text_range(&store, wrong_revision, 0, 64),
        Err(SyndicReadError::Invariant(
            "sealed content reference disagrees with its exact manifest"
        ))
    ));
    let wrong_encoding = ContentReference::new(
        content.id(),
        content.revision(),
        ContentEncoding::Utf8V1,
        content.summary(),
    );
    assert!(matches!(
        storage.sealed_content_text_range(&store, wrong_encoding, 0, 64),
        Err(SyndicReadError::Invariant(
            "sealed content reference disagrees with its exact manifest"
        ))
    ));
}

#[test]
fn nonsealed_lifecycle_and_missing_chunk_reject_without_fallback() {
    let prepared = composer("bounded text");
    let fixture = seed("phase13-content-text-lifecycle", &prepared);
    let mut replace = FixtureBatch::new();
    replace
        .put(FixtureRecord::ContentManifest(prepared.building_manifest()))
        .unwrap();
    commit(&fixture.store, fixture.storage, replace);
    let building_read =
        fixture
            .storage
            .sealed_content_text_range(&fixture.store, fixture.content, 0, 64);
    assert!(
        matches!(
            building_read,
            Err(SyndicReadError::Invariant(
                "ownerless content is unavailable before seal"
            ))
        ),
        "unexpected building-content result: {building_read:?}"
    );

    let fixture = seed("phase13-content-text-missing-chunk", &prepared);
    let mut remove = FixtureBatch::new();
    remove
        .delete(FixtureDelete::ContentChunk {
            content: fixture.content.id(),
            ordinal: syndic_storage::ContentChunkOrdinal::FIRST,
        })
        .unwrap();
    commit(&fixture.store, fixture.storage, remove);
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_range(&fixture.store, fixture.content, 0, 64,),
        Err(SyndicReadError::Invariant(
            "sealed content text chunk is missing"
        ))
    ));
}

#[test]
fn payload_limit_and_out_of_range_offset_are_typed() {
    let fixture = seed("phase13-content-text-input-bounds", &composer("text"));
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_range(&fixture.store, fixture.content, 0, 0),
        Err(SyndicReadError::InvalidContentTextReadLimit {
            maximum: 65_536,
            actual: 0,
        })
    ));
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_range(&fixture.store, fixture.content, 0, 65_537,),
        Err(SyndicReadError::InvalidContentTextReadLimit { .. })
    ));
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_range(&fixture.store, fixture.content, 5, 1),
        Err(SyndicReadError::InvalidContentTextOffset {
            content_bytes: 4,
            offset: 5,
        })
    ));
}

#[test]
fn manifest_change_during_page_assembly_is_concurrent_state() {
    let home = TestHome::new("phase13-content-text-concurrent");
    let faults = FaultController::new();
    let mut store = open_with_faults(home.path(), faults.clone());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let prepared = composer(&"concurrent".repeat(10_000));
    let (content, records) = prepared_content_records(&prepared);
    commit(&store, storage, batch(records));

    let block = faults.block_next(FaultPoint::BeforeReadConfirmation);
    let store = Arc::new(store);
    let reader_store = Arc::clone(&store);
    let reader =
        thread::spawn(move || storage.sealed_content_text_range(&reader_store, content, 0, 4_096));
    assert!(block.wait_until_reached(Duration::from_secs(10)));

    let replacement = syndic_storage::ContentManifestRecord::new(
        content.id(),
        ContentRevision::new(2).unwrap(),
        content.encoding(),
        syndic_storage::ContentLifecycle::Sealed,
        content.summary().chunk_count(),
        content.summary().encoded_bytes(),
        content.summary().digest(),
        content.summary(),
    );
    let mut mutation = FixtureBatch::new();
    mutation
        .put(FixtureRecord::ContentManifest(replacement))
        .unwrap();
    commit(&store, storage, mutation);
    block.release();

    assert!(matches!(
        reader.join().unwrap(),
        Err(SyndicReadError::ConcurrentChange {
            operation: "sealed-content text-range read"
        })
    ));
    let store = Arc::try_unwrap(store).unwrap_or_else(|_| panic!("reader retained the home"));
    drop(store);
    drop(home);
}

#[test]
fn sealed_manifest_identity_must_remain_content_addressed() {
    let home = TestHome::new("phase13-content-text-corrupt-manifest");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let prepared = PreparedContent::utf8("corrupt identity").unwrap();
    let original = prepared.reference(ContentRevision::new(1).unwrap());
    let corrupt_id = SyndicContentId::from_bytes([0x44; 16]);
    let corrupt = syndic_storage::ContentManifestRecord::new(
        corrupt_id,
        original.revision(),
        original.encoding(),
        syndic_storage::ContentLifecycle::Sealed,
        original.summary().chunk_count(),
        original.summary().encoded_bytes(),
        original.summary().digest(),
        original.summary(),
    );
    let mut records = FixtureBatch::new();
    records
        .put(FixtureRecord::ContentManifest(corrupt))
        .unwrap();
    commit(&store, storage, records);
    let corrupt_reference = ContentReference::new(
        corrupt_id,
        original.revision(),
        original.encoding(),
        original.summary(),
    );
    let corrupt_read = storage.sealed_content_text_range(&store, corrupt_reference, 0, 64);
    assert!(
        matches!(
            corrupt_read,
            Err(SyndicReadError::Invariant(
                "sealed content reference disagrees with its exact manifest"
            ))
        ),
        "unexpected corrupt-manifest result: {corrupt_read:?}"
    );
}

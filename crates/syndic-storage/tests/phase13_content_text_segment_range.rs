#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::HomeStore;
use beryl_model::{ContentRevision, SyndicDraftMarkerId};
use syndic_storage::{
    ComposerAtom, ComposerPayload, ContentEncoding, ContentReference, ImageLabelOrdinal,
    PreparedContent, SyndicContentTextSegment, SyndicContentTextSegmentBoundary, SyndicReadError,
    SyndicStorage,
};

use support::{TestHome, batch, commit, open, prepared_content_records};

#[path = "phase13_content_text_segment_range/corruption.rs"]
mod corruption;

struct Fixture {
    store: HomeStore,
    _home: TestHome,
    storage: SyndicStorage,
    content: ContentReference,
}

fn text(value: &str) -> ComposerAtom {
    ComposerAtom::text(value).unwrap()
}

fn marker(seed: u8, label: u64) -> ComposerAtom {
    ComposerAtom::image_marker(
        SyndicDraftMarkerId::from_bytes([seed; 16]),
        ImageLabelOrdinal::new(label).unwrap(),
    )
}

fn prepared(atoms: Vec<ComposerAtom>) -> PreparedContent {
    PreparedContent::composer(&ComposerPayload::new(atoms).unwrap()).unwrap()
}

fn seed(name: &str, prepared: &PreparedContent) -> Fixture {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (content, records) = prepared_content_records(prepared);
    commit(&store, storage.clone(), batch(records));
    Fixture {
        store,
        _home: home,
        storage,
        content,
    }
}

fn prove(
    fixture: &Fixture,
    after_marker: Option<SyndicContentTextSegmentBoundary>,
) -> SyndicContentTextSegment {
    fixture
        .storage
        .prove_sealed_content_text_segment(&fixture.store, fixture.content, after_marker)
        .unwrap()
        .unwrap()
}

fn collect(fixture: &Fixture, segment: &SyndicContentTextSegment, maximum: usize) -> String {
    let mut result = String::new();
    let mut offset = segment.start();
    loop {
        let page = fixture
            .storage
            .sealed_content_text_segment_range(&fixture.store, segment, offset, maximum)
            .unwrap()
            .unwrap();
        assert_eq!(page.content(), fixture.content);
        assert_eq!(page.segment_start(), segment.start());
        assert_eq!(page.segment_end(), segment.end());
        assert_eq!(page.start(), offset);
        assert!(page.text().len() <= maximum);
        if page.text().is_empty() {
            assert_eq!(page.stored_bytes(), 0);
            assert_eq!(page.decoded_bytes(), 0);
        } else {
            assert!(page.stored_bytes() > 0);
            assert!(page.decoded_bytes() > 0);
        }
        result.push_str(page.text());
        match page.next_offset() {
            Some(next) => {
                assert_eq!(next, offset + page.text().len() as u64);
                assert!(next > offset);
                assert!(next < segment.end());
                offset = next;
            }
            None => {
                assert_eq!(offset + page.text().len() as u64, segment.end());
                return result;
            }
        }
    }
}

#[test]
fn proves_and_reads_text_on_both_sides_of_one_marker() {
    let fixture = seed(
        "phase13-content-segments-sides",
        &prepared(vec![text("before"), marker(1, 1), text("after")]),
    );

    let before = prove(&fixture, None);
    assert_eq!((before.start(), before.end()), (0, 6));
    assert_eq!(before.preceding_marker(), None);
    let boundary = before.following_marker().unwrap();
    assert_eq!(boundary.marker_ordinal().get(), 1);
    assert_eq!(boundary.logical_offset(), 6);
    assert_eq!(
        boundary.marker_id(),
        SyndicDraftMarkerId::from_bytes([1; 16])
    );
    assert_eq!(boundary.label(), ImageLabelOrdinal::new(1).unwrap());
    assert_eq!(collect(&fixture, &before, 2), "before");

    let after = prove(&fixture, Some(boundary));
    assert_eq!((after.start(), after.end()), (6, 11));
    assert_eq!(after.preceding_marker(), Some(boundary));
    assert_eq!(after.following_marker(), None);
    assert_eq!(collect(&fixture, &after, 2), "after");

    let suffix = fixture
        .storage
        .sealed_content_text_segment_range(&fixture.store, &after, 8, 64)
        .unwrap()
        .unwrap();
    assert_eq!(suffix.text(), "ter");
    assert_eq!(suffix.next_offset(), None);
    let terminal = fixture
        .storage
        .sealed_content_text_segment_range(&fixture.store, &after, 11, 1)
        .unwrap()
        .unwrap();
    assert_eq!(terminal.text(), "");
    assert_eq!(terminal.next_offset(), None);
}

#[test]
fn exact_boundary_cursors_preserve_leading_adjacent_and_trailing_empty_segments() {
    let fixture = seed(
        "phase13-content-segments-empty",
        &prepared(vec![
            marker(1, 1),
            text("mid"),
            marker(2, 2),
            marker(3, 3),
            text("tail"),
            marker(4, 4),
        ]),
    );

    let expected = [
        (0, 0, ""),
        (0, 3, "mid"),
        (3, 3, ""),
        (3, 7, "tail"),
        (7, 7, ""),
    ];
    let mut after = None;
    for (index, (start, end, text)) in expected.into_iter().enumerate() {
        let segment = prove(&fixture, after);
        assert_eq!((segment.start(), segment.end()), (start, end));
        assert_eq!(collect(&fixture, &segment, 2), text);
        assert_eq!(
            segment
                .preceding_marker()
                .map(|marker| marker.marker_ordinal().get()),
            (index != 0).then_some(index as u64),
        );
        assert_eq!(
            segment
                .following_marker()
                .map(|marker| marker.marker_ordinal().get()),
            (index < 4).then_some(index as u64 + 1),
        );
        after = segment.following_marker();
    }
}

#[test]
fn marker_only_content_has_one_distinct_proof_per_boundary_interval() {
    let fixture = seed(
        "phase13-content-segments-marker-only",
        &prepared(vec![marker(1, 1), marker(2, 2), marker(3, 3)]),
    );
    let mut after = None;
    for index in 0..=3 {
        let segment = prove(&fixture, after);
        assert_eq!((segment.start(), segment.end()), (0, 0));
        assert_eq!(
            segment
                .preceding_marker()
                .map(|marker| marker.marker_ordinal().get()),
            (index != 0).then_some(index),
        );
        assert_eq!(
            segment
                .following_marker()
                .map(|marker| marker.marker_ordinal().get()),
            (index < 3).then_some(index + 1),
        );
        after = segment.following_marker();
    }
}

#[test]
fn unicode_pages_trim_safely_and_honor_absolute_starts() {
    let first = "a\u{1f48e}\u{e9}";
    let second = "\u{3b2}\u{7d42}";
    let fixture = seed(
        "phase13-content-segments-unicode",
        &prepared(vec![text(first), marker(7, 1), text(second)]),
    );
    let leading = prove(&fixture, None);
    let trailing = prove(&fixture, leading.following_marker());

    let page = fixture
        .storage
        .sealed_content_text_segment_range(&fixture.store, &leading, 0, 4)
        .unwrap()
        .unwrap();
    assert_eq!(page.text(), "a");
    assert_eq!(page.next_offset(), Some(1));
    assert_eq!(collect(&fixture, &leading, 4), first);

    let suffix = fixture
        .storage
        .sealed_content_text_segment_range(&fixture.store, &trailing, 9, 8)
        .unwrap()
        .unwrap();
    assert_eq!(suffix.text(), "\u{7d42}");
    assert_eq!(suffix.next_offset(), None);
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_segment_range(&fixture.store, &leading, 2, 4,),
        Err(SyndicReadError::InvalidContentTextOffset {
            content_bytes: 12,
            offset: 2,
        })
    ));
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_segment_range(&fixture.store, &leading, 1, 3,),
        Err(SyndicReadError::ContentTextReadLimitTooSmall {
            offset: 1,
            actual: 3,
        })
    ));
}

#[test]
fn exact_page_boundaries_have_one_absolute_continuation() {
    let source = "x".repeat(8_192);
    let fixture = seed(
        "phase13-content-segments-page-boundary",
        &prepared(vec![text(&source), marker(1, 1), text("next")]),
    );
    let segment = prove(&fixture, None);
    let first = fixture
        .storage
        .sealed_content_text_segment_range(&fixture.store, &segment, 0, 4_096)
        .unwrap()
        .unwrap();
    assert_eq!(first.text().len(), 4_096);
    assert_eq!(first.next_offset(), Some(4_096));
    let second = fixture
        .storage
        .sealed_content_text_segment_range(&fixture.store, &segment, 4_096, 4_096)
        .unwrap()
        .unwrap();
    assert_eq!(second.text().len(), 4_096);
    assert_eq!(second.next_offset(), None);
}

#[test]
fn cursor_offsets_limits_references_and_absence_are_typed() {
    let source = prepared(vec![text("left"), marker(1, 1), text("right")]);
    let fixture = seed("phase13-content-segments-invalid", &source);
    let leading = prove(&fixture, None);
    let first_piece_donor = seed(
        "phase13-content-segments-invalid-first-donor",
        &prepared(vec![marker(7, 7)]),
    );
    let first_piece_marker = prove(&first_piece_donor, None).following_marker().unwrap();
    assert!(matches!(
        fixture.storage.prove_sealed_content_text_segment(
            &fixture.store,
            fixture.content,
            Some(first_piece_marker),
        ),
        Err(SyndicReadError::InvalidContentTextSegmentCursor {
            piece_count: 3,
            after_piece: 1,
        })
    ));
    let fourth_piece_donor = seed(
        "phase13-content-segments-invalid-fourth-donor",
        &prepared(vec![text("a"), marker(8, 8), text("b"), marker(9, 9)]),
    );
    let first = prove(&fourth_piece_donor, None);
    let fourth_piece_marker = prove(&fourth_piece_donor, first.following_marker())
        .following_marker()
        .unwrap();
    assert!(matches!(
        fixture.storage.prove_sealed_content_text_segment(
            &fixture.store,
            fixture.content,
            Some(fourth_piece_marker),
        ),
        Err(SyndicReadError::InvalidContentTextSegmentCursor {
            piece_count: 3,
            after_piece: 4,
        })
    ));
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_segment_range(&fixture.store, &leading, 5, 64,),
        Err(SyndicReadError::InvalidContentTextSegmentOffset {
            segment_start: 0,
            segment_end: 4,
            offset: 5,
        })
    ));
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_segment_range(&fixture.store, &leading, 0, 0,),
        Err(SyndicReadError::InvalidContentTextReadLimit {
            maximum: 65_536,
            actual: 0,
        })
    ));
    assert!(matches!(
        fixture
            .storage
            .sealed_content_text_segment_range(&fixture.store, &leading, 0, 65_537,),
        Err(SyndicReadError::InvalidContentTextReadLimit { .. })
    ));

    let wrong_revision = ContentReference::new(
        fixture.content.id(),
        ContentRevision::new(2).unwrap(),
        fixture.content.encoding(),
        fixture.content.summary(),
    );
    assert!(matches!(
        fixture
            .storage
            .prove_sealed_content_text_segment(&fixture.store, wrong_revision, None,),
        Err(SyndicReadError::Invariant(
            "sealed content reference disagrees with its exact manifest"
        ))
    ));
    let wrong_encoding = ContentReference::new(
        fixture.content.id(),
        fixture.content.revision(),
        ContentEncoding::Utf8V1,
        fixture.content.summary(),
    );
    assert!(matches!(
        fixture
            .storage
            .prove_sealed_content_text_segment(&fixture.store, wrong_encoding, None,),
        Err(SyndicReadError::Invariant(
            "sealed content reference disagrees with its exact manifest"
        ))
    ));

    let missing = source.reference(ContentRevision::new(1).unwrap());
    let home = TestHome::new("phase13-content-segments-missing");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    assert!(
        storage
            .prove_sealed_content_text_segment(&store, missing, None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn proof_is_complete_before_terminal_or_valid_prefix_pages_are_authorized() {
    let prefix = "p".repeat(8_192);
    let fixture = seed(
        "phase13-content-segments-proof-before-pages",
        &prepared(vec![text(&prefix), marker(1, 1), text("tail")]),
    );
    let content_end = fixture.content.summary().logical_utf8_bytes();

    let proven_prefix = prove(&fixture, None);
    assert_eq!((proven_prefix.start(), proven_prefix.end()), (0, 8_192));
    assert!(proven_prefix.following_marker().is_some());
    assert!(matches!(
        fixture.storage.sealed_content_text_segment_range(
            &fixture.store,
            &proven_prefix,
            content_end,
            1,
        ),
        Err(SyndicReadError::InvalidContentTextSegmentOffset {
            segment_start: 0,
            segment_end: 8_192,
            offset: 8_196,
        })
    ));

    let first = fixture
        .storage
        .sealed_content_text_segment_range(&fixture.store, &proven_prefix, 0, 4_096)
        .unwrap()
        .unwrap();
    assert_eq!(first.text(), &prefix[..4_096]);
    assert_eq!(first.next_offset(), Some(4_096));
    let tail = prove(&fixture, proven_prefix.following_marker());
    assert_eq!(collect(&fixture, &tail, 1), "tail");
}

#[test]
fn large_proof_scans_once_while_later_pages_remain_payload_bounded() {
    let source = "0123456789abcdef".repeat(100_000);
    let source_bytes = source.len() as u64;
    let fixture = seed(
        "phase13-content-segments-bounded",
        &prepared(vec![text(&source), marker(1, 1), text("tail")]),
    );
    assert!(fixture.content.summary().chunk_count() > 20);

    let segment = prove(&fixture, None);
    assert_eq!((segment.start(), segment.end()), (0, source_bytes));
    let page = fixture
        .storage
        .sealed_content_text_segment_range(&fixture.store, &segment, 700_000, 4_096)
        .unwrap()
        .unwrap();
    assert_eq!(page.text(), &source[700_000..704_096]);
    assert_eq!(page.next_offset(), Some(704_096));
    assert!(page.stored_bytes() < 200_000);
    assert!(page.decoded_bytes() < 200_000);
    assert!(page.stored_bytes() < fixture.content.summary().encoded_bytes() as usize);
}

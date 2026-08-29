use beryl_model::SyndicDraftMarkerId;
use sha2::{Digest, Sha256};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete, FixtureRecord};
use syndic_storage::{
    ComposerAtomOrdinal, ContentPieceRecord, ContentTextSpanRecord, ImageLabelOrdinal,
    InputMarkerOrdinal, SyndicReadError,
};

use super::*;

#[test]
fn malformed_or_missing_text_index_evidence_rejects_before_proof() {
    let source = prepared(vec![text("left"), marker(1, 1), text("right")]);
    let after = source
        .text_spans()
        .iter()
        .copied()
        .find(|span| span.break_before())
        .unwrap();

    let fixture = seed("phase13-content-segments-corrupt-break", &source);
    let leading = prove(&fixture, None);
    let boundary = leading.following_marker().unwrap();
    let corrupt = ContentTextSpanRecord::new(
        after.content_id(),
        after.piece_ordinal(),
        after.chunk_ordinal(),
        after.chunk_start(),
        after.logical_start(),
        after.logical_end(),
        after.encoded_start(),
        after.encoded_end(),
        false,
        after.digest(),
    )
    .unwrap();
    let mut mutation = FixtureBatch::new();
    mutation
        .put(FixtureRecord::ContentTextSpan(corrupt))
        .unwrap();
    mutation
        .put(FixtureRecord::ContentPiece(ContentPieceRecord::text(
            corrupt,
        )))
        .unwrap();
    commit(&fixture.store, fixture.storage.clone(), mutation);
    assert!(matches!(
        fixture.storage.prove_sealed_content_text_segment(
            &fixture.store,
            fixture.content,
            Some(boundary),
        ),
        Err(SyndicReadError::Invariant(
            "content text segment text piece frontier disagrees"
        ))
    ));

    let fixture = seed("phase13-content-segments-missing-span", &source);
    let leading = prove(&fixture, None);
    let boundary = leading.following_marker().unwrap();
    let mut mutation = FixtureBatch::new();
    mutation
        .delete(FixtureDelete::ContentTextSpan {
            content: fixture.content.id(),
            logical_start: after.logical_start(),
        })
        .unwrap();
    commit(&fixture.store, fixture.storage.clone(), mutation);
    assert!(matches!(
        fixture.storage.prove_sealed_content_text_segment(
            &fixture.store,
            fixture.content,
            Some(boundary),
        ),
        Err(SyndicReadError::Invariant(
            "content text segment text piece index is missing"
        ))
    ));
}

#[test]
fn page_revalidates_the_physical_boundary_after_proof() {
    let source = prepared(vec![text("left"), marker(1, 1), text("right")]);
    let after = source
        .text_spans()
        .iter()
        .copied()
        .find(|span| span.break_before())
        .unwrap();
    let fixture = seed("phase13-content-segments-page-boundary-corrupt", &source);
    let leading = prove(&fixture, None);
    let segment = prove(&fixture, leading.following_marker());
    let corrupt = ContentTextSpanRecord::new(
        after.content_id(),
        after.piece_ordinal(),
        after.chunk_ordinal(),
        after.chunk_start(),
        after.logical_start(),
        after.logical_end(),
        after.encoded_start(),
        after.encoded_end(),
        false,
        after.digest(),
    )
    .unwrap();
    let mut mutation = FixtureBatch::new();
    mutation
        .put(FixtureRecord::ContentTextSpan(corrupt))
        .unwrap();
    commit(&fixture.store, fixture.storage.clone(), mutation);
    assert!(matches!(
        fixture.storage.sealed_content_text_segment_range(
            &fixture.store,
            &segment,
            segment.start(),
            64,
        ),
        Err(SyndicReadError::Invariant(
            "proven content text segment physical marker boundary disagrees"
        ))
    ));
}

#[test]
fn fake_marker_at_zero_is_rejected_against_encoded_content() {
    let source = prepared(vec![text(&"x".repeat(32)), marker(1, 1), text("tail")]);
    let span = source.text_spans()[0];
    let chunk = source
        .chunks()
        .iter()
        .find(|chunk| chunk.ordinal() == span.chunk_ordinal())
        .unwrap();
    let encoded_start = span.encoded_start();
    let encoded_end = encoded_start + 25;
    let local_start = usize::try_from(encoded_start - span.chunk_start()).unwrap();
    let local_end = usize::try_from(encoded_end - span.chunk_start()).unwrap();
    let digest: [u8; 32] = Sha256::digest(&chunk.bytes()[local_start..local_end]).into();
    let fake = ContentPieceRecord::image_marker(
        span.content_id(),
        span.piece_ordinal(),
        ComposerAtomOrdinal::FIRST,
        InputMarkerOrdinal::FIRST,
        0,
        encoded_start,
        encoded_end,
        SyndicDraftMarkerId::from_bytes([9; 16]),
        ImageLabelOrdinal::new(1).unwrap(),
        digest,
    )
    .unwrap();
    let fixture = seed("phase13-content-segments-fake-marker", &source);
    let mut mutation = FixtureBatch::new();
    mutation.put(FixtureRecord::ContentPiece(fake)).unwrap();
    commit(&fixture.store, fixture.storage.clone(), mutation);
    assert_marker_bytes_reject(&fixture);
}

#[test]
fn wrong_marker_digest_is_rejected_after_exact_byte_read() {
    let source = prepared(vec![marker(1, 1), text("tail")]);
    let marker = source.pieces()[0];
    let corrupt = rebuild_marker(marker, InputMarkerOrdinal::FIRST, [0; 32]);
    let fixture = seed("phase13-content-segments-marker-digest", &source);
    let mut mutation = FixtureBatch::new();
    mutation.put(FixtureRecord::ContentPiece(corrupt)).unwrap();
    commit(&fixture.store, fixture.storage.clone(), mutation);
    assert_marker_bytes_reject(&fixture);
}

#[test]
fn wrong_marker_ordinal_is_rejected_as_noncontiguous() {
    let source = prepared(vec![marker(1, 1), text("mid"), marker(2, 2)]);
    let marker = source.pieces()[0];
    let digest = match marker {
        ContentPieceRecord::ImageMarker { digest, .. } => digest,
        ContentPieceRecord::Text(_) => panic!("fixture begins with a marker"),
    };
    let corrupt = rebuild_marker(marker, InputMarkerOrdinal::new(2).unwrap(), digest);
    let fixture = seed("phase13-content-segments-marker-order", &source);
    let mut mutation = FixtureBatch::new();
    mutation.put(FixtureRecord::ContentPiece(corrupt)).unwrap();
    commit(&fixture.store, fixture.storage.clone(), mutation);
    assert!(matches!(
        fixture
            .storage
            .prove_sealed_content_text_segment(&fixture.store, fixture.content, None,),
        Err(SyndicReadError::Invariant(
            "content text segment marker ordinal is not contiguous"
        ))
    ));
}

#[test]
fn carried_preceding_marker_proof_rejects_a_changed_marker_ordinal() {
    let source = prepared(vec![marker(1, 1), text("mid"), marker(2, 2), text("tail")]);
    let fixture = seed("phase13-content-segments-carried-marker-order", &source);
    let leading = prove(&fixture, None);
    let preceding = leading.following_marker().unwrap();
    let marker = source.pieces()[0];
    let digest = match marker {
        ContentPieceRecord::ImageMarker { digest, .. } => digest,
        ContentPieceRecord::Text(_) => panic!("fixture begins with a marker"),
    };
    let corrupt = rebuild_marker(marker, InputMarkerOrdinal::new(2).unwrap(), digest);
    let mut mutation = FixtureBatch::new();
    mutation.put(FixtureRecord::ContentPiece(corrupt)).unwrap();
    commit(&fixture.store, fixture.storage.clone(), mutation);

    assert!(matches!(
        fixture.storage.prove_sealed_content_text_segment(
            &fixture.store,
            fixture.content,
            Some(preceding),
        ),
        Err(SyndicReadError::Invariant(
            "content text segment preceding marker proof disagrees"
        ))
    ));
}

fn assert_marker_bytes_reject(fixture: &Fixture) {
    assert!(matches!(
        fixture
            .storage
            .prove_sealed_content_text_segment(&fixture.store, fixture.content, None,),
        Err(SyndicReadError::Invariant(
            "content text segment marker bytes or digest disagree"
        ))
    ));
}

fn rebuild_marker(
    piece: ContentPieceRecord,
    replacement_ordinal: InputMarkerOrdinal,
    replacement_digest: [u8; 32],
) -> ContentPieceRecord {
    let ContentPieceRecord::ImageMarker {
        content_id,
        ordinal,
        atom_ordinal,
        logical_offset,
        encoded_start,
        encoded_end,
        marker_id,
        label,
        ..
    } = piece
    else {
        panic!("fixture piece is an image marker")
    };
    ContentPieceRecord::image_marker(
        content_id,
        ordinal,
        atom_ordinal,
        replacement_ordinal,
        logical_offset,
        encoded_start,
        encoded_end,
        marker_id,
        label,
        replacement_digest,
    )
    .unwrap()
}

#![cfg(feature = "test-faults")]

use std::convert::Infallible;

use beryl_model::{SyndicDraftMarkerId, advance_content_marker_digest, content_marker_digest_seed};
use syndic_storage::{
    CONTENT_CHUNK_MAX_BYTES, ComposerAtom, ComposerPayload, ContentByteSpanRecord,
    ContentChunkRecord, ContentPieceRecord, ContentTextSpanRecord, ImageLabelOrdinal,
    PreparedContent,
    test_faults::{
        ComposerV1AtomWriter, ComposerV1FoldError, ComposerV1RecordSink, fold_composer_v1,
        plan_composer_v1,
    },
};

#[derive(Default)]
struct Records {
    chunks: Vec<(ContentChunkRecord, ContentByteSpanRecord)>,
    text_spans: Vec<ContentTextSpanRecord>,
    pieces: Vec<ContentPieceRecord>,
}

impl ComposerV1RecordSink for Records {
    type Error = Infallible;

    fn chunk(
        &mut self,
        chunk: ContentChunkRecord,
        span: ContentByteSpanRecord,
    ) -> Result<(), Self::Error> {
        self.chunks.push((chunk, span));
        Ok(())
    }

    fn text_piece(
        &mut self,
        span: ContentTextSpanRecord,
        piece: ContentPieceRecord,
    ) -> Result<(), Self::Error> {
        self.text_spans.push(span);
        self.pieces.push(piece);
        Ok(())
    }

    fn image_piece(&mut self, piece: ContentPieceRecord) -> Result<(), Self::Error> {
        self.pieces.push(piece);
        Ok(())
    }
}

fn write_text<E>(
    writer: &mut dyn ComposerV1AtomWriter<SinkError = E>,
    length: usize,
    fragments: &[&str],
) -> Result<(), ComposerV1FoldError<E>> {
    writer.begin_text(u64::try_from(length).unwrap())?;
    for fragment in fragments {
        writer.text_fragment(fragment)?;
    }
    writer.end_text()
}

#[test]
fn utf8_scalar_crossing_the_nominal_chunk_edge_moves_whole() {
    let prefix = "a".repeat(CONTENT_CHUNK_MAX_BYTES - 19);
    let suffix = "💎z";
    let logical_bytes = prefix.len() + suffix.len();
    let plan = plan_composer_v1(1, |index, writer| {
        assert_eq!(index, 0);
        write_text(writer, logical_bytes, &[&prefix, suffix])
    })
    .unwrap();
    let mut records = Records::default();
    let outcome = fold_composer_v1(plan, &mut records, |index, writer| {
        assert_eq!(index, 0);
        write_text(writer, logical_bytes, &[&prefix, suffix])
    })
    .unwrap();

    assert_eq!(outcome, plan);
    assert_eq!(outcome.max_buffer_bytes(), CONTENT_CHUNK_MAX_BYTES - 1);
    assert_eq!(records.chunks.len(), 2);
    assert!(records.chunks.iter().all(|(chunk, span)| {
        chunk.content_id() == outcome.content_id() && span.content_id() == outcome.content_id()
    }));
    assert_eq!(
        records.chunks[0].0.bytes().len(),
        CONTENT_CHUNK_MAX_BYTES - 1
    );
    assert_eq!(records.chunks[1].0.bytes(), suffix.as_bytes());
    assert_eq!(records.chunks[0].1.start(), 0);
    assert_eq!(
        records.chunks[0].1.end(),
        u64::try_from(CONTENT_CHUNK_MAX_BYTES - 1).unwrap()
    );
    assert_eq!(records.text_spans.len(), 2);
    assert_eq!(records.text_spans[0].logical_start(), 0);
    assert_eq!(records.text_spans[0].logical_end(), prefix.len() as u64);
    assert_eq!(records.text_spans[0].encoded_start(), 18);
    assert_eq!(
        records.text_spans[0].encoded_end(),
        u64::try_from(CONTENT_CHUNK_MAX_BYTES - 1).unwrap()
    );
    assert_eq!(records.text_spans[1].logical_start(), prefix.len() as u64);
    assert_eq!(records.text_spans[1].logical_end(), logical_bytes as u64);
    assert_eq!(
        outcome.summary().encoded_bytes(),
        u64::try_from(18 + logical_bytes).unwrap()
    );
    assert_eq!(outcome.summary().logical_utf8_bytes(), logical_bytes as u64);
    assert_eq!(outcome.summary().piece_count(), 2);
}

#[test]
fn empty_payload_and_empty_text_atom_preserve_distinct_canonical_encodings() {
    let empty_plan = plan_composer_v1(0, |_, _| unreachable!()).unwrap();
    let mut empty_records = Records::default();
    let empty_outcome =
        fold_composer_v1(empty_plan, &mut empty_records, |_, _| unreachable!()).unwrap();
    assert_eq!(empty_outcome.summary().atom_count(), 0);
    assert_eq!(empty_outcome.summary().encoded_bytes(), 9);
    assert_eq!(
        empty_records.chunks[0].0.bytes(),
        &[1, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert!(empty_records.pieces.is_empty());

    let text_plan = plan_composer_v1(1, |_, writer| write_text(writer, 0, &[])).unwrap();
    let mut text_records = Records::default();
    let text_outcome = fold_composer_v1(text_plan, &mut text_records, |_, writer| {
        write_text(writer, 0, &[])
    })
    .unwrap();
    let mut expected = vec![1];
    expected.extend_from_slice(&1_u64.to_be_bytes());
    expected.push(0);
    expected.extend_from_slice(&0_u64.to_be_bytes());
    assert_eq!(text_records.chunks[0].0.bytes(), expected);
    assert_eq!(text_outcome.summary().atom_count(), 1);
    assert_eq!(text_outcome.summary().logical_utf8_bytes(), 0);
    assert_eq!(text_outcome.summary().piece_count(), 0);
    assert_ne!(empty_outcome.content_id(), text_outcome.content_id());
}

#[test]
fn marker_order_and_cross_chunk_ranges_match_prepared_content() {
    let prefix = "p".repeat(CONTENT_CHUNK_MAX_BYTES - 28);
    let first_marker = SyndicDraftMarkerId::from_bytes([1; 16]);
    let second_marker = SyndicDraftMarkerId::from_bytes([2; 16]);
    let first_label = ImageLabelOrdinal::new(3).unwrap();
    let second_label = ImageLabelOrdinal::new(7).unwrap();
    let drive =
        |index: u64, writer: &mut dyn ComposerV1AtomWriter<SinkError = Infallible>| match index {
            0 => write_text(writer, prefix.len(), &[&prefix]),
            1 => writer.image_marker(first_marker, first_label),
            2 => writer.image_marker(second_marker, second_label),
            3 => write_text(writer, "é".len(), &["é"]),
            _ => unreachable!(),
        };
    let plan = plan_composer_v1(4, drive).unwrap();
    let mut records = Records::default();
    let outcome = fold_composer_v1(plan, &mut records, |index, writer| match index {
        0 => write_text(writer, prefix.len(), &[&prefix]),
        1 => writer.image_marker(first_marker, first_label),
        2 => writer.image_marker(second_marker, second_label),
        3 => write_text(writer, "é".len(), &["é"]),
        _ => unreachable!(),
    })
    .unwrap();

    assert_eq!(records.chunks.len(), 2);
    assert_eq!(records.chunks[0].0.bytes().len(), CONTENT_CHUNK_MAX_BYTES);
    assert_eq!(records.chunks[1].0.bytes().len(), 51);
    assert_eq!(records.pieces.len(), 4);
    assert_eq!(records.text_spans.len(), 2);
    assert!(!records.text_spans[0].break_before());
    assert!(records.text_spans[1].break_before());
    assert_eq!(records.text_spans[1].logical_start(), prefix.len() as u64);

    let ContentPieceRecord::ImageMarker {
        ordinal,
        atom_ordinal,
        marker_ordinal,
        logical_offset,
        encoded_start,
        encoded_end,
        marker_id,
        label,
        ..
    } = records.pieces[1]
    else {
        panic!("second piece must be the first image marker")
    };
    assert_eq!(ordinal.get(), 2);
    assert_eq!(atom_ordinal.get(), 2);
    assert_eq!(marker_ordinal.get(), 1);
    assert_eq!(logical_offset, prefix.len() as u64);
    assert_eq!(encoded_start, (CONTENT_CHUNK_MAX_BYTES - 10) as u64);
    assert_eq!(encoded_end, (CONTENT_CHUNK_MAX_BYTES + 15) as u64);
    assert_eq!(marker_id, first_marker);
    assert_eq!(label, first_label);

    let ContentPieceRecord::ImageMarker {
        ordinal,
        atom_ordinal,
        marker_ordinal,
        logical_offset,
        encoded_start,
        encoded_end,
        marker_id,
        label,
        ..
    } = records.pieces[2]
    else {
        panic!("third piece must be the second image marker")
    };
    assert_eq!(ordinal.get(), 3);
    assert_eq!(atom_ordinal.get(), 3);
    assert_eq!(marker_ordinal.get(), 2);
    assert_eq!(logical_offset, prefix.len() as u64);
    assert_eq!(encoded_start, (CONTENT_CHUNK_MAX_BYTES + 15) as u64);
    assert_eq!(encoded_end, (CONTENT_CHUNK_MAX_BYTES + 40) as u64);
    assert_eq!(marker_id, second_marker);
    assert_eq!(label, second_label);

    let marker_digest = advance_content_marker_digest(
        advance_content_marker_digest(content_marker_digest_seed(), first_marker, first_label),
        second_marker,
        second_label,
    );
    assert_eq!(outcome.summary().marker_digest(), marker_digest);
    assert_eq!(outcome.summary().maximum_image_label(), Some(second_label));
    assert_eq!(outcome.summary().image_marker_count(), 2);
    assert_eq!(outcome.summary().piece_count(), 4);

    let payload = ComposerPayload::new(vec![
        ComposerAtom::text(&prefix).unwrap(),
        ComposerAtom::image_marker(first_marker, first_label),
        ComposerAtom::image_marker(second_marker, second_label),
        ComposerAtom::text("é").unwrap(),
    ])
    .unwrap();
    let prepared = PreparedContent::composer(&payload).unwrap();
    assert_eq!(prepared.id(), outcome.content_id());
    assert_eq!(prepared.summary(), outcome.summary());
    assert!(
        prepared
            .chunks()
            .iter()
            .eq(records.chunks.iter().map(|(chunk, _)| chunk))
    );
    assert_eq!(prepared.text_spans(), records.text_spans);
    assert_eq!(prepared.pieces(), records.pieces);
}

#[test]
fn final_pass_rejects_any_summary_or_content_identity_drift() {
    let plan = plan_composer_v1(1, |_, writer| write_text(writer, 4, &["same"])).unwrap();
    let mut records = Records::default();
    let result = fold_composer_v1(plan, &mut records, |_, writer| {
        write_text(writer, 4, &["diff"])
    });
    assert!(matches!(result, Err(ComposerV1FoldError::PlanMismatch)));
    assert_eq!(records.chunks.len(), 1);
    assert!(records.chunks[0].0.bytes().ends_with(b"diff"));
}

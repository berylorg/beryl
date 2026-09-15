use sha2::{Digest, Sha256};

use super::{
    DraftComposerBuildRecordV1, DraftComposerMaterializationErrorV1, DraftComposerRecordFrontierV1,
    DraftComposerSourceCursorV1, checked_next,
};
use crate::{
    ComposerAtomOrdinal, ContentChunkOrdinal, ContentPieceOrdinal, ContentPieceRecord,
    ContentTextSpanRecord, DraftPieceLeafValueV1, InputMarkerOrdinal,
};

type DrainedRecord = (
    DraftComposerRecordFrontierV1,
    Option<ContentTextSpanRecord>,
    Option<ContentPieceRecord>,
);

pub(super) fn drain_record(
    build: &DraftComposerBuildRecordV1,
    source: &DraftPieceLeafValueV1,
) -> Result<DrainedRecord, DraftComposerMaterializationErrorV1> {
    let content = build
        .output()
        .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
    let frontier = build.records();
    let rank = frontier.cursor().piece_index();
    let atom_ordinal = ComposerAtomOrdinal::new(checked_next(rank)?)
        .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
    match source {
        DraftPieceLeafValueV1::Text(text) => {
            let atom_offset = usize::try_from(frontier.cursor().atom_encoded_offset())
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let offset = atom_offset.saturating_sub(9);
            if offset > text.len() || !text.is_char_boundary(offset) {
                return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
            }
            let encoded_start = frontier
                .encoded_bytes()
                .checked_add(9_usize.saturating_sub(atom_offset) as u64)
                .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
            if encoded_start >= build.output_encoded_bytes() {
                return drain_partial_atom(build);
            }
            let available = build.output_encoded_bytes() - encoded_start;
            let mut take = usize::try_from(available)
                .unwrap_or(usize::MAX)
                .min(text.len() - offset);
            while take != 0 && !text.is_char_boundary(offset + take) {
                take -= 1;
            }
            if take == 0 {
                return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
            }
            let logical_end = frontier
                .logical_utf8_bytes()
                .checked_add(take as u64)
                .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let encoded_end = encoded_start
                .checked_add(take as u64)
                .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let piece_count = checked_next(frontier.piece_count())?;
            let piece_ordinal = ContentPieceOrdinal::new(piece_count)
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let chunk_ordinal = ContentChunkOrdinal::new(frontier.chunk_ordinal())
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let digest: [u8; 32] = Sha256::digest(&text.as_bytes()[offset..offset + take]).into();
            let span = ContentTextSpanRecord::new(
                content.id(),
                piece_ordinal,
                chunk_ordinal,
                frontier.chunk_start(),
                frontier.logical_utf8_bytes(),
                logical_end,
                encoded_start,
                encoded_end,
                frontier.break_before(),
                digest,
            )
            .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)?;
            let complete = offset + take == text.len();
            let cursor = if complete {
                DraftComposerSourceCursorV1::new(checked_next(rank)?, 0)
            } else {
                DraftComposerSourceCursorV1::new(rank, (9 + offset + take) as u64)
            };
            let next = DraftComposerRecordFrontierV1::new(
                cursor,
                encoded_end,
                logical_end,
                piece_count,
                frontier.marker_count(),
                frontier.marker_digest(),
                frontier.maximum_image_label(),
                frontier.chunk_start(),
                frontier.chunk_ordinal(),
                false,
            );
            Ok((next, Some(span), Some(ContentPieceRecord::text(span))))
        }
        DraftPieceLeafValueV1::Marker(marker) => {
            let offset = frontier.cursor().atom_encoded_offset();
            if offset >= 25 {
                return Err(DraftComposerMaterializationErrorV1::InvalidBuild);
            }
            let encoded_start = frontier
                .encoded_bytes()
                .checked_sub(offset)
                .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
            let encoded_end = encoded_start
                .checked_add(25)
                .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
            if encoded_end > build.output_encoded_bytes() {
                return drain_partial_atom(build);
            }
            let marker_count = checked_next(frontier.marker_count())?;
            let marker_ordinal = InputMarkerOrdinal::new(marker_count)
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let piece_count = checked_next(frontier.piece_count())?;
            let piece_ordinal = ContentPieceOrdinal::new(piece_count)
                .map_err(|_| DraftComposerMaterializationErrorV1::LengthOverflow)?;
            let mut encoded = [0_u8; 25];
            encoded[0] = 1;
            encoded[1..17].copy_from_slice(marker.marker_id().as_bytes());
            encoded[17..].copy_from_slice(&marker.label().get().to_be_bytes());
            let piece = ContentPieceRecord::image_marker(
                content.id(),
                piece_ordinal,
                atom_ordinal,
                marker_ordinal,
                frontier.logical_utf8_bytes(),
                encoded_start,
                encoded_end,
                marker.marker_id(),
                marker.label(),
                Sha256::digest(encoded).into(),
            )
            .map_err(|_| DraftComposerMaterializationErrorV1::InvalidOutput)?;
            let digest = beryl_model::advance_sequential_marker_digest(
                frontier.marker_digest(),
                marker.marker_id(),
                marker.label(),
            );
            let maximum = Some(
                frontier
                    .maximum_image_label()
                    .map_or(marker.label(), |current| current.max(marker.label())),
            );
            let next = DraftComposerRecordFrontierV1::new(
                DraftComposerSourceCursorV1::new(checked_next(rank)?, 0),
                encoded_end,
                frontier.logical_utf8_bytes(),
                piece_count,
                marker_count,
                digest,
                maximum,
                frontier.chunk_start(),
                frontier.chunk_ordinal(),
                true,
            );
            Ok((next, None, Some(piece)))
        }
    }
}

fn drain_partial_atom(
    build: &DraftComposerBuildRecordV1,
) -> Result<DrainedRecord, DraftComposerMaterializationErrorV1> {
    let frontier = build.records();
    let consumed = build
        .output_encoded_bytes()
        .checked_sub(frontier.encoded_bytes())
        .filter(|consumed| *consumed != 0)
        .ok_or(DraftComposerMaterializationErrorV1::InvalidBuild)?;
    let offset = frontier
        .cursor()
        .atom_encoded_offset()
        .checked_add(consumed)
        .ok_or(DraftComposerMaterializationErrorV1::LengthOverflow)?;
    Ok((
        DraftComposerRecordFrontierV1::new(
            DraftComposerSourceCursorV1::new(frontier.cursor().piece_index(), offset),
            build.output_encoded_bytes(),
            frontier.logical_utf8_bytes(),
            frontier.piece_count(),
            frontier.marker_count(),
            frontier.marker_digest(),
            frontier.maximum_image_label(),
            frontier.chunk_start(),
            frontier.chunk_ordinal(),
            frontier.break_before(),
        ),
        None,
        None,
    ))
}

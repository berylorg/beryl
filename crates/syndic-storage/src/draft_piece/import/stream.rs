use beryl_model::{ImageLabelOrdinal, SyndicDraftMarkerId};
use sha2::{Digest, Sha256};

use super::super::{DRAFT_PIECE_TEXT_LEAF_MAX_BYTES, DraftPieceLeafValueV1, DraftPieceMarkerV1};
use super::{
    model::*,
    read::{SourceReader, encoded_range, source_piece},
};
use crate::{ContentPieceRecord, SyndicReadError};

pub(crate) struct StreamStep<B> {
    pub frontier: DraftSealedComposerFrontierV1<B>,
    pub leaf: Option<DraftPieceLeafValueV1>,
}

pub(crate) fn marker_frame<B: SealedComposerBasisV1>(
    reader: &impl SourceReader,
    frontier: DraftSealedComposerFrontierV1<B>,
) -> Result<(SyndicDraftMarkerId, ImageLabelOrdinal), SyndicReadError> {
    let bytes = encoded_range(
        reader,
        frontier.basis.content(),
        frontier.encoded_cursor,
        25,
    )?;
    if bytes[0] != 1 {
        return invalid();
    }
    let id = SyndicDraftMarkerId::from_bytes(bytes[1..17].try_into().unwrap());
    if id.as_bytes() == &[0; 16] {
        return invalid();
    }
    let label = ImageLabelOrdinal::new(u64::from_be_bytes(bytes[17..25].try_into().unwrap()))
        .map_err(|_| SyndicReadError::Invariant("import image label invalid"))?;
    let piece = source_piece(reader, frontier.basis.content(), frontier.next_piece)?;
    match piece {
        ContentPieceRecord::ImageMarker {
            atom_ordinal,
            marker_ordinal,
            logical_offset,
            encoded_start,
            encoded_end,
            marker_id,
            label: piece_label,
            digest,
            ..
        } if atom_ordinal.get()
            == frontier
                .atoms
                .checked_add(1)
                .ok_or(SyndicReadError::Invariant("import atom overflow"))?
            && marker_ordinal.get()
                == frontier
                    .markers
                    .checked_add(1)
                    .ok_or(SyndicReadError::Invariant("import marker overflow"))?
            && logical_offset == frontier.logical_bytes
            && encoded_start == frontier.encoded_cursor
            && encoded_end == encoded_start + 25
            && marker_id == id
            && piece_label == label
            && digest == <[u8; 32]>::from(Sha256::digest(&bytes)) =>
        {
            Ok((id, label))
        }
        _ => invalid(),
    }
}

pub(crate) fn next_stream_step<B: SealedComposerBasisV1>(
    reader: &impl SourceReader,
    mut f: DraftSealedComposerFrontierV1<B>,
    association: Option<SealedComposerMarkerAssociationV1>,
) -> Result<StreamStep<B>, SyndicReadError> {
    if f.phase != DraftSealedImportPhaseV1::Streaming {
        return invalid();
    }
    let summary = f.basis.content().summary();
    let mut leaf = None;
    match f.decoder {
        DraftSealedImportDecoderV1::BeforeHeader => {
            let header = encoded_range(reader, f.basis.content(), 0, 9)?;
            if header[0] != 1
                || u64::from_be_bytes(header[1..9].try_into().unwrap()) != summary.atom_count()
            {
                return invalid();
            }
            f.encoded_cursor = 9;
            f.decoder = DraftSealedImportDecoderV1::BetweenAtoms;
        }
        DraftSealedImportDecoderV1::BetweenAtoms => {
            if f.atoms == summary.atom_count() {
                if f.encoded_cursor != summary.encoded_bytes()
                    || f.logical_bytes != summary.logical_utf8_bytes()
                    || f.markers != summary.image_marker_count()
                    || f.next_piece
                        != summary
                            .piece_count()
                            .checked_add(1)
                            .ok_or(SyndicReadError::Invariant("import source piece overflow"))?
                    || f.piece_text_offset != 0
                    || f.sequential_digest != summary.marker_digest()
                    || f.maximum_label != summary.maximum_image_label()
                    || f.basis.assets().is_some_and(|p| {
                        p.ordered_assets().marker_asset_digest() != f.ordered_digest
                    })
                {
                    return invalid();
                }
                f.decoder = DraftSealedImportDecoderV1::End;
                f.phase = DraftSealedImportPhaseV1::CrossValidating;
                f.sequential_digest = beryl_model::sequential_marker_digest_seed();
                f.ordered_digest = beryl_model::ordered_marker_asset_digest_seed();
                f.maximum_label = None;
                if !f.is_locally_exact(super::super::DraftPieceBuildLifecycleV1::Open) {
                    return invalid();
                }
            } else {
                let tag = encoded_range(reader, f.basis.content(), f.encoded_cursor, 1)?[0];
                match tag {
                    0 => {
                        let frame = encoded_range(reader, f.basis.content(), f.encoded_cursor, 9)?;
                        let length = u64::from_be_bytes(frame[1..9].try_into().unwrap());
                        f.encoded_cursor = add(f.encoded_cursor, 9)?;
                        if add(f.encoded_cursor, length)? > summary.encoded_bytes() {
                            return invalid();
                        }
                        if length == 0 {
                            f.atoms = add(f.atoms, 1)?;
                        } else {
                            f.text_remaining = length;
                            f.decoder = DraftSealedImportDecoderV1::TextBody;
                        }
                    }
                    1 => {
                        let (id, label) = marker_frame(reader, f.clone())?;
                        let association = association
                            .ok_or(SyndicReadError::Invariant("import image proof required"))?;
                        if association.marker_id() != id
                            || association.label() != label
                            || association.ordinal() != add(f.markers, 1)?
                        {
                            return invalid();
                        }
                        leaf = Some(DraftPieceLeafValueV1::Marker(DraftPieceMarkerV1::new(
                            id,
                            association.ordinal(),
                            label,
                            association.asset_id(),
                        )));
                        f.encoded_cursor = add(f.encoded_cursor, 25)?;
                        f.next_piece = add(f.next_piece, 1)?;
                        f.atoms = add(f.atoms, 1)?;
                        f.markers = add(f.markers, 1)?;
                        f.sequential_digest = beryl_model::advance_sequential_marker_digest(
                            f.sequential_digest,
                            id,
                            label,
                        );
                        f.ordered_digest = beryl_model::advance_ordered_marker_asset_digest(
                            f.ordered_digest,
                            id,
                            label,
                            association.asset_id(),
                        );
                        f.maximum_label = Some(f.maximum_label.map_or(label, |v| v.max(label)));
                    }
                    _ => return invalid(),
                }
            }
        }
        DraftSealedImportDecoderV1::TextBody => {
            let mut bytes = Vec::new();
            for _ in 0..4 {
                let piece = source_piece(reader, f.basis.content(), f.next_piece)?;
                let ContentPieceRecord::Text(span) = piece else {
                    return invalid();
                };
                if span.logical_start().checked_add(f.piece_text_offset) != Some(f.logical_bytes)
                    || span.encoded_start().checked_add(f.piece_text_offset)
                        != Some(f.encoded_cursor)
                    || f.piece_text_offset >= span.len()
                {
                    return invalid();
                }
                let whole_span = encoded_range(
                    reader,
                    f.basis.content(),
                    span.encoded_start(),
                    span.len() as usize,
                )?;
                if <[u8; 32]>::from(Sha256::digest(&whole_span)) != span.digest() {
                    return invalid();
                }
                let available = span.len() - f.piece_text_offset;
                let limit = (DRAFT_PIECE_TEXT_LEAF_MAX_BYTES - bytes.len()) as u64;
                let take = available.min(f.text_remaining).min(limit);
                if take == 0 {
                    return invalid();
                }
                let chunk = &whole_span
                    [f.piece_text_offset as usize..(f.piece_text_offset + take) as usize];
                bytes.extend_from_slice(chunk);
                let valid = match std::str::from_utf8(&bytes) {
                    Ok(_) => bytes.len(),
                    Err(error) if error.error_len().is_none() => error.valid_up_to(),
                    Err(_) => return invalid(),
                };
                let incomplete = bytes.len() - valid;
                let crosses_piece = take == available
                    && incomplete != 0
                    && bytes.len() < DRAFT_PIECE_TEXT_LEAF_MAX_BYTES;
                let committed = if crosses_piece {
                    take
                } else {
                    take.checked_sub(incomplete as u64)
                        .ok_or(SyndicReadError::Invariant("import UTF-8 cursor mismatch"))?
                };
                if !crosses_piece {
                    bytes.truncate(valid);
                }
                f.encoded_cursor = add(f.encoded_cursor, committed)?;
                f.logical_bytes = add(f.logical_bytes, committed)?;
                f.text_remaining = f
                    .text_remaining
                    .checked_sub(committed)
                    .ok_or(SyndicReadError::Invariant("import text framing mismatch"))?;
                f.piece_text_offset = add(f.piece_text_offset, committed)?;
                if f.piece_text_offset == span.len() {
                    f.next_piece = add(f.next_piece, 1)?;
                    f.piece_text_offset = 0;
                }
                if !crosses_piece {
                    break;
                }
            }
            let text = String::from_utf8(bytes)
                .map_err(|_| SyndicReadError::Invariant("import text window incomplete UTF-8"))?;
            if text.is_empty() || text.as_bytes().contains(&0) {
                return invalid();
            }
            leaf = Some(DraftPieceLeafValueV1::Text(text));
            if f.text_remaining == 0 {
                f.decoder = DraftSealedImportDecoderV1::BetweenAtoms;
                f.atoms = add(f.atoms, 1)?;
            }
        }
        DraftSealedImportDecoderV1::End => return invalid(),
    }
    Ok(StreamStep { frontier: f, leaf })
}

fn add(left: u64, right: u64) -> Result<u64, SyndicReadError> {
    left.checked_add(right)
        .ok_or(SyndicReadError::Invariant("import source cursor overflow"))
}
fn invalid<T>() -> Result<T, SyndicReadError> {
    Err(SyndicReadError::Invariant(
        "sealed import source window disagreement",
    ))
}

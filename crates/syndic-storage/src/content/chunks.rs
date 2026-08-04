use super::*;

pub(super) fn encoded_chunk_ranges(
    encoded: &[u8],
    prepared: &[PreparedPiece],
) -> Result<Vec<(usize, usize)>, SyndicRecordError> {
    let mut ranges = Vec::new();
    let mut start = 0_usize;
    let mut piece_index = 0_usize;
    while start < encoded.len() {
        let mut end = start
            .checked_add(crate::CONTENT_CHUNK_MAX_BYTES)
            .ok_or(SyndicRecordError::LengthOverflow {
                kind: "content chunk range",
            })?
            .min(encoded.len());
        if end < encoded.len() {
            while let Some(piece) = prepared.get(piece_index) {
                let (_, piece_end) = piece.encoded_range();
                if piece_end > end as u64 {
                    break;
                }
                piece_index += 1;
            }
            if let Some(PreparedPiece::Text(text)) = prepared.get(piece_index)
                && text.encoded_start < end as u64
                && (end as u64) < text.encoded_end
            {
                let text_start = usize::try_from(text.encoded_start).map_err(|_| {
                    SyndicRecordError::LengthOverflow {
                        kind: "content text chunk boundary",
                    }
                })?;
                let text_end = usize::try_from(text.encoded_end).map_err(|_| {
                    SyndicRecordError::LengthOverflow {
                        kind: "content text chunk boundary",
                    }
                })?;
                let source = std::str::from_utf8(
                    encoded
                        .get(text_start..text_end)
                        .ok_or(SyndicRecordError::InvalidContentEncoding)?,
                )
                .map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
                let mut local_end = end - text_start;
                while !source.is_char_boundary(local_end) {
                    end -= 1;
                    local_end -= 1;
                }
            }
        }
        if end <= start {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
        ranges.push((start, end));
        start = end;
    }
    Ok(ranges)
}

pub(super) fn prepared_content_pieces(
    content_id: SyndicContentId,
    encoded: &[u8],
    chunk_ranges: &[(usize, usize)],
    prepared: &[PreparedPiece],
) -> Result<
    (
        Vec<crate::ContentTextSpanRecord>,
        Vec<crate::ContentPieceRecord>,
    ),
    SyndicRecordError,
> {
    let mut text_spans = Vec::new();
    let mut pieces = Vec::new();
    for prepared_piece in prepared {
        let PreparedPiece::Text(range) = prepared_piece else {
            let PreparedPiece::ImageMarker {
                atom_ordinal,
                marker_ordinal,
                logical_offset,
                encoded_start,
                encoded_end,
                marker_id,
                label,
            } = *prepared_piece
            else {
                unreachable!("prepared piece variants are exhaustive")
            };
            let start =
                usize::try_from(encoded_start).map_err(|_| SyndicRecordError::LengthOverflow {
                    kind: "composer marker range",
                })?;
            let end =
                usize::try_from(encoded_end).map_err(|_| SyndicRecordError::LengthOverflow {
                    kind: "composer marker range",
                })?;
            let source = encoded
                .get(start..end)
                .ok_or(SyndicRecordError::InvalidContentEncoding)?;
            let ordinal = next_content_piece_ordinal(pieces.len())?;
            pieces.push(crate::ContentPieceRecord::image_marker(
                content_id,
                ordinal,
                atom_ordinal,
                marker_ordinal,
                logical_offset,
                encoded_start,
                encoded_end,
                marker_id,
                label,
                Sha256::digest(source).into(),
            )?);
            continue;
        };
        let range_start = usize::try_from(range.encoded_start).map_err(|_| {
            SyndicRecordError::LengthOverflow {
                kind: "content text encoded range",
            }
        })?;
        let range_end =
            usize::try_from(range.encoded_end).map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "content text encoded range",
            })?;
        let mut chunk_index = chunk_ranges.partition_point(|(_, end)| *end <= range_start);
        let mut logical_start = range.logical_start;
        let mut first = true;
        let mut encoded_start = range_start;
        while encoded_start < range_end {
            let &(chunk_start, chunk_end) = chunk_ranges
                .get(chunk_index)
                .ok_or(SyndicRecordError::InvalidContentEncoding)?;
            if encoded_start < chunk_start || encoded_start >= chunk_end {
                return Err(SyndicRecordError::InvalidContentEncoding);
            }
            let segment_end = chunk_end.min(range_end);
            let source = encoded
                .get(encoded_start..segment_end)
                .ok_or(SyndicRecordError::InvalidContentEncoding)?;
            std::str::from_utf8(source).map_err(|_| SyndicRecordError::InvalidContentEncoding)?;
            let logical_end = logical_start.checked_add(source.len() as u64).ok_or(
                SyndicRecordError::LengthOverflow {
                    kind: "content logical text range",
                },
            )?;
            let ordinal = next_content_piece_ordinal(pieces.len())?;
            let chunk_ordinal = ContentChunkOrdinal::new(
                u64::try_from(chunk_index)
                    .ok()
                    .and_then(|index| index.checked_add(1))
                    .ok_or(SyndicRecordError::LengthOverflow {
                        kind: "content text chunk ordinal",
                    })?,
            )
            .map_err(|_| SyndicRecordError::LengthOverflow {
                kind: "content text chunk ordinal",
            })?;
            let span = crate::ContentTextSpanRecord::new(
                content_id,
                ordinal,
                chunk_ordinal,
                chunk_start as u64,
                logical_start,
                logical_end,
                encoded_start as u64,
                segment_end as u64,
                first && range.break_before,
                Sha256::digest(source).into(),
            )?;
            text_spans.push(span);
            pieces.push(crate::ContentPieceRecord::text(span));
            encoded_start = segment_end;
            logical_start = logical_end;
            first = false;
            chunk_index += 1;
        }
        if logical_start != range.logical_end {
            return Err(SyndicRecordError::InvalidContentEncoding);
        }
    }
    Ok((text_spans, pieces))
}

fn next_content_piece_ordinal(
    current_count: usize,
) -> Result<crate::ContentPieceOrdinal, SyndicRecordError> {
    let ordinal = u64::try_from(current_count)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or(SyndicRecordError::LengthOverflow {
            kind: "content pieces",
        })?;
    crate::ContentPieceOrdinal::new(ordinal).map_err(|_| SyndicRecordError::LengthOverflow {
        kind: "content pieces",
    })
}

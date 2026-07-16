use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};

use crate::{
    ContentPieceRecord, ContentTextSpanRecord, SyndicMutationError, codec::*, domain::SyndicDomain,
};

use super::super::{point, required};

pub(crate) enum LoadedPiece {
    Text {
        span: ContentTextSpanRecord,
        source: Box<str>,
    },
    ImageMarker(ContentPieceRecord),
    End,
}

pub(crate) fn load_piece(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &crate::ItemProjectionBuildRecord,
    checkpoint: &crate::MarkdownParserCheckpoint,
) -> Result<LoadedPiece, SyndicMutationError> {
    if checkpoint.consumed_source_bytes() > build.source_bytes() {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    let piece_count = build.source_content().summary().piece_count();
    let next_piece = checkpoint.next_piece_ordinal().get();
    let eof_piece = piece_count
        .checked_add(1)
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    if next_piece >= eof_piece {
        return if next_piece == eof_piece
            && checkpoint.consumed_source_bytes() == build.source_bytes()
        {
            Ok(LoadedPiece::End)
        } else {
            Err(SyndicMutationError::ProjectionBuildConflict)
        };
    }
    let key = ContentPieceKey {
        owner: build.source_content().id(),
        ordinal: checkpoint.next_piece_ordinal(),
    };
    let Some(piece) = point::<ContentPiecesFamily>(reader, &key)? else {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    };
    if piece.content_id() != build.source_content().id()
        || piece.ordinal() != checkpoint.next_piece_ordinal()
    {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    match piece {
        ContentPieceRecord::Text(span) => {
            if checkpoint.consumed_source_bytes() < span.logical_start()
                || checkpoint.consumed_source_bytes() >= span.logical_end()
            {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            let chunk = required::<ContentChunksFamily>(
                reader,
                &ContentChunkKey {
                    owner: span.content_id(),
                    ordinal: span.chunk_ordinal(),
                },
            )?;
            let logical_offset = checkpoint.consumed_source_bytes() - span.logical_start();
            let encoded_start = span
                .encoded_start()
                .checked_add(logical_offset)
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            let local_start = usize::try_from(encoded_start - span.chunk_start())
                .map_err(|_| SyndicMutationError::ProjectionBuildConflict)?;
            let local_end = usize::try_from(span.encoded_end() - span.chunk_start())
                .map_err(|_| SyndicMutationError::ProjectionBuildConflict)?;
            let bytes = chunk
                .bytes()
                .get(local_start..local_end)
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            let source = std::str::from_utf8(bytes)
                .map_err(|_| SyndicMutationError::ProjectionBuildConflict)?
                .into();
            Ok(LoadedPiece::Text { span, source })
        }
        marker @ ContentPieceRecord::ImageMarker { logical_offset, .. } => {
            if logical_offset != checkpoint.consumed_source_bytes() {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            Ok(LoadedPiece::ImageMarker(marker))
        }
    }
}

pub(super) fn read_logical_range(
    reader: &DomainReader<'_, SyndicDomain>,
    content: crate::ContentReference,
    range: crate::ProjectionSourceRange,
) -> Result<Box<str>, SyndicMutationError> {
    if range.len() > crate::TRANSCRIPT_PAGE_MAX_BYTES as u64
        || range.end() > content.summary().logical_utf8_bytes()
    {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    let predecessor = reader.cursor::<ContentTextSpansCodec>(
        &CursorRange::closed(
            ContentTextSpanKey {
                owner: content.id(),
                logical_start: 0,
            },
            ContentTextSpanKey {
                owner: content.id(),
                logical_start: range.start(),
            },
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 1024 * 1024)
            .expect("logical-range predecessor bounds are nonzero"),
    )?;
    let first = predecessor
        .records()
        .first()
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    let mut logical = range.start();
    let mut after = None;
    let mut output = Vec::with_capacity(range.len() as usize);
    loop {
        let last = ContentTextSpanKey {
            owner: content.id(),
            logical_start: range.end(),
        };
        let cursor = match after {
            Some(previous) => CursorRange::after(
                ContentTextSpanKey {
                    owner: content.id(),
                    logical_start: previous,
                },
                last,
            ),
            None => CursorRange::closed(first.key().to_owned(), last),
        };
        let page = reader.cursor::<ContentTextSpansCodec>(
            &cursor,
            CursorDirection::Forward,
            CursorReadLimits::new(64, 1024 * 1024).expect("logical-range page bounds are nonzero"),
        )?;
        if page.records().is_empty() {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        for record in page.records() {
            let span = record.value();
            if span.logical_start() > logical || span.logical_end() <= logical {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            let end = range.end().min(span.logical_end());
            let chunk = required::<ContentChunksFamily>(
                reader,
                &ContentChunkKey {
                    owner: content.id(),
                    ordinal: span.chunk_ordinal(),
                },
            )?;
            let encoded_start = span
                .encoded_start()
                .checked_add(logical - span.logical_start())
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            let encoded_end = encoded_start
                .checked_add(end - logical)
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            let local_start = usize::try_from(encoded_start - span.chunk_start())
                .map_err(|_| SyndicMutationError::ProjectionBuildConflict)?;
            let local_end = usize::try_from(encoded_end - span.chunk_start())
                .map_err(|_| SyndicMutationError::ProjectionBuildConflict)?;
            output.extend_from_slice(
                chunk
                    .bytes()
                    .get(local_start..local_end)
                    .ok_or(SyndicMutationError::ProjectionBuildConflict)?,
            );
            logical = end;
            after = Some(span.logical_start());
            if logical == range.end() {
                return std::str::from_utf8(&output)
                    .map(Into::into)
                    .map_err(|_| SyndicMutationError::ProjectionBuildConflict);
            }
        }
        if !page.has_more() {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
    }
}

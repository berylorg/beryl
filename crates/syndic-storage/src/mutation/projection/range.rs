use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};

use crate::{ContentPieceRecord, SyndicMutationError, codec::*, domain::SyndicDomain};

use super::super::{point, required};

pub(crate) enum LoadedPiece {
    Text {
        logical_end: u64,
        source: Box<str>,
        next_cursor: crate::ProjectionTextSourceCursor,
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
    match build.source() {
        crate::ProjectionTextSource::Composer(content) => {
            load_composer_piece(reader, content, build, checkpoint)
        }
        crate::ProjectionTextSource::ProviderNarrative(narrative) => {
            load_provider_piece(reader, narrative, build, checkpoint)
        }
    }
}

fn load_composer_piece(
    reader: &DomainReader<'_, SyndicDomain>,
    content: crate::ContentReference,
    build: &crate::ItemProjectionBuildRecord,
    checkpoint: &crate::MarkdownParserCheckpoint,
) -> Result<LoadedPiece, SyndicMutationError> {
    let Some(next_ordinal) = checkpoint.source_cursor().composer_piece() else {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    };
    let piece_count = content.summary().piece_count();
    let next_piece = next_ordinal.get();
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
        owner: content.id(),
        ordinal: next_ordinal,
    };
    let Some(piece) = point::<ContentPiecesFamily>(reader, &key)? else {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    };
    if piece.content_id() != content.id() || piece.ordinal() != next_ordinal {
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
            Ok(LoadedPiece::Text {
                logical_end: span.logical_end(),
                source,
                next_cursor: crate::ProjectionTextSourceCursor::Composer(
                    next_ordinal.checked_next()?,
                ),
            })
        }
        marker @ ContentPieceRecord::ImageMarker { logical_offset, .. } => {
            if logical_offset != checkpoint.consumed_source_bytes() {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            Ok(LoadedPiece::ImageMarker(marker))
        }
    }
}

fn load_provider_piece(
    reader: &DomainReader<'_, SyndicDomain>,
    narrative: crate::ProviderNarrativeReference,
    build: &crate::ItemProjectionBuildRecord,
    checkpoint: &crate::MarkdownParserCheckpoint,
) -> Result<LoadedPiece, SyndicMutationError> {
    let Some(span_start) = checkpoint.source_cursor().provider_logical_start() else {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    };
    let consumed = checkpoint.consumed_source_bytes();
    if consumed == build.source_bytes() {
        return if span_start == consumed {
            Ok(LoadedPiece::End)
        } else {
            Err(SyndicMutationError::ProjectionBuildConflict)
        };
    }
    let key =
        ProviderNarrativeSpanKey::new(narrative.content_id(), narrative.generation(), span_start);
    let span = point::<ProviderNarrativeSpansFamily>(reader, &key)?
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    if span.content_id() != narrative.content_id()
        || span.generation() != narrative.generation()
        || span.logical_start() != span_start
        || consumed < span_start
        || consumed >= span.logical_end()
        || span.logical_end() > narrative.logical_utf8_bytes()
    {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    let remaining = span.logical_end() - consumed;
    let requested = remaining.min(crate::TRANSCRIPT_PAGE_MAX_BYTES as u64);
    let source_start = span
        .source_start()
        .checked_add(consumed - span.logical_start())
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    let source_end = source_start
        .checked_add(requested)
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    if source_end > span.source_end() {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    let manifest = required::<ContentManifestsFamily>(reader, &narrative.content_id())?;
    if manifest.encoding() != crate::ContentEncoding::ProviderItemV1
        || manifest.current_reference().is_none()
    {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    let bytes = crate::validation::read_encoded_range(
        reader,
        narrative.content_id(),
        manifest.encoded_bytes(),
        source_start,
        source_end,
    )
    .map_err(|_| SyndicMutationError::ProjectionBuildConflict)?;
    let valid_bytes = match std::str::from_utf8(&bytes) {
        Ok(_) => bytes.len(),
        Err(error) if error.error_len().is_none() && error.valid_up_to() != 0 => {
            error.valid_up_to()
        }
        Err(_) => return Err(SyndicMutationError::ProjectionBuildConflict),
    };
    let source = std::str::from_utf8(&bytes[..valid_bytes])
        .map_err(|_| SyndicMutationError::ProjectionBuildConflict)?
        .into();
    let logical_end = consumed
        .checked_add(valid_bytes as u64)
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    let next_start = if logical_end == span.logical_end() {
        logical_end
    } else {
        span_start
    };
    Ok(LoadedPiece::Text {
        logical_end,
        source,
        next_cursor: crate::ProjectionTextSourceCursor::ProviderNarrative {
            logical_start: next_start,
        },
    })
}

pub(super) fn read_source_range(
    reader: &DomainReader<'_, SyndicDomain>,
    source: crate::ProjectionTextSource,
    range: crate::ProjectionSourceRange,
) -> Result<Box<str>, SyndicMutationError> {
    match source {
        crate::ProjectionTextSource::Composer(content) => {
            read_composer_logical_range(reader, content, range)
        }
        crate::ProjectionTextSource::ProviderNarrative(narrative) => {
            read_provider_logical_range(reader, narrative, range)
        }
    }
}

fn read_composer_logical_range(
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

fn read_provider_logical_range(
    reader: &DomainReader<'_, SyndicDomain>,
    narrative: crate::ProviderNarrativeReference,
    range: crate::ProjectionSourceRange,
) -> Result<Box<str>, SyndicMutationError> {
    if range.len() > crate::TRANSCRIPT_PAGE_MAX_BYTES as u64
        || range.end() > narrative.logical_utf8_bytes()
    {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    let manifest = required::<ContentManifestsFamily>(reader, &narrative.content_id())?;
    if manifest.encoding() != crate::ContentEncoding::ProviderItemV1
        || manifest.current_reference().is_none()
    {
        return Err(SyndicMutationError::ProjectionBuildConflict);
    }
    let predecessor = reader.cursor::<ProviderNarrativeSpansCodec>(
        &CursorRange::closed(
            ProviderNarrativeSpanKey::first_for_generation(
                narrative.content_id(),
                narrative.generation(),
            ),
            ProviderNarrativeSpanKey::new(
                narrative.content_id(),
                narrative.generation(),
                range.start(),
            ),
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 512).expect("narrative predecessor bounds are nonzero"),
    )?;
    let first = predecessor
        .records()
        .first()
        .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
    let mut logical = range.start();
    let mut after = None;
    let mut output = Vec::with_capacity(range.len() as usize);
    loop {
        let last = ProviderNarrativeSpanKey::new(
            narrative.content_id(),
            narrative.generation(),
            range.end(),
        );
        let cursor = match after {
            Some(previous) => CursorRange::after(
                ProviderNarrativeSpanKey::new(
                    narrative.content_id(),
                    narrative.generation(),
                    previous,
                ),
                last,
            ),
            None => CursorRange::closed(*first.key(), last),
        };
        let page = reader.cursor::<ProviderNarrativeSpansCodec>(
            &cursor,
            CursorDirection::Forward,
            CursorReadLimits::new(64, 65_536)
                .expect("narrative logical-range page bounds are nonzero"),
        )?;
        if page.records().is_empty() {
            return Err(SyndicMutationError::ProjectionBuildConflict);
        }
        for record in page.records() {
            let span = record.value();
            if record.key().content_id() != narrative.content_id()
                || record.key().generation() != narrative.generation()
                || record.key().logical_start() != span.logical_start()
                || span.logical_start() > logical
                || span.logical_end() <= logical
            {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            let end = range.end().min(span.logical_end());
            let encoded_start = span
                .source_start()
                .checked_add(logical - span.logical_start())
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            let encoded_end = encoded_start
                .checked_add(end - logical)
                .ok_or(SyndicMutationError::ProjectionBuildConflict)?;
            if encoded_end > span.source_end() {
                return Err(SyndicMutationError::ProjectionBuildConflict);
            }
            output.extend_from_slice(
                &crate::validation::read_encoded_range(
                    reader,
                    narrative.content_id(),
                    manifest.encoded_bytes(),
                    encoded_start,
                    encoded_end,
                )
                .map_err(|_| SyndicMutationError::ProjectionBuildConflict)?,
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

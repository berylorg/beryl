use super::*;

pub(super) fn finish_page(
    output: Vec<u8>,
    logical: u64,
    start: u64,
    max_payload_bytes: usize,
    totals: ReadByteTotals,
    #[cfg(feature = "test-faults")] output_residency: Option<ContentTextReadResidencyLease>,
) -> Result<TextPageAssembly, SyndicReadError> {
    if output.is_empty() {
        return Err(SyndicReadError::ContentTextReadLimitTooSmall {
            offset: start,
            actual: max_payload_bytes,
        });
    }
    Ok(TextPageAssembly {
        bytes: output,
        end: logical,
        stored_bytes: totals.stored,
        decoded_bytes: totals.decoded,
        #[cfg(feature = "test-faults")]
        output_residency,
    })
}

pub(super) struct CachedChunk {
    ordinal: ContentChunkOrdinal,
    start: u64,
    record: ContentChunkRecord,
    #[cfg(feature = "test-faults")]
    _residency: Option<ContentTextReadResidencyLease>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_span(
    storage: &SyndicStorage,
    store: &HomeStore,
    content: ContentReference,
    key: &ContentTextSpanKey,
    span: ContentTextSpanRecord,
    predecessor: bool,
    segment: Option<TextSegmentBounds>,
    cached_chunk: &mut Option<CachedChunk>,
    logical: &mut u64,
    desired_end: u64,
    output: &mut Vec<u8>,
    totals: &mut ReadByteTotals,
    #[cfg(feature = "test-faults")] tracker: Option<&ContentTextReadResidencyTracker>,
) -> Result<bool, SyndicReadError> {
    validate_span(content, key, span, predecessor, segment, *logical)?;
    if cached_chunk.as_ref().is_none_or(|cached| {
        cached.ordinal != span.chunk_ordinal() || cached.start != span.chunk_start()
    }) {
        *cached_chunk = None;
        let key = ContentChunkKey {
            owner: content.id(),
            ordinal: span.chunk_ordinal(),
        };
        let page = storage.page::<ContentChunksFamily>(
            store,
            CursorRange::closed(key.clone(), key),
            CursorReadLimits::new(1, family_cursor_max_bytes::<ContentChunksFamily>())
                .expect("content chunk cursor bound is nonzero"),
        )?;
        totals.add(
            page.stored_bytes(),
            page.decoded_bytes(),
            "logical content text byte accounting overflowed",
        )?;
        let chunk = page
            .into_records()
            .into_iter()
            .next()
            .ok_or(SyndicReadError::Invariant(
                "sealed content text chunk is missing",
            ))?;
        #[cfg(feature = "test-faults")]
        let chunk_bytes = chunk.bytes().len();
        let record = chunk;
        *cached_chunk = Some(CachedChunk {
            ordinal: span.chunk_ordinal(),
            start: span.chunk_start(),
            record,
            #[cfg(feature = "test-faults")]
            _residency: tracker.map(|tracker| tracker.acquire_cached_chunk(chunk_bytes)),
        });
    }
    let cached = cached_chunk
        .as_ref()
        .expect("sealed content text loads its span chunk");
    if cached.record.content_id() != content.id()
        || cached.record.ordinal() != span.chunk_ordinal()
        || <[u8; 32]>::from(Sha256::digest(cached.record.bytes())) != *cached.record.digest()
    {
        return Err(SyndicReadError::Invariant(
            "sealed content text chunk identity or digest disagrees",
        ));
    }

    let full_start = span
        .encoded_start()
        .checked_sub(span.chunk_start())
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(SyndicReadError::Invariant(
            "sealed content text span start is outside its chunk",
        ))?;
    let full_end = span
        .encoded_end()
        .checked_sub(span.chunk_start())
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(SyndicReadError::Invariant(
            "sealed content text span end is outside its chunk",
        ))?;
    let source =
        cached
            .record
            .bytes()
            .get(full_start..full_end)
            .ok_or(SyndicReadError::Invariant(
                "sealed content text span is outside its chunk",
            ))?;
    if <[u8; 32]>::from(Sha256::digest(source)) != span.digest() {
        return Err(SyndicReadError::Invariant(
            "sealed content text span digest disagrees with its bytes",
        ));
    }
    let source = std::str::from_utf8(source)
        .map_err(|_| SyndicReadError::Invariant("sealed content text span is not valid UTF-8"))?;
    let local_start = usize::try_from(*logical - span.logical_start())
        .map_err(|_| SyndicReadError::Invariant("sealed content text logical offset overflowed"))?;
    if !source.is_char_boundary(local_start) {
        return Err(invalid_offset(
            content.summary().logical_utf8_bytes(),
            *logical,
        ));
    }
    let selected_end = desired_end.min(span.logical_end());
    let mut local_end = usize::try_from(selected_end - span.logical_start())
        .map_err(|_| SyndicReadError::Invariant("sealed content text logical offset overflowed"))?;
    while local_end > local_start && !source.is_char_boundary(local_end) {
        local_end -= 1;
    }
    if local_end == local_start {
        return Ok(true);
    }
    output.extend_from_slice(&source.as_bytes()[local_start..local_end]);
    let local_end = u64::try_from(local_end)
        .map_err(|_| SyndicReadError::Invariant("sealed content text logical offset overflowed"))?;
    *logical = span
        .logical_start()
        .checked_add(local_end)
        .ok_or(SyndicReadError::Invariant(
            "sealed content text logical offset overflowed",
        ))?;
    Ok(*logical < selected_end)
}

fn validate_span(
    content: ContentReference,
    key: &ContentTextSpanKey,
    span: ContentTextSpanRecord,
    predecessor: bool,
    segment: Option<TextSegmentBounds>,
    logical: u64,
) -> Result<(), SyndicReadError> {
    let summary = content.summary();
    let start_agrees = if predecessor {
        span.logical_start() <= logical
    } else {
        span.logical_start() == logical
    };
    if key.owner != content.id()
        || key.logical_start != span.logical_start()
        || span.content_id() != content.id()
        || !start_agrees
        || span.logical_end() <= logical
        || span.logical_end() > summary.logical_utf8_bytes()
        || span.encoded_end() > summary.encoded_bytes()
        || span.chunk_ordinal().get() > summary.chunk_count()
        || span.piece_ordinal().get() > summary.piece_count()
    {
        return Err(SyndicReadError::Invariant(
            "sealed content text span identity or frontier disagrees",
        ));
    }
    if let Some(bounds) = segment {
        let expected_break =
            predecessor && span.logical_start() == bounds.start && bounds.break_at_start;
        if span.break_before() != expected_break {
            return Err(SyndicReadError::Invariant(
                "proven content text segment physical marker boundary disagrees",
            ));
        }
    } else if span.break_before() {
        return Err(SyndicReadError::Invariant(
            "marker-free sealed content contains a marker-separated text span",
        ));
    }
    Ok(())
}

pub(in crate::read) fn family_limit<F: Family>() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(family_point_limit::<F>().max_bytes())
        .expect("codec family point-read bound is nonzero")
}

pub(in crate::read) fn invalid_offset(content_bytes: u64, offset: u64) -> SyndicReadError {
    SyndicReadError::InvalidContentTextOffset {
        content_bytes,
        offset,
    }
}

pub(super) fn concurrent() -> SyndicReadError {
    SyndicReadError::ConcurrentChange {
        operation: CONTENT_TEXT_OPERATION,
    }
}

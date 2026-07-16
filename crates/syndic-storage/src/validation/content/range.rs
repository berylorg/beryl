use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};

use crate::{codec::*, domain::SyndicDomain, error::SyndicValidationError};

use crate::validation::scan::require;

const RANGE_MAX_BYTES: u64 = 65_536;

pub(in crate::validation) fn read_logical_range(
    reader: &DomainReader<'_, SyndicDomain>,
    content: crate::ContentReference,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, SyndicValidationError> {
    let length = bounded_length(start, end, content.summary().logical_utf8_bytes())?;
    let predecessor = reader.cursor::<ContentTextSpansCodec>(
        &CursorRange::closed(
            ContentTextSpanKey {
                owner: content.id(),
                logical_start: 0,
            },
            ContentTextSpanKey {
                owner: content.id(),
                logical_start: start,
            },
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 512).expect("logical predecessor bounds are nonzero"),
    )?;
    let first = predecessor
        .records()
        .first()
        .ok_or(SyndicValidationError::Invariant(
            "logical content range has no indexed predecessor",
        ))?;
    let first_start = first.key().logical_start;
    let mut after = None;
    let mut output = Vec::with_capacity(length);
    let mut cursor_start = start;
    loop {
        let range = match after {
            Some(previous) => CursorRange::after(
                ContentTextSpanKey {
                    owner: content.id(),
                    logical_start: previous,
                },
                ContentTextSpanKey {
                    owner: content.id(),
                    logical_start: end,
                },
            ),
            None => CursorRange::closed(
                ContentTextSpanKey {
                    owner: content.id(),
                    logical_start: first_start,
                },
                ContentTextSpanKey {
                    owner: content.id(),
                    logical_start: end,
                },
            ),
        };
        let page = reader.cursor::<ContentTextSpansCodec>(
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(256, 65_536).expect("logical range-page bounds are nonzero"),
        )?;
        if page.records().is_empty() {
            return invariant("logical content range has an indexed gap");
        }
        for record in page.records() {
            let span = record.value();
            if span.content_id() != content.id()
                || span.logical_start() > cursor_start
                || span.logical_end() <= cursor_start
            {
                return invariant("logical content range index is not contiguous");
            }
            let take_start = cursor_start.max(span.logical_start());
            let take_end = end.min(span.logical_end());
            let chunk = require::<ContentChunksFamily>(
                reader,
                &ContentChunkKey {
                    owner: content.id(),
                    ordinal: span.chunk_ordinal(),
                },
                "logical content range chunk is missing",
            )?;
            let encoded_start = span
                .encoded_start()
                .checked_add(take_start - span.logical_start())
                .ok_or(SyndicValidationError::Invariant(
                    "logical content range mapping overflowed",
                ))?;
            let encoded_end = encoded_start.checked_add(take_end - take_start).ok_or(
                SyndicValidationError::Invariant("logical content range mapping overflowed"),
            )?;
            append_chunk_range(
                &mut output,
                chunk.bytes(),
                span.chunk_start(),
                encoded_start,
                encoded_end,
                "logical content range lies outside its chunk",
            )?;
            cursor_start = take_end;
            after = Some(span.logical_start());
            if cursor_start == end {
                return Ok(output);
            }
        }
        if !page.has_more() {
            return invariant("logical content range ended before its requested frontier");
        }
    }
}

pub(super) fn read_encoded_range(
    reader: &DomainReader<'_, SyndicDomain>,
    content: beryl_model::SyndicContentId,
    committed_bytes: u64,
    start: u64,
    end: u64,
) -> Result<Vec<u8>, SyndicValidationError> {
    let length = bounded_length(start, end, committed_bytes)?;
    let predecessor = reader.cursor::<ContentByteSpansCodec>(
        &CursorRange::closed(
            ContentByteSpanKey {
                owner: content,
                start: 0,
            },
            ContentByteSpanKey {
                owner: content,
                start,
            },
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 512).expect("encoded predecessor bounds are nonzero"),
    )?;
    let first = predecessor
        .records()
        .first()
        .ok_or(SyndicValidationError::Invariant(
            "encoded content range has no indexed predecessor",
        ))?;
    let first_start = first.key().start;
    let mut after = None;
    let mut output = Vec::with_capacity(length);
    let mut cursor_start = start;
    loop {
        let range = match after {
            Some(previous) => CursorRange::after(
                ContentByteSpanKey {
                    owner: content,
                    start: previous,
                },
                ContentByteSpanKey {
                    owner: content,
                    start: end,
                },
            ),
            None => CursorRange::closed(
                ContentByteSpanKey {
                    owner: content,
                    start: first_start,
                },
                ContentByteSpanKey {
                    owner: content,
                    start: end,
                },
            ),
        };
        let page = reader.cursor::<ContentByteSpansCodec>(
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(256, 65_536).expect("encoded range-page bounds are nonzero"),
        )?;
        if page.records().is_empty() {
            return invariant("encoded content range has an indexed gap");
        }
        for record in page.records() {
            let span = record.value();
            if span.content_id() != content
                || span.start() > cursor_start
                || span.end() <= cursor_start
            {
                return invariant("encoded content range index is not contiguous");
            }
            let take_start = cursor_start.max(span.start());
            let take_end = end.min(span.end());
            let chunk = require::<ContentChunksFamily>(
                reader,
                &ContentChunkKey {
                    owner: content,
                    ordinal: span.ordinal(),
                },
                "encoded content range chunk is missing",
            )?;
            append_chunk_range(
                &mut output,
                chunk.bytes(),
                span.start(),
                take_start,
                take_end,
                "encoded content range lies outside its chunk",
            )?;
            cursor_start = take_end;
            after = Some(span.start());
            if cursor_start == end {
                return Ok(output);
            }
        }
        if !page.has_more() {
            return invariant("encoded content range ended before its requested frontier");
        }
    }
}

fn bounded_length(start: u64, end: u64, frontier: u64) -> Result<usize, SyndicValidationError> {
    let length = end
        .checked_sub(start)
        .ok_or(SyndicValidationError::Invariant(
            "content range is reversed",
        ))?;
    if length == 0 || length > RANGE_MAX_BYTES || end > frontier {
        return invariant("content range exceeds its bounded source");
    }
    usize::try_from(length)
        .map_err(|_| SyndicValidationError::Invariant("content range length overflowed"))
}

fn append_chunk_range(
    output: &mut Vec<u8>,
    chunk: &[u8],
    chunk_start: u64,
    start: u64,
    end: u64,
    outside: &'static str,
) -> Result<(), SyndicValidationError> {
    let local_start = usize::try_from(start - chunk_start)
        .map_err(|_| SyndicValidationError::Invariant("content range offset overflowed"))?;
    let local_end = usize::try_from(end - chunk_start)
        .map_err(|_| SyndicValidationError::Invariant("content range offset overflowed"))?;
    output.extend_from_slice(
        chunk
            .get(local_start..local_end)
            .ok_or(SyndicValidationError::Invariant(outside))?,
    );
    Ok(())
}

fn invariant<T>(message: &'static str) -> Result<T, SyndicValidationError> {
    Err(SyndicValidationError::Invariant(message))
}

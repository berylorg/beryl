use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};

use crate::{
    ContentByteSpanRecord, ContentChunkRecord, ProviderNarrativeReference,
    ProviderNarrativeSpanRecord, SyndicPointReadLimit, SyndicReadError, codec::*,
    domain::SyndicStorage,
};

use super::ReadByteTotals;

pub(super) fn read_provider_narrative_bytes_into(
    storage: &SyndicStorage,
    store: &HomeStore,
    narrative: ProviderNarrativeReference,
    start: u64,
    end: u64,
    output: &mut [u8],
) -> Result<ReadByteTotals, SyndicReadError> {
    if start >= end || end > narrative.logical_utf8_bytes() {
        return Err(SyndicReadError::Invariant(
            "resource range exceeds its provider narrative frontier",
        ));
    }
    let manifest = storage
        .point::<ContentManifestsFamily>(
            store,
            narrative.content_id(),
            SyndicPointReadLimit::new(4_096)
                .expect("provider manifest point-read bound is nonzero"),
        )?
        .ok_or(SyndicReadError::Invariant(
            "provider narrative content manifest is missing",
        ))?;
    if manifest.id() != narrative.content_id()
        || manifest.encoding() != crate::ContentEncoding::ProviderItemV1
        || manifest.current_reference().is_none()
    {
        return Err(SyndicReadError::Invariant(
            "provider narrative has no readable ProviderItemV1 content",
        ));
    }
    let encoded_frontier = manifest.encoded_bytes();
    let predecessor = store
        .read_cursor::<crate::domain::SyndicDomain, ProviderNarrativeSpansCodec>(
            storage.handle,
            &CursorRange::closed(
                ProviderNarrativeSpanKey::first_for_generation(
                    narrative.content_id(),
                    narrative.generation(),
                ),
                ProviderNarrativeSpanKey::new(
                    narrative.content_id(),
                    narrative.generation(),
                    start,
                ),
            ),
            CursorDirection::Reverse,
            CursorReadLimits::new(1, 512).expect("provider narrative predecessor bound is nonzero"),
        )?;
    let mut totals = ReadByteTotals::new(predecessor.stored_bytes(), predecessor.decoded_bytes());
    let Some(first) = predecessor.records().first() else {
        return Err(SyndicReadError::Invariant(
            "resource range has no provider narrative predecessor",
        ));
    };
    validate_provider_span_record(narrative, *first.key(), *first.value())?;
    let mut pending = Some(*first.value());
    let mut after_start = first.value().logical_start();
    let mut logical = start;
    if usize::try_from(end - start).ok() != Some(output.len()) {
        return Err(SyndicReadError::Invariant(
            "caller storage disagrees with the provider narrative range",
        ));
    }
    let mut written = 0;
    let mut cached_chunk: Option<ContentChunkRecord> = None;

    while logical < end {
        if let Some(span) = pending.take() {
            append_provider_narrative_span(
                storage,
                store,
                narrative,
                encoded_frontier,
                span,
                &mut cached_chunk,
                &mut logical,
                end,
                output,
                &mut written,
                &mut totals,
            )?;
            if logical == end {
                break;
            }
        }
        let page = store.read_cursor::<crate::domain::SyndicDomain, ProviderNarrativeSpansCodec>(
            storage.handle,
            &CursorRange::after(
                ProviderNarrativeSpanKey::new(
                    narrative.content_id(),
                    narrative.generation(),
                    after_start,
                ),
                ProviderNarrativeSpanKey::new(
                    narrative.content_id(),
                    narrative.generation(),
                    end - 1,
                ),
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(256, crate::TRANSCRIPT_PAGE_MAX_BYTES)
                .expect("provider narrative range-page bounds are nonzero"),
        )?;
        totals.add(
            page.stored_bytes(),
            page.decoded_bytes(),
            "resource range byte accounting overflowed",
        )?;
        if page.records().is_empty() {
            return Err(SyndicReadError::Invariant(
                "resource range has a provider narrative gap",
            ));
        }
        for record in page.records() {
            validate_provider_span_record(narrative, *record.key(), *record.value())?;
            let span = *record.value();
            after_start = span.logical_start();
            append_provider_narrative_span(
                storage,
                store,
                narrative,
                encoded_frontier,
                span,
                &mut cached_chunk,
                &mut logical,
                end,
                output,
                &mut written,
                &mut totals,
            )?;
            if logical == end {
                break;
            }
        }
    }
    if written != output.len() {
        return Err(SyndicReadError::Invariant(
            "resource range returned an incomplete provider narrative window",
        ));
    }
    Ok(totals)
}

fn validate_provider_span_record(
    narrative: ProviderNarrativeReference,
    key: ProviderNarrativeSpanKey,
    span: ProviderNarrativeSpanRecord,
) -> Result<(), SyndicReadError> {
    if key.content_id() != narrative.content_id()
        || key.generation() != narrative.generation()
        || key.logical_start() != span.logical_start()
        || span.content_id() != narrative.content_id()
        || span.generation() != narrative.generation()
        || span.logical_end() > narrative.logical_utf8_bytes()
    {
        return Err(SyndicReadError::Invariant(
            "provider narrative span exceeds its stored reference",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_provider_narrative_span(
    storage: &SyndicStorage,
    store: &HomeStore,
    narrative: ProviderNarrativeReference,
    encoded_frontier: u64,
    span: ProviderNarrativeSpanRecord,
    cached_chunk: &mut Option<ContentChunkRecord>,
    logical: &mut u64,
    end: u64,
    output: &mut [u8],
    written: &mut usize,
    totals: &mut ReadByteTotals,
) -> Result<(), SyndicReadError> {
    if span.logical_start() > *logical
        || span.logical_end() <= *logical
        || span.logical_end() > narrative.logical_utf8_bytes()
    {
        return Err(SyndicReadError::Invariant(
            "resource range provider narrative spans are not contiguous",
        ));
    }
    let selected_end = end.min(span.logical_end());
    let source_start = span
        .source_start()
        .checked_add(*logical - span.logical_start())
        .ok_or(SyndicReadError::Invariant(
            "provider narrative source offset overflowed",
        ))?;
    let source_end =
        source_start
            .checked_add(selected_end - *logical)
            .ok_or(SyndicReadError::Invariant(
                "provider narrative source offset overflowed",
            ))?;
    if source_end > span.source_end() || source_end > encoded_frontier {
        return Err(SyndicReadError::Invariant(
            "provider narrative span exceeds its ProviderItemV1 content",
        ));
    }
    append_provider_encoded_range(
        storage,
        store,
        narrative.content_id(),
        encoded_frontier,
        source_start,
        source_end,
        cached_chunk,
        output,
        written,
        totals,
    )?;
    *logical = selected_end;
    if *logical == narrative.logical_utf8_bytes()
        && span.logical_end() == narrative.logical_utf8_bytes()
        && span.resulting_chain_digest() != narrative.chain_digest()
    {
        return Err(SyndicReadError::Invariant(
            "provider narrative terminal chain digest disagrees with its reference",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_provider_encoded_range(
    storage: &SyndicStorage,
    store: &HomeStore,
    content_id: beryl_model::SyndicContentId,
    encoded_frontier: u64,
    start: u64,
    end: u64,
    cached_chunk: &mut Option<ContentChunkRecord>,
    output: &mut [u8],
    written: &mut usize,
    totals: &mut ReadByteTotals,
) -> Result<(), SyndicReadError> {
    if start >= end || end > encoded_frontier {
        return Err(SyndicReadError::Invariant(
            "provider narrative encoded range exceeds committed content",
        ));
    }
    let predecessor = store.read_cursor::<crate::domain::SyndicDomain, ContentByteSpansCodec>(
        storage.handle,
        &CursorRange::closed(
            ContentByteSpanKey {
                owner: content_id,
                start: 0,
            },
            ContentByteSpanKey {
                owner: content_id,
                start,
            },
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 512).expect("provider encoded-range predecessor bound is nonzero"),
    )?;
    totals.add(
        predecessor.stored_bytes(),
        predecessor.decoded_bytes(),
        "resource range byte accounting overflowed",
    )?;
    let Some(first) = predecessor.records().first() else {
        return Err(SyndicReadError::Invariant(
            "provider narrative encoded range has no byte-span predecessor",
        ));
    };
    if first.key().owner != content_id || first.key().start != first.value().start() {
        return Err(SyndicReadError::Invariant(
            "provider narrative byte-span key disagrees with its record",
        ));
    }
    let mut pending = Some(*first.value());
    let mut after_start = first.value().start();
    let mut encoded = start;
    while encoded < end {
        if let Some(span) = pending.take() {
            append_provider_byte_span(
                storage,
                store,
                content_id,
                span,
                cached_chunk,
                &mut encoded,
                end,
                output,
                written,
                totals,
            )?;
            if encoded == end {
                break;
            }
        }
        let page = store.read_cursor::<crate::domain::SyndicDomain, ContentByteSpansCodec>(
            storage.handle,
            &CursorRange::after(
                ContentByteSpanKey {
                    owner: content_id,
                    start: after_start,
                },
                ContentByteSpanKey {
                    owner: content_id,
                    start: end - 1,
                },
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(256, crate::TRANSCRIPT_PAGE_MAX_BYTES)
                .expect("provider encoded-range page bounds are nonzero"),
        )?;
        totals.add(
            page.stored_bytes(),
            page.decoded_bytes(),
            "resource range byte accounting overflowed",
        )?;
        if page.records().is_empty() {
            return Err(SyndicReadError::Invariant(
                "provider narrative encoded range has a byte-span gap",
            ));
        }
        for record in page.records() {
            let span = *record.value();
            if record.key().owner != content_id || record.key().start != span.start() {
                return Err(SyndicReadError::Invariant(
                    "provider narrative byte-span key disagrees with its record",
                ));
            }
            after_start = span.start();
            append_provider_byte_span(
                storage,
                store,
                content_id,
                span,
                cached_chunk,
                &mut encoded,
                end,
                output,
                written,
                totals,
            )?;
            if encoded == end {
                break;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_provider_byte_span(
    storage: &SyndicStorage,
    store: &HomeStore,
    content_id: beryl_model::SyndicContentId,
    span: ContentByteSpanRecord,
    cached_chunk: &mut Option<ContentChunkRecord>,
    encoded: &mut u64,
    end: u64,
    output: &mut [u8],
    written: &mut usize,
    totals: &mut ReadByteTotals,
) -> Result<(), SyndicReadError> {
    if span.content_id() != content_id || span.start() > *encoded || span.end() <= *encoded {
        return Err(SyndicReadError::Invariant(
            "provider narrative content byte spans are not contiguous",
        ));
    }
    if cached_chunk
        .as_ref()
        .is_none_or(|chunk| chunk.content_id() != content_id || chunk.ordinal() != span.ordinal())
    {
        let key = ContentChunkKey {
            owner: content_id,
            ordinal: span.ordinal(),
        };
        let page = storage.page::<ContentChunksFamily>(
            store,
            CursorRange::closed(key.clone(), key),
            CursorReadLimits::new(1, crate::CONTENT_CHUNK_MAX_BYTES + 512)
                .expect("provider content chunk cursor bound is nonzero"),
        )?;
        totals.add(
            page.stored_bytes(),
            page.decoded_bytes(),
            "resource range byte accounting overflowed",
        )?;
        let chunk = page
            .into_records()
            .into_iter()
            .next()
            .ok_or(SyndicReadError::Invariant(
                "provider narrative content chunk is missing",
            ))?;
        *cached_chunk = Some(chunk);
    }
    let chunk = cached_chunk
        .as_ref()
        .expect("provider range loads its current chunk");
    if chunk.content_id() != content_id
        || chunk.ordinal() != span.ordinal()
        || chunk.digest() != &span.chunk_digest()
        || u64::try_from(chunk.bytes().len()).ok() != Some(span.len())
    {
        return Err(SyndicReadError::Invariant(
            "provider narrative chunk disagrees with its byte span",
        ));
    }
    let selected_end = end.min(span.end());
    let local_start = usize::try_from(*encoded - span.start())
        .map_err(|_| SyndicReadError::Invariant("provider chunk offset overflowed"))?;
    let local_end = usize::try_from(selected_end - span.start())
        .map_err(|_| SyndicReadError::Invariant("provider chunk offset overflowed"))?;
    let bytes = chunk
        .bytes()
        .get(local_start..local_end)
        .ok_or(SyndicReadError::Invariant(
            "provider narrative range lies outside its content chunk",
        ))?;
    super::append_output_bytes(output, written, bytes)?;
    *encoded = selected_end;
    Ok(())
}

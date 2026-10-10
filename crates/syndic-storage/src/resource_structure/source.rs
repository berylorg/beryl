use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};
use beryl_model::SyndicContentId;

use super::{Binding, ByteRange, construction::ConstructionError, ledger::Ledger};
use crate::{
    ContentChunkRecord, ProjectionTextSource,
    codec::{
        ContentByteSpanKey, ContentByteSpansFamily, ContentChunkKey, ContentChunksFamily,
        ContentTextSpanKey, ContentTextSpansFamily, ExactCodec, Family, ProviderNarrativeSpanKey,
        ProviderNarrativeSpansFamily,
    },
    domain::SyndicDomain,
};

pub(super) trait SourceRecords {
    fn predecessor<F: Family>(
        &self,
        first: F::Key,
        last: F::Key,
    ) -> Result<(F::Key, F::Value), ConstructionError>;

    fn chunk(&self, key: &ContentChunkKey) -> Result<ContentChunkRecord, ConstructionError>;
}

impl SourceRecords for DomainReader<'_, SyndicDomain> {
    fn predecessor<F: Family>(
        &self,
        first: F::Key,
        last: F::Key,
    ) -> Result<(F::Key, F::Value), ConstructionError> {
        let page = self.cursor::<ExactCodec<F>>(
            &CursorRange::closed(first, last),
            CursorDirection::Reverse,
            CursorReadLimits::new(1, 512).map_err(|_| ConstructionError::Limit)?,
        )?;
        let record = page
            .into_records()
            .into_iter()
            .next()
            .ok_or(ConstructionError::Corrupt)?;
        Ok(record.into_parts())
    }

    fn chunk(&self, key: &ContentChunkKey) -> Result<ContentChunkRecord, ConstructionError> {
        self.point::<ExactCodec<ContentChunksFamily>>(
            key,
            beryl_home_store::PointReadLimit::new(
                ContentChunksFamily::MAX_VALUE_BYTES + beryl_home_store::RECORD_VERSION_BYTES,
            )
            .map_err(|_| ConstructionError::Limit)?,
        )?
        .ok_or(ConstructionError::Corrupt)
    }
}

fn predecessor<F: Family>(
    records: &impl SourceRecords,
    ledger: &mut Ledger,
    first: F::Key,
    last: F::Key,
) -> Result<(F::Key, F::Value), ConstructionError> {
    ledger.reserve_read(1, 512, 512)?;
    records.predecessor::<F>(first, last)
}

struct WindowReader<'a, R> {
    records: &'a R,
    ledger: &'a mut Ledger,
    content: SyndicContentId,
    encoded_frontier: u64,
    cache: Option<ContentChunkRecord>,
    output: Vec<u8>,
}

impl<R: SourceRecords> WindowReader<'_, R> {
    fn encoded(
        &mut self,
        range: ByteRange,
        composer: Option<(crate::ContentChunkOrdinal, u64)>,
    ) -> Result<(), ConstructionError> {
        if !range.is_valid() || range.end > self.encoded_frontier {
            return Err(ConstructionError::Corrupt);
        }
        let mut position = range.start;
        while position < range.end {
            let (key, span) = predecessor::<ContentByteSpansFamily>(
                self.records,
                self.ledger,
                ContentByteSpanKey {
                    owner: self.content,
                    start: 0,
                },
                ContentByteSpanKey {
                    owner: self.content,
                    start: position,
                },
            )?;
            if key.owner != self.content
                || key.start != span.start()
                || span.content_id() != self.content
                || span.start() > position
                || span.end() <= position
                || span.end() > self.encoded_frontier
                || composer.is_some_and(|expected| expected != (span.ordinal(), span.start()))
            {
                return Err(ConstructionError::Corrupt);
            }
            if self
                .cache
                .as_ref()
                .is_none_or(|chunk| chunk.ordinal() != span.ordinal())
            {
                let bytes = ContentChunksFamily::MAX_VALUE_BYTES
                    + ContentChunksFamily::MAX_KEY_BYTES
                    + beryl_home_store::RECORD_VERSION_BYTES;
                self.ledger.reserve_read(1, bytes, bytes)?;
                self.cache = Some(self.records.chunk(&ContentChunkKey {
                    owner: self.content,
                    ordinal: span.ordinal(),
                })?);
            }
            let chunk = self.cache.as_ref().ok_or(ConstructionError::Corrupt)?;
            if chunk.content_id() != self.content
                || chunk.ordinal() != span.ordinal()
                || chunk.digest() != &span.chunk_digest()
                || u64::try_from(chunk.bytes().len()).ok() != Some(span.len())
            {
                return Err(ConstructionError::Corrupt);
            }
            let end = range.end.min(span.end());
            let local_start =
                usize::try_from(position - span.start()).map_err(|_| ConstructionError::Limit)?;
            let local_end =
                usize::try_from(end - span.start()).map_err(|_| ConstructionError::Limit)?;
            let bytes = chunk
                .bytes()
                .get(local_start..local_end)
                .ok_or(ConstructionError::Corrupt)?;
            self.output.extend_from_slice(bytes);
            position = end;
        }
        Ok(())
    }
}

pub(super) fn read_window(
    reader: &DomainReader<'_, SyndicDomain>,
    ledger: &mut Ledger,
    binding: &Binding,
    relative_start: u64,
    length: usize,
) -> Result<Vec<u8>, ConstructionError> {
    let encoded_frontier = super::construction::authenticate_source(reader, ledger, binding)?;
    read_authenticated_window(
        reader,
        ledger,
        binding,
        encoded_frontier,
        relative_start,
        length,
    )
}

pub(super) fn read_authenticated_window(
    records: &impl SourceRecords,
    ledger: &mut Ledger,
    binding: &Binding,
    encoded_frontier: u64,
    relative_start: u64,
    length: usize,
) -> Result<Vec<u8>, ConstructionError> {
    if length > 65_536 || !binding.range.is_valid() {
        return Err(ConstructionError::Limit);
    }
    let start = binding
        .range
        .start
        .checked_add(relative_start)
        .ok_or(ConstructionError::Limit)?;
    let end = start
        .checked_add(length as u64)
        .ok_or(ConstructionError::Limit)?;
    if !binding.range.contains(ByteRange { start, end })
        || end > binding.source.logical_utf8_bytes()
    {
        return Err(ConstructionError::Unavailable);
    }
    ledger.reserve_read(0, 0, length)?;
    let mut window = WindowReader {
        records,
        ledger,
        content: binding.source.content_id(),
        encoded_frontier,
        cache: None,
        output: Vec::with_capacity(length),
    };
    let mut logical = start;
    while logical < end {
        let (logical_start, logical_end, encoded_start, encoded_end, composer) = match binding
            .source
        {
            ProjectionTextSource::Composer(_) => {
                let (key, span) = predecessor::<ContentTextSpansFamily>(
                    records,
                    window.ledger,
                    ContentTextSpanKey {
                        owner: window.content,
                        logical_start: 0,
                    },
                    ContentTextSpanKey {
                        owner: window.content,
                        logical_start: logical,
                    },
                )?;
                if key.owner != window.content
                    || key.logical_start != span.logical_start()
                    || span.content_id() != window.content
                {
                    return Err(ConstructionError::Corrupt);
                }
                (
                    span.logical_start(),
                    span.logical_end(),
                    span.encoded_start(),
                    span.encoded_end(),
                    Some((span.chunk_ordinal(), span.chunk_start())),
                )
            }
            ProjectionTextSource::ProviderNarrative(narrative) => {
                let (key, span) = predecessor::<ProviderNarrativeSpansFamily>(
                    records,
                    window.ledger,
                    ProviderNarrativeSpanKey::first_for_generation(
                        window.content,
                        narrative.generation(),
                    ),
                    ProviderNarrativeSpanKey::new(window.content, narrative.generation(), logical),
                )?;
                if key.content_id() != window.content
                    || key.generation() != narrative.generation()
                    || key.logical_start() != span.logical_start()
                    || span.content_id() != window.content
                    || span.generation() != narrative.generation()
                    || (span.logical_end() == narrative.logical_utf8_bytes()
                        && span.resulting_chain_digest() != narrative.chain_digest())
                {
                    return Err(ConstructionError::Corrupt);
                }
                (
                    span.logical_start(),
                    span.logical_end(),
                    span.source_start(),
                    span.source_end(),
                    None,
                )
            }
        };
        if logical_start > logical
            || logical_end <= logical
            || logical_end > binding.source.logical_utf8_bytes()
            || encoded_start >= encoded_end
            || encoded_end > encoded_frontier
            || logical_end - logical_start != encoded_end - encoded_start
        {
            return Err(ConstructionError::Corrupt);
        }
        let next = end.min(logical_end);
        let selected_start = encoded_start
            .checked_add(logical - logical_start)
            .ok_or(ConstructionError::Limit)?;
        let selected_end = selected_start
            .checked_add(next - logical)
            .ok_or(ConstructionError::Limit)?;
        window.encoded(
            ByteRange {
                start: selected_start,
                end: selected_end,
            },
            composer,
        )?;
        logical = next;
    }
    if window.output.len() != length {
        return Err(ConstructionError::Corrupt);
    }
    Ok(window.output)
}

#[cfg(test)]
#[path = "../../tests/unit/resource_structure_source.rs"]
mod tests;

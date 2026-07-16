use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use beryl_model::SyndicResourceId;

use crate::{
    ContentChunkRecord, ContentTextSpanRecord, ResourceBacking, SyndicReadError, codec::*,
    domain::SyndicStorage,
};

use super::SyndicPointReadLimit;

/// One bounded slice of immutable textual resource bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicResourceRangeRead {
    resource_id: SyndicResourceId,
    start: u64,
    bytes: Box<[u8]>,
    next_offset: Option<u64>,
    stored_bytes: usize,
}

impl SyndicResourceRangeRead {
    #[must_use]
    pub const fn resource_id(&self) -> SyndicResourceId {
        self.resource_id
    }

    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Resource-relative offset at which the caller can continue the same request.
    #[must_use]
    pub const fn next_offset(&self) -> Option<u64> {
        self.next_offset
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
}

impl SyndicStorage {
    /// Reads a bounded resource-relative half-open byte range.
    ///
    /// `max_payload_bytes` must be in `1..=65_536`. A response that reaches that bound before
    /// `end` supplies the exact resource-relative continuation offset.
    pub fn resource_text_range(
        &self,
        store: &HomeStore,
        resource_id: SyndicResourceId,
        start: u64,
        end: u64,
        max_payload_bytes: usize,
    ) -> Result<Option<SyndicResourceRangeRead>, SyndicReadError> {
        if max_payload_bytes == 0 || max_payload_bytes > crate::TRANSCRIPT_PAGE_MAX_BYTES {
            return Err(SyndicReadError::InvalidResourceReadLimit {
                maximum: crate::TRANSCRIPT_PAGE_MAX_BYTES,
                actual: max_payload_bytes,
            });
        }
        let point_limit = SyndicPointReadLimit::new(4_096)
            .expect("resource metadata point-read bound is nonzero");
        let Some(resource) = self.resource(store, resource_id, point_limit)? else {
            return Ok(None);
        };
        let metadata = resource.record();
        let ResourceBacking::CanonicalTextRange { content_id, range } = metadata.backing() else {
            return Err(SyndicReadError::ResourceHasNoTextBacking);
        };
        let resource_bytes = metadata.byte_length().ok_or(SyndicReadError::Invariant(
            "text-backed resource omitted its byte length",
        ))?;
        if start > end || end > resource_bytes {
            return Err(SyndicReadError::InvalidResourceRange {
                resource_bytes,
                start,
                end,
            });
        }
        let mut stored_bytes = resource.stored_bytes();
        if start == end {
            return Ok(Some(SyndicResourceRangeRead {
                resource_id,
                start,
                bytes: Box::default(),
                next_offset: None,
                stored_bytes,
            }));
        }
        let absolute_start = range
            .start()
            .checked_add(start)
            .ok_or(SyndicReadError::Invariant(
                "resource range offset overflowed",
            ))?;
        let requested = end - start;
        let returned = requested.min(max_payload_bytes as u64);
        let absolute_end =
            absolute_start
                .checked_add(returned)
                .ok_or(SyndicReadError::Invariant(
                    "resource range offset overflowed",
                ))?;
        let (bytes, range_stored_bytes) =
            read_logical_bytes(self, store, content_id, absolute_start, absolute_end)?;
        stored_bytes =
            stored_bytes
                .checked_add(range_stored_bytes)
                .ok_or(SyndicReadError::Invariant(
                    "resource range stored-byte accounting overflowed",
                ))?;
        let next = start
            .checked_add(returned)
            .ok_or(SyndicReadError::Invariant(
                "resource continuation overflowed",
            ))?;
        Ok(Some(SyndicResourceRangeRead {
            resource_id,
            start,
            bytes: bytes.into_boxed_slice(),
            next_offset: (next < end).then_some(next),
            stored_bytes,
        }))
    }
}

fn read_logical_bytes(
    storage: &SyndicStorage,
    store: &HomeStore,
    content_id: beryl_model::SyndicContentId,
    start: u64,
    end: u64,
) -> Result<(Vec<u8>, usize), SyndicReadError> {
    let predecessor = store.read_cursor::<crate::domain::SyndicDomain, ContentTextSpansCodec>(
        storage.handle,
        &CursorRange::closed(
            ContentTextSpanKey {
                owner: content_id,
                logical_start: 0,
            },
            ContentTextSpanKey {
                owner: content_id,
                logical_start: start,
            },
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 512).expect("logical predecessor bound is nonzero"),
    )?;
    let mut stored_bytes = predecessor.stored_bytes();
    let Some(first) = predecessor.records().first() else {
        return Err(SyndicReadError::Invariant(
            "resource range has no indexed text predecessor",
        ));
    };
    let mut pending = Some(*first.value());
    let mut after_start = first.value().logical_start();
    let mut logical = start;
    let capacity = usize::try_from(end - start)
        .map_err(|_| SyndicReadError::Invariant("resource range length overflowed"))?;
    let mut output = Vec::with_capacity(capacity);
    let mut cached_chunk: Option<ContentChunkRecord> = None;

    while logical < end {
        if let Some(span) = pending.take() {
            append_span(
                storage,
                store,
                span,
                &mut cached_chunk,
                &mut logical,
                end,
                &mut output,
                &mut stored_bytes,
            )?;
            if logical == end {
                break;
            }
        }

        let page = store.read_cursor::<crate::domain::SyndicDomain, ContentTextSpansCodec>(
            storage.handle,
            &CursorRange::after(
                ContentTextSpanKey {
                    owner: content_id,
                    logical_start: after_start,
                },
                ContentTextSpanKey {
                    owner: content_id,
                    logical_start: end,
                },
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(256, 65_536)
                .expect("logical range index-page bounds are nonzero"),
        )?;
        stored_bytes =
            stored_bytes
                .checked_add(page.stored_bytes())
                .ok_or(SyndicReadError::Invariant(
                    "resource range stored-byte accounting overflowed",
                ))?;
        if page.records().is_empty() {
            return Err(SyndicReadError::Invariant(
                "resource range has an indexed text gap",
            ));
        }
        for record in page.records() {
            let span = *record.value();
            after_start = span.logical_start();
            append_span(
                storage,
                store,
                span,
                &mut cached_chunk,
                &mut logical,
                end,
                &mut output,
                &mut stored_bytes,
            )?;
            if logical == end {
                break;
            }
        }
    }
    if output.len() != capacity {
        return Err(SyndicReadError::Invariant(
            "resource range returned an incomplete logical byte window",
        ));
    }
    Ok((output, stored_bytes))
}

#[allow(clippy::too_many_arguments)]
fn append_span(
    storage: &SyndicStorage,
    store: &HomeStore,
    span: ContentTextSpanRecord,
    cached_chunk: &mut Option<ContentChunkRecord>,
    logical: &mut u64,
    end: u64,
    output: &mut Vec<u8>,
    stored_bytes: &mut usize,
) -> Result<(), SyndicReadError> {
    if span.logical_start() > *logical || span.logical_end() <= *logical {
        return Err(SyndicReadError::Invariant(
            "resource range text spans are not contiguous",
        ));
    }
    if cached_chunk
        .as_ref()
        .is_none_or(|chunk| chunk.ordinal() != span.chunk_ordinal())
    {
        let limit = SyndicPointReadLimit::new(crate::CONTENT_CHUNK_MAX_BYTES + 512)
            .expect("content chunk point-read bound is nonzero");
        let chunk = storage
            .point::<ContentChunksFamily>(
                store,
                ContentChunkKey {
                    owner: span.content_id(),
                    ordinal: span.chunk_ordinal(),
                },
                limit,
            )?
            .ok_or(SyndicReadError::Invariant(
                "resource range content chunk is missing",
            ))?;
        *stored_bytes =
            stored_bytes
                .checked_add(chunk.stored_bytes())
                .ok_or(SyndicReadError::Invariant(
                    "resource range stored-byte accounting overflowed",
                ))?;
        *cached_chunk = Some(chunk.record().clone());
    }
    let chunk = cached_chunk
        .as_ref()
        .expect("resource range loads its current chunk");
    if chunk.content_id() != span.content_id() || chunk.ordinal() != span.chunk_ordinal() {
        return Err(SyndicReadError::Invariant(
            "resource range chunk identity disagrees with its span",
        ));
    }
    let selected_end = end.min(span.logical_end());
    let encoded_start = span
        .encoded_start()
        .checked_add(*logical - span.logical_start())
        .ok_or(SyndicReadError::Invariant(
            "resource range encoded offset overflowed",
        ))?;
    let encoded_end =
        encoded_start
            .checked_add(selected_end - *logical)
            .ok_or(SyndicReadError::Invariant(
                "resource range encoded offset overflowed",
            ))?;
    let local_start = usize::try_from(encoded_start - span.chunk_start())
        .map_err(|_| SyndicReadError::Invariant("resource range chunk offset overflowed"))?;
    let local_end = usize::try_from(encoded_end - span.chunk_start())
        .map_err(|_| SyndicReadError::Invariant("resource range chunk offset overflowed"))?;
    let bytes = chunk
        .bytes()
        .get(local_start..local_end)
        .ok_or(SyndicReadError::Invariant(
            "resource range lies outside its content chunk",
        ))?;
    output.extend_from_slice(bytes);
    *logical = selected_end;
    Ok(())
}

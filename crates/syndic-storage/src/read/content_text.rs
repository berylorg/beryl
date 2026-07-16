use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use sha2::{Digest, Sha256};

use crate::{
    ContentChunkOrdinal, ContentChunkRecord, ContentLifecycle, ContentManifestRecord,
    ContentReference, ContentTextSpanRecord, SyndicReadError,
    codec::{
        ContentChunkKey, ContentChunksFamily, ContentManifestsFamily, ContentTextSpanKey,
        ContentTextSpansCodec, Family, family_point_limit,
    },
    content::input_marker_digest,
    domain::SyndicStorage,
};

use super::SyndicPointReadLimit;

pub(super) const CONTENT_TEXT_MAX_PAYLOAD_BYTES: usize = 65_536;
const CONTENT_TEXT_INDEX_PAGE_ITEMS: usize = 256;
const CONTENT_TEXT_INDEX_PAGE_BYTES: usize = 65_536;
const CONTENT_TEXT_OPERATION: &str = "sealed-content text-range read";

/// One bounded UTF-8 page from an exact immutable logical content value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicContentTextRangeRead {
    content: ContentReference,
    start: u64,
    text: Box<str>,
    next_offset: Option<u64>,
    stored_bytes: usize,
}

impl SyndicContentTextRangeRead {
    pub(super) fn new(
        content: ContentReference,
        start: u64,
        text: Box<str>,
        next_offset: Option<u64>,
        stored_bytes: usize,
    ) -> Self {
        Self {
            content,
            start,
            text,
            next_offset,
            stored_bytes,
        }
    }

    /// Returns the exact sealed content reference read by this page.
    #[must_use]
    pub const fn content(&self) -> ContentReference {
        self.content
    }

    /// Returns this page's logical UTF-8 start offset.
    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }

    /// Returns the bounded logical text payload.
    #[must_use]
    pub const fn text(&self) -> &str {
        &self.text
    }

    /// Returns the exact logical UTF-8 offset for the next page, when any.
    #[must_use]
    pub const fn next_offset(&self) -> Option<u64> {
        self.next_offset
    }

    /// Returns all key-and-value bytes read to stabilize and assemble this page.
    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }
}

impl SyndicStorage {
    /// Reads one bounded logical UTF-8 page from an exact sealed content reference.
    ///
    /// `start` is an absolute logical UTF-8 byte offset and must be a character boundary.
    /// `max_payload_bytes` must be in `1..=65_536`. A nonterminal page ends at the last UTF-8
    /// boundary that fits that ceiling and supplies its exact continuation offset. Valid content
    /// containing any image marker is rejected because this boundary returns text only.
    pub fn sealed_content_text_range(
        &self,
        store: &HomeStore,
        content: ContentReference,
        start: u64,
        max_payload_bytes: usize,
    ) -> Result<Option<SyndicContentTextRangeRead>, SyndicReadError> {
        if max_payload_bytes == 0 || max_payload_bytes > CONTENT_TEXT_MAX_PAYLOAD_BYTES {
            return Err(SyndicReadError::InvalidContentTextReadLimit {
                maximum: CONTENT_TEXT_MAX_PAYLOAD_BYTES,
                actual: max_payload_bytes,
            });
        }

        let manifest_limit = family_limit::<ContentManifestsFamily>();
        let Some(first) = self.content_manifest(store, content.id(), manifest_limit)? else {
            return match self.content_manifest(store, content.id(), manifest_limit)? {
                None => Ok(None),
                Some(_) => Err(concurrent()),
            };
        };
        validate_manifest(first.record(), content)?;
        let content_bytes = content.summary().logical_utf8_bytes();
        if start > content_bytes {
            return Err(invalid_offset(content_bytes, start));
        }

        let (bytes, end, range_stored_bytes) = if start == content_bytes {
            (Vec::new(), start, 0)
        } else {
            let payload_bytes_u64 = u64::try_from(max_payload_bytes)
                .expect("the fixed content text payload bound fits u64");
            let desired_end = start.saturating_add(payload_bytes_u64).min(content_bytes);
            read_text_page(self, store, content, start, desired_end, max_payload_bytes)?
        };

        let second = self
            .content_manifest(store, content.id(), manifest_limit)?
            .ok_or_else(concurrent)?;
        if second.record() != first.record() {
            return Err(concurrent());
        }
        validate_manifest(second.record(), content)?;
        let stored_bytes = first
            .stored_bytes()
            .checked_add(range_stored_bytes)
            .and_then(|value| value.checked_add(second.stored_bytes()))
            .ok_or(SyndicReadError::Invariant(
                "logical content text stored-byte accounting overflowed",
            ))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| {
                SyndicReadError::Invariant("logical content text page is not valid UTF-8")
            })?
            .into_boxed_str();
        Ok(Some(SyndicContentTextRangeRead {
            content,
            start,
            text,
            next_offset: (end < content_bytes).then_some(end),
            stored_bytes,
        }))
    }
}

fn validate_manifest(
    manifest: &ContentManifestRecord,
    content: ContentReference,
) -> Result<(), SyndicReadError> {
    let summary = content.summary();
    if manifest.id() != content.id()
        || manifest.revision() != content.revision()
        || manifest.encoding() != content.encoding()
        || manifest.expected() != summary
    {
        return Err(SyndicReadError::Invariant(
            "sealed content reference disagrees with its exact manifest",
        ));
    }
    if manifest.lifecycle() != ContentLifecycle::Sealed {
        return Err(SyndicReadError::ContentTextRequiresSealed);
    }
    if manifest.owner().is_some()
        || manifest.sealed_reference() != Some(content)
        || manifest.chunk_count() != summary.chunk_count()
        || manifest.encoded_bytes() != summary.encoded_bytes()
        || manifest.chain_digest() != summary.digest()
        || content.id() != beryl_model::SyndicContentId::from_digest(*summary.digest().as_bytes())
    {
        return Err(SyndicReadError::Invariant(
            "sealed content reference disagrees with its exact manifest",
        ));
    }
    if summary.image_marker_count() != 0 {
        return Err(SyndicReadError::ContentTextContainsImageMarkers {
            actual: summary.image_marker_count(),
        });
    }
    if summary.marker_digest() != input_marker_digest(std::iter::empty()) {
        return Err(SyndicReadError::Invariant(
            "marker-free content has a nonempty marker digest",
        ));
    }
    Ok(())
}

pub(super) fn read_text_page(
    storage: &SyndicStorage,
    store: &HomeStore,
    content: ContentReference,
    start: u64,
    desired_end: u64,
    max_payload_bytes: usize,
) -> Result<(Vec<u8>, u64, usize), SyndicReadError> {
    let predecessor = store.read_cursor::<crate::domain::SyndicDomain, ContentTextSpansCodec>(
        storage.handle,
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
        CursorReadLimits::new(1, 512).expect("text predecessor bounds are nonzero"),
    )?;
    let mut stored_bytes = predecessor.stored_bytes();
    let Some(first) = predecessor.records().first() else {
        return Err(SyndicReadError::Invariant(
            "logical content text has no indexed predecessor",
        ));
    };
    let mut logical = start;
    let mut after_start = first.key().logical_start;
    let mut output = Vec::with_capacity(max_payload_bytes);
    let mut cached_chunk = None;
    let reached_page_boundary = append_span(
        storage,
        store,
        content,
        first.key(),
        *first.value(),
        true,
        &mut cached_chunk,
        &mut logical,
        desired_end,
        &mut output,
        &mut stored_bytes,
    )?;
    if reached_page_boundary {
        return finish_page(output, logical, start, max_payload_bytes, stored_bytes);
    }

    while logical < desired_end {
        let page = store.read_cursor::<crate::domain::SyndicDomain, ContentTextSpansCodec>(
            storage.handle,
            &CursorRange::after(
                ContentTextSpanKey {
                    owner: content.id(),
                    logical_start: after_start,
                },
                ContentTextSpanKey {
                    owner: content.id(),
                    logical_start: desired_end,
                },
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(CONTENT_TEXT_INDEX_PAGE_ITEMS, CONTENT_TEXT_INDEX_PAGE_BYTES)
                .expect("text index-page bounds are nonzero"),
        )?;
        stored_bytes =
            stored_bytes
                .checked_add(page.stored_bytes())
                .ok_or(SyndicReadError::Invariant(
                    "logical content text stored-byte accounting overflowed",
                ))?;
        if page.records().is_empty() {
            return Err(SyndicReadError::Invariant(
                "logical content text has an indexed gap",
            ));
        }
        for record in page.records() {
            after_start = record.key().logical_start;
            if append_span(
                storage,
                store,
                content,
                record.key(),
                *record.value(),
                false,
                &mut cached_chunk,
                &mut logical,
                desired_end,
                &mut output,
                &mut stored_bytes,
            )? {
                return finish_page(output, logical, start, max_payload_bytes, stored_bytes);
            }
            if logical == desired_end {
                break;
            }
        }
    }
    finish_page(output, logical, start, max_payload_bytes, stored_bytes)
}

fn finish_page(
    output: Vec<u8>,
    logical: u64,
    start: u64,
    max_payload_bytes: usize,
    stored_bytes: usize,
) -> Result<(Vec<u8>, u64, usize), SyndicReadError> {
    if output.is_empty() {
        return Err(SyndicReadError::ContentTextReadLimitTooSmall {
            offset: start,
            actual: max_payload_bytes,
        });
    }
    Ok((output, logical, stored_bytes))
}

struct CachedChunk {
    ordinal: ContentChunkOrdinal,
    start: u64,
    record: ContentChunkRecord,
}

#[allow(clippy::too_many_arguments)]
fn append_span(
    storage: &SyndicStorage,
    store: &HomeStore,
    content: ContentReference,
    key: &ContentTextSpanKey,
    span: ContentTextSpanRecord,
    predecessor: bool,
    cached_chunk: &mut Option<CachedChunk>,
    logical: &mut u64,
    desired_end: u64,
    output: &mut Vec<u8>,
    stored_bytes: &mut usize,
) -> Result<bool, SyndicReadError> {
    validate_span(content, key, span, predecessor, *logical)?;
    if cached_chunk.as_ref().is_none_or(|cached| {
        cached.ordinal != span.chunk_ordinal() || cached.start != span.chunk_start()
    }) {
        let chunk = storage
            .point::<ContentChunksFamily>(
                store,
                ContentChunkKey {
                    owner: content.id(),
                    ordinal: span.chunk_ordinal(),
                },
                family_limit::<ContentChunksFamily>(),
            )?
            .ok_or(SyndicReadError::Invariant(
                "sealed content text chunk is missing",
            ))?;
        *stored_bytes =
            stored_bytes
                .checked_add(chunk.stored_bytes())
                .ok_or(SyndicReadError::Invariant(
                    "logical content text stored-byte accounting overflowed",
                ))?;
        *cached_chunk = Some(CachedChunk {
            ordinal: span.chunk_ordinal(),
            start: span.chunk_start(),
            record: chunk.record().clone(),
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
    if span.break_before() {
        return Err(SyndicReadError::Invariant(
            "marker-free sealed content contains a marker-separated text span",
        ));
    }
    Ok(())
}

fn family_limit<F: Family>() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(family_point_limit::<F>().max_bytes())
        .expect("codec family point-read bound is nonzero")
}

pub(super) fn invalid_offset(content_bytes: u64, offset: u64) -> SyndicReadError {
    SyndicReadError::InvalidContentTextOffset {
        content_bytes,
        offset,
    }
}

fn concurrent() -> SyndicReadError {
    SyndicReadError::ConcurrentChange {
        operation: CONTENT_TEXT_OPERATION,
    }
}

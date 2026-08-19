use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore};
use sha2::{Digest, Sha256};

use crate::{
    ContentChunkOrdinal, ContentChunkRecord, ContentEncoding, ContentLifecycle,
    ContentManifestRecord, ContentReference, ContentTextSpanRecord, SyndicReadError,
    codec::{
        ContentChunkKey, ContentChunksFamily, ContentManifestsFamily, ContentTextSpanKey,
        ContentTextSpansCodec, Family, family_cursor_max_bytes, family_point_limit,
    },
    content::input_marker_digest,
    domain::SyndicStorage,
};

use super::{ReadByteTotals, SyndicPointReadLimit};

mod append;
mod segment;

use append::{append_span, concurrent, finish_page};
pub(super) use append::{family_limit, invalid_offset};

pub use segment::{
    SyndicContentTextSegment, SyndicContentTextSegmentBoundary, SyndicContentTextSegmentRangeRead,
};

pub(super) const CONTENT_TEXT_MAX_PAYLOAD_BYTES: usize = 65_536;
const CONTENT_TEXT_INDEX_PAGE_ITEMS: usize = 256;
const CONTENT_TEXT_INDEX_PAGE_BYTES: usize = 65_536;
const CONTENT_TEXT_OPERATION: &str = "sealed-content text-range read";

#[cfg(feature = "test-faults")]
use crate::test_faults::{ContentTextReadResidencyLease, ContentTextReadResidencyTracker};

pub(super) struct TextPageAssembly {
    bytes: Vec<u8>,
    end: u64,
    stored_bytes: usize,
    decoded_bytes: usize,
    #[cfg(feature = "test-faults")]
    output_residency: Option<ContentTextReadResidencyLease>,
}

/// One bounded UTF-8 page from an exact immutable logical content value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicContentTextRangeRead {
    content: ContentReference,
    start: u64,
    text: Box<str>,
    next_offset: Option<u64>,
    stored_bytes: usize,
    decoded_bytes: usize,
}

impl SyndicContentTextRangeRead {
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

    /// Consumes the page and transfers its bounded text without copying bytes.
    #[must_use]
    pub fn into_text(self) -> Box<str> {
        self.text
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

    /// Returns the checked practical decoded bytes retained by typed cursor pages.
    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
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
        self.sealed_content_text_range_inner(
            store,
            content,
            start,
            max_payload_bytes,
            #[cfg(feature = "test-faults")]
            None,
        )
    }

    /// Reads through the production path while recording content-free dependency residency.
    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn sealed_content_text_range_tracked_for_lifecycle_test(
        &self,
        store: &HomeStore,
        content: ContentReference,
        start: u64,
        max_payload_bytes: usize,
        tracker: &ContentTextReadResidencyTracker,
    ) -> Result<Option<SyndicContentTextRangeRead>, SyndicReadError> {
        self.sealed_content_text_range_inner(
            store,
            content,
            start,
            max_payload_bytes,
            Some(tracker),
        )
    }

    fn sealed_content_text_range_inner(
        &self,
        store: &HomeStore,
        content: ContentReference,
        start: u64,
        max_payload_bytes: usize,
        #[cfg(feature = "test-faults")] tracker: Option<&ContentTextReadResidencyTracker>,
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
        validate_manifest(&first, content, MarkerPolicy::MarkerFree)?;
        let content_bytes = content.summary().logical_utf8_bytes();
        if start > content_bytes {
            return Err(invalid_offset(content_bytes, start));
        }

        let assembly = if start == content_bytes {
            TextPageAssembly {
                bytes: Vec::new(),
                end: start,
                stored_bytes: 0,
                decoded_bytes: 0,
                #[cfg(feature = "test-faults")]
                output_residency: None,
            }
        } else {
            let payload_bytes_u64 = u64::try_from(max_payload_bytes)
                .expect("the fixed content text payload bound fits u64");
            let desired_end = start.saturating_add(payload_bytes_u64).min(content_bytes);
            read_text_page(
                self,
                store,
                content,
                start,
                desired_end,
                max_payload_bytes,
                None,
                #[cfg(feature = "test-faults")]
                tracker,
            )?
        };

        let second = self
            .content_manifest(store, content.id(), manifest_limit)?
            .ok_or_else(concurrent)?;
        if second != first {
            return Err(concurrent());
        }
        validate_manifest(&second, content, MarkerPolicy::MarkerFree)?;
        let text = String::from_utf8(assembly.bytes)
            .map_err(|_| {
                SyndicReadError::Invariant("logical content text page is not valid UTF-8")
            })?
            .into_boxed_str();
        let page = SyndicContentTextRangeRead {
            content,
            start,
            text,
            next_offset: (assembly.end < content_bytes).then_some(assembly.end),
            stored_bytes: assembly.stored_bytes,
            decoded_bytes: assembly.decoded_bytes,
        };
        #[cfg(feature = "test-faults")]
        drop(assembly.output_residency);
        Ok(Some(page))
    }
}

#[derive(Clone, Copy)]
pub(super) enum MarkerPolicy {
    MarkerFree,
    MarkerAware,
}

#[derive(Clone, Copy)]
pub(super) struct TextSegmentBounds {
    pub(super) start: u64,
    pub(super) break_at_start: bool,
}

pub(super) fn validate_manifest(
    manifest: &ContentManifestRecord,
    content: ContentReference,
    marker_policy: MarkerPolicy,
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
        || (content.encoding() != ContentEncoding::ComposerV1
            && content.id()
                != beryl_model::SyndicContentId::from_digest(*summary.digest().as_bytes()))
    {
        return Err(SyndicReadError::Invariant(
            "sealed content reference disagrees with its exact manifest",
        ));
    }
    if matches!(marker_policy, MarkerPolicy::MarkerFree) && summary.image_marker_count() != 0 {
        return Err(SyndicReadError::ContentTextContainsImageMarkers {
            actual: summary.image_marker_count(),
        });
    }
    if summary.image_marker_count() == 0
        && summary.marker_digest() != input_marker_digest(std::iter::empty())
    {
        return Err(SyndicReadError::Invariant(
            "marker-free content has a nonempty marker digest",
        ));
    }
    if matches!(marker_policy, MarkerPolicy::MarkerAware)
        && summary.image_marker_count() != 0
        && content.encoding() != crate::ContentEncoding::ComposerV1
    {
        return Err(SyndicReadError::Invariant(
            "marker-bearing sealed content is not ComposerV1",
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
    segment: Option<TextSegmentBounds>,
    #[cfg(feature = "test-faults")] tracker: Option<&ContentTextReadResidencyTracker>,
) -> Result<TextPageAssembly, SyndicReadError> {
    let (first_key, first_span, mut totals) = {
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
        #[cfg(feature = "test-faults")]
        let _cursor_page_residency =
            tracker.map(|tracker| tracker.acquire_cursor_page(predecessor.stored_bytes()));
        let Some(first) = predecessor.records().first() else {
            return Err(SyndicReadError::Invariant(
                "logical content text has no indexed predecessor",
            ));
        };
        (
            *first.key(),
            *first.value(),
            ReadByteTotals::new(predecessor.stored_bytes(), predecessor.decoded_bytes()),
        )
    };
    let mut logical = start;
    let mut after_start = first_key.logical_start;
    #[cfg(feature = "test-faults")]
    let output_residency = tracker.map(|tracker| tracker.acquire_output(max_payload_bytes));
    let mut output = Vec::with_capacity(max_payload_bytes);
    let mut cached_chunk = None;
    let reached_page_boundary = append_span(
        storage,
        store,
        content,
        &first_key,
        first_span,
        true,
        segment,
        &mut cached_chunk,
        &mut logical,
        desired_end,
        &mut output,
        &mut totals,
        #[cfg(feature = "test-faults")]
        tracker,
    )?;
    if reached_page_boundary {
        return finish_page(
            output,
            logical,
            start,
            max_payload_bytes,
            totals,
            #[cfg(feature = "test-faults")]
            output_residency,
        );
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
        #[cfg(feature = "test-faults")]
        let _cursor_page_residency =
            tracker.map(|tracker| tracker.acquire_cursor_page(page.stored_bytes()));
        totals.add(
            page.stored_bytes(),
            page.decoded_bytes(),
            "logical content text byte accounting overflowed",
        )?;
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
                segment,
                &mut cached_chunk,
                &mut logical,
                desired_end,
                &mut output,
                &mut totals,
                #[cfg(feature = "test-faults")]
                tracker,
            )? {
                return finish_page(
                    output,
                    logical,
                    start,
                    max_payload_bytes,
                    totals,
                    #[cfg(feature = "test-faults")]
                    output_residency,
                );
            }
            if logical == desired_end {
                break;
            }
        }
    }
    finish_page(
        output,
        logical,
        start,
        max_payload_bytes,
        totals,
        #[cfg(feature = "test-faults")]
        output_residency,
    )
}

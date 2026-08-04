use beryl_home_store::HomeStore;
use beryl_model::SyndicDraftMarkerId;

use crate::{
    ContentPieceOrdinal, ContentReference, ImageLabelOrdinal, InputMarkerOrdinal, SyndicReadError,
    codec::ContentManifestsFamily, domain::SyndicStorage,
};

#[cfg(feature = "test-faults")]
use super::ContentTextReadResidencyTracker;
use super::{
    CONTENT_TEXT_MAX_PAYLOAD_BYTES, MarkerPolicy, TextPageAssembly, TextSegmentBounds,
    family_limit, read_text_page, validate_manifest,
};

mod marker;
mod validation;

use validation::validate_segment;

const CONTENT_TEXT_SEGMENT_PROOF_OPERATION: &str = "sealed-content text-segment proof";
const CONTENT_TEXT_SEGMENT_RANGE_OPERATION: &str = "sealed-content text-segment-range read";

/// One authenticated image-marker boundary around a sealed authored-text segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyndicContentTextSegmentBoundary {
    piece_ordinal: ContentPieceOrdinal,
    marker_ordinal: InputMarkerOrdinal,
    logical_offset: u64,
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
}

impl SyndicContentTextSegmentBoundary {
    /// Returns the marker's exact ordered content-piece position.
    #[must_use]
    pub const fn piece_ordinal(self) -> ContentPieceOrdinal {
        self.piece_ordinal
    }

    /// Returns the marker's exact contiguous marker position.
    #[must_use]
    pub const fn marker_ordinal(self) -> InputMarkerOrdinal {
        self.marker_ordinal
    }

    /// Returns the marker's absolute zero-width logical UTF-8 offset.
    #[must_use]
    pub const fn logical_offset(self) -> u64 {
        self.logical_offset
    }

    /// Returns the exact authenticated draft marker identity.
    #[must_use]
    pub const fn marker_id(self) -> SyndicDraftMarkerId {
        self.marker_id
    }

    /// Returns the exact authenticated generated image label.
    #[must_use]
    pub const fn label(self) -> ImageLabelOrdinal {
        self.label
    }
}

/// Opaque proof of one complete marker-bounded segment in exact sealed content.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicContentTextSegment {
    content: ContentReference,
    start: u64,
    end: u64,
    preceding_marker: Option<SyndicContentTextSegmentBoundary>,
    following_marker: Option<SyndicContentTextSegmentBoundary>,
}

impl SyndicContentTextSegment {
    /// Returns the exact sealed content reference authenticated by this proof.
    #[must_use]
    pub const fn content(&self) -> ContentReference {
        self.content
    }

    /// Returns the derived absolute logical UTF-8 segment start.
    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }

    /// Returns the derived absolute logical UTF-8 segment end.
    #[must_use]
    pub const fn end(&self) -> u64 {
        self.end
    }

    /// Returns the authenticated marker immediately before this segment, if any.
    #[must_use]
    pub const fn preceding_marker(&self) -> Option<SyndicContentTextSegmentBoundary> {
        self.preceding_marker
    }

    /// Returns the authenticated marker immediately after this segment, if any.
    #[must_use]
    pub const fn following_marker(&self) -> Option<SyndicContentTextSegmentBoundary> {
        self.following_marker
    }
}

/// One bounded UTF-8 page from one exact image-marker-bounded logical text segment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicContentTextSegmentRangeRead {
    content: ContentReference,
    segment_start: u64,
    segment_end: u64,
    start: u64,
    text: Box<str>,
    next_offset: Option<u64>,
    stored_bytes: usize,
    decoded_bytes: usize,
}

impl SyndicContentTextSegmentRangeRead {
    /// Returns the exact sealed content reference read by this page.
    #[must_use]
    pub const fn content(&self) -> ContentReference {
        self.content
    }

    /// Returns the absolute logical UTF-8 start of the proven marker-bounded segment.
    #[must_use]
    pub const fn segment_start(&self) -> u64 {
        self.segment_start
    }

    /// Returns the absolute logical UTF-8 end of the proven marker-bounded segment.
    #[must_use]
    pub const fn segment_end(&self) -> u64 {
        self.segment_end
    }

    /// Returns this page's absolute logical UTF-8 start offset.
    #[must_use]
    pub const fn start(&self) -> u64 {
        self.start
    }

    /// Returns the bounded exact authored text payload.
    #[must_use]
    pub const fn text(&self) -> &str {
        &self.text
    }

    /// Consumes the page and transfers its bounded text without copying bytes.
    #[must_use]
    pub fn into_text(self) -> Box<str> {
        self.text
    }

    /// Returns the absolute offset for the next page, or `None` at `segment_end`.
    #[must_use]
    pub const fn next_offset(&self) -> Option<u64> {
        self.next_offset
    }

    /// Returns the checked stored key-and-value bytes read by typed cursor pages.
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
    /// Proves the unique sealed authored-text segment immediately after `after_marker`.
    ///
    /// `None` selects the unique leading segment. `Some(boundary)` must be the exact opaque
    /// following-marker boundary returned by the preceding segment proof and selects the segment
    /// immediately after it. The read
    /// scans that complete ordered-piece interval with fixed memory and returns no proof until the
    /// next authenticated marker or exact content EOF is reached. This preserves distinct leading,
    /// trailing, adjacent-marker, marker-only, and marker-free segments.
    pub fn prove_sealed_content_text_segment(
        &self,
        store: &HomeStore,
        content: ContentReference,
        after_marker: Option<SyndicContentTextSegmentBoundary>,
    ) -> Result<Option<SyndicContentTextSegment>, SyndicReadError> {
        let manifest_limit = family_limit::<ContentManifestsFamily>();
        let Some(first) = self.content_manifest(store, content.id(), manifest_limit)? else {
            return match self.content_manifest(store, content.id(), manifest_limit)? {
                None => Ok(None),
                Some(_) => Err(concurrent(CONTENT_TEXT_SEGMENT_PROOF_OPERATION)),
            };
        };
        validate_manifest(&first, content, MarkerPolicy::MarkerAware)?;
        let validated = validate_segment(self, store, content, after_marker)?;
        let second = self
            .content_manifest(store, content.id(), manifest_limit)?
            .ok_or_else(|| concurrent(CONTENT_TEXT_SEGMENT_PROOF_OPERATION))?;
        if second != first {
            return Err(concurrent(CONTENT_TEXT_SEGMENT_PROOF_OPERATION));
        }
        validate_manifest(&second, content, MarkerPolicy::MarkerAware)?;
        Ok(Some(SyndicContentTextSegment {
            content,
            start: validated.start,
            end: validated.end,
            preceding_marker: validated.preceding_marker,
            following_marker: validated.following_marker,
        }))
    }
}

impl SyndicStorage {
    /// Reads one bounded logical UTF-8 page authorized by an opaque exact segment proof.
    ///
    /// `start` is absolute, must lie within the proven segment, and must be a UTF-8 boundary.
    /// `max_payload_bytes` must be in `1..=65_536`. The complete segment is not rescanned; the
    /// physical page path remains independently validated and cannot cross a contradictory marker.
    pub fn sealed_content_text_segment_range(
        &self,
        store: &HomeStore,
        segment: &SyndicContentTextSegment,
        start: u64,
        max_payload_bytes: usize,
    ) -> Result<Option<SyndicContentTextSegmentRangeRead>, SyndicReadError> {
        self.sealed_content_text_segment_range_inner(
            store,
            segment,
            start,
            max_payload_bytes,
            #[cfg(feature = "test-faults")]
            None,
        )
    }

    /// Reads through the production path while recording content-free dependency residency.
    #[cfg(feature = "test-faults")]
    #[doc(hidden)]
    pub fn sealed_content_text_segment_range_tracked_for_lifecycle_test(
        &self,
        store: &HomeStore,
        segment: &SyndicContentTextSegment,
        start: u64,
        max_payload_bytes: usize,
        tracker: &ContentTextReadResidencyTracker,
    ) -> Result<Option<SyndicContentTextSegmentRangeRead>, SyndicReadError> {
        self.sealed_content_text_segment_range_inner(
            store,
            segment,
            start,
            max_payload_bytes,
            Some(tracker),
        )
    }

    fn sealed_content_text_segment_range_inner(
        &self,
        store: &HomeStore,
        segment: &SyndicContentTextSegment,
        start: u64,
        max_payload_bytes: usize,
        #[cfg(feature = "test-faults")] tracker: Option<&ContentTextReadResidencyTracker>,
    ) -> Result<Option<SyndicContentTextSegmentRangeRead>, SyndicReadError> {
        if max_payload_bytes == 0 || max_payload_bytes > CONTENT_TEXT_MAX_PAYLOAD_BYTES {
            return Err(SyndicReadError::InvalidContentTextReadLimit {
                maximum: CONTENT_TEXT_MAX_PAYLOAD_BYTES,
                actual: max_payload_bytes,
            });
        }
        if start < segment.start || start > segment.end {
            return Err(SyndicReadError::InvalidContentTextSegmentOffset {
                segment_start: segment.start,
                segment_end: segment.end,
                offset: start,
            });
        }

        let content = segment.content;
        let manifest_limit = family_limit::<ContentManifestsFamily>();
        let Some(first) = self.content_manifest(store, content.id(), manifest_limit)? else {
            return match self.content_manifest(store, content.id(), manifest_limit)? {
                None => Ok(None),
                Some(_) => Err(concurrent(CONTENT_TEXT_SEGMENT_RANGE_OPERATION)),
            };
        };
        validate_manifest(&first, content, MarkerPolicy::MarkerAware)?;
        let assembly = if start == segment.end {
            TextPageAssembly {
                bytes: Vec::new(),
                end: start,
                stored_bytes: 0,
                decoded_bytes: 0,
                #[cfg(feature = "test-faults")]
                output_residency: None,
            }
        } else {
            let maximum = u64::try_from(max_payload_bytes)
                .expect("the fixed content text payload bound fits u64");
            let desired_end = start.saturating_add(maximum).min(segment.end);
            read_text_page(
                self,
                store,
                content,
                start,
                desired_end,
                max_payload_bytes,
                Some(TextSegmentBounds {
                    start: segment.start,
                    break_at_start: segment.preceding_marker.is_some(),
                }),
                #[cfg(feature = "test-faults")]
                tracker,
            )?
        };

        let second = self
            .content_manifest(store, content.id(), manifest_limit)?
            .ok_or_else(|| concurrent(CONTENT_TEXT_SEGMENT_RANGE_OPERATION))?;
        if second != first {
            return Err(concurrent(CONTENT_TEXT_SEGMENT_RANGE_OPERATION));
        }
        validate_manifest(&second, content, MarkerPolicy::MarkerAware)?;
        let stored_bytes = assembly.stored_bytes;
        let text = String::from_utf8(assembly.bytes)
            .map_err(|_| {
                SyndicReadError::Invariant("logical content text segment page is not valid UTF-8")
            })?
            .into_boxed_str();
        let page = SyndicContentTextSegmentRangeRead {
            content,
            segment_start: segment.start,
            segment_end: segment.end,
            start,
            text,
            next_offset: (assembly.end < segment.end).then_some(assembly.end),
            stored_bytes,
            decoded_bytes: assembly.decoded_bytes,
        };
        #[cfg(feature = "test-faults")]
        drop(assembly.output_residency);
        Ok(Some(page))
    }
}

fn concurrent(operation: &'static str) -> SyndicReadError {
    SyndicReadError::ConcurrentChange { operation }
}

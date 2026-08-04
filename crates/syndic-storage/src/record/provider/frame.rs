use crate::{
    ContentEncoding, ContentReference, ProviderFrameHistorySupportV1,
    ProviderFrameObservationSummaryV1, ProviderFrameReferenceV1, ProviderItemStreamStateV1,
};

use super::{
    ProviderNarrativeReference, ProviderStorageRecordError, narrative::validate_sealed_narrative,
};

/// Exact published provider-frame snapshot over one sealed content frontier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedProviderFrameReference {
    content: ContentReference,
    frame: ProviderFrameReferenceV1,
    observation: ProviderFrameObservationSummaryV1,
    stream_state: ProviderItemStreamStateV1,
    narrative: Option<ProviderNarrativeReference>,
}

impl SealedProviderFrameReference {
    pub fn new(
        content: ContentReference,
        frame: ProviderFrameReferenceV1,
        observation: ProviderFrameObservationSummaryV1,
        stream_state: ProviderItemStreamStateV1,
        narrative: Option<ProviderNarrativeReference>,
    ) -> Result<Self, ProviderStorageRecordError> {
        let content_end = content.summary().encoded_bytes();
        if content.encoding() != ContentEncoding::ProviderItemV1 {
            return Err(ProviderStorageRecordError::InvalidContentEncoding);
        }
        let summary = content.summary();
        if summary.piece_count() != 0
            || summary.logical_utf8_bytes() != 0
            || summary.atom_count() != 0
            || summary.image_marker_count() != 0
            || summary.marker_digest() != crate::content::input_marker_digest(std::iter::empty())
        {
            return Err(ProviderStorageRecordError::InvalidProviderContentSummary);
        }
        if frame.encoded_end() != content_end {
            return Err(ProviderStorageRecordError::FrameContentFrontierMismatch {
                frame_end: frame.encoded_end(),
                content_end,
            });
        }
        if frame.encoded_start() >= frame.encoded_end() || frame.encoded_end() > content_end {
            return Err(ProviderStorageRecordError::FrameOutsideContent {
                start: frame.encoded_start(),
                end: frame.encoded_end(),
                content_end,
            });
        }
        if stream_state.item_id() != frame.item_id()
            || stream_state.kind() != frame.item_kind()
            || frame.ordinal().checked_next().ok() != Some(stream_state.next_ordinal())
        {
            return Err(ProviderStorageRecordError::StreamStateFrameMismatch);
        }
        if !stream_state_agrees_with_observation(&stream_state, observation) {
            return Err(ProviderStorageRecordError::StreamStateObservationMismatch);
        }
        validate_sealed_narrative(content.id(), &frame, stream_state.is_complete(), narrative)?;
        Ok(Self {
            content,
            frame,
            observation,
            stream_state,
            narrative,
        })
    }

    #[must_use]
    pub const fn content(&self) -> ContentReference {
        self.content
    }
    #[must_use]
    pub const fn frame(&self) -> &ProviderFrameReferenceV1 {
        &self.frame
    }
    #[must_use]
    pub const fn observation(&self) -> ProviderFrameObservationSummaryV1 {
        self.observation
    }
    #[must_use]
    pub const fn stream_state(&self) -> &ProviderItemStreamStateV1 {
        &self.stream_state
    }
    #[must_use]
    pub const fn narrative(&self) -> Option<ProviderNarrativeReference> {
        self.narrative
    }
    #[must_use]
    pub const fn history_support(&self) -> ProviderFrameHistorySupportV1 {
        self.stream_state.history_support()
    }
}

fn stream_state_agrees_with_observation(
    state: &ProviderItemStreamStateV1,
    observation: ProviderFrameObservationSummaryV1,
) -> bool {
    match observation {
        ProviderFrameObservationSummaryV1::Started(observed_at) => {
            !state.is_complete() && state.started_at() == Some(observed_at)
        }
        ProviderFrameObservationSummaryV1::Delta => {
            !state.is_complete() && state.started_at().is_some()
        }
        ProviderFrameObservationSummaryV1::Completed(observed_at) => {
            state.is_complete()
                && match state.started_at() {
                    Some(started_at) => observed_at >= started_at,
                    None => state.kind().permits_completion_only(),
                }
        }
    }
}

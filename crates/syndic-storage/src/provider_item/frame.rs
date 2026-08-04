use std::num::NonZeroU64;

use beryl_model::CasItemId;

use crate::{ProviderItemKind, UnsupportedHistoryReason};

use super::{
    ProviderFileUpdateChangeV1, ProviderItemV1, ProviderItemValidationError, ProviderTextV1,
};

/// Stable one-based order of a frame inside one item-owned ProviderItemV1 stream.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderFrameOrdinalV1(NonZeroU64);

impl ProviderFrameOrdinalV1 {
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    pub fn new(value: u64) -> Result<Self, ProviderItemValidationError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(ProviderItemValidationError::ZeroFrameOrdinal)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub fn checked_next(self) -> Result<Self, ProviderItemValidationError> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(ProviderItemValidationError::FrameOrdinalExhausted)
    }
}

/// Exact nonnegative provider-supplied lifecycle observation in milliseconds.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderLifecycleTimestampMsV1(u64);

impl ProviderLifecycleTimestampMsV1 {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Whether one retained provider observation can support complete captured history.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ProviderFrameHistorySupportV1 {
    #[default]
    Supported,
    Unsupported(UnsupportedHistoryReason),
}

impl ProviderFrameHistorySupportV1 {
    /// Accumulates observations monotonically, retaining the first unsupported reason.
    #[must_use]
    pub const fn merge(self, next: Self) -> Self {
        match self {
            Self::Unsupported(_) => self,
            Self::Supported => next,
        }
    }

    #[must_use]
    pub const fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }

    #[must_use]
    pub const fn unsupported_reason(self) -> Option<UnsupportedHistoryReason> {
        match self {
            Self::Supported => None,
            Self::Unsupported(reason) => Some(reason),
        }
    }
}

/// Every admitted pinned delta, with its expected item kind carried by the type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderItemDeltaV1 {
    AgentMessage {
        delta: ProviderTextV1,
    },
    Plan {
        delta: ProviderTextV1,
    },
    ReasoningSummaryPartAdded {
        summary_index: u64,
    },
    ReasoningSummaryText {
        summary_index: u64,
        delta: ProviderTextV1,
    },
    ReasoningTextObserved {
        content_index: u64,
    },
    CommandExecutionOutput {
        delta: ProviderTextV1,
    },
    FileChangeOutput {
        delta: ProviderTextV1,
    },
    FileChangePatchUpdated {
        changes: Vec<ProviderFileUpdateChangeV1>,
    },
    McpToolCallProgress {
        message: ProviderTextV1,
    },
}

impl ProviderItemDeltaV1 {
    #[must_use]
    pub const fn expected_kind(&self) -> ProviderItemKind {
        match self {
            Self::AgentMessage { .. } => ProviderItemKind::AgentMessage,
            Self::Plan { .. } => ProviderItemKind::Plan,
            Self::ReasoningSummaryPartAdded { .. }
            | Self::ReasoningSummaryText { .. }
            | Self::ReasoningTextObserved { .. } => ProviderItemKind::Reasoning,
            Self::CommandExecutionOutput { .. } => ProviderItemKind::CommandExecution,
            Self::FileChangeOutput { .. } | Self::FileChangePatchUpdated { .. } => {
                ProviderItemKind::FileChange
            }
            Self::McpToolCallProgress { .. } => ProviderItemKind::McpToolCall,
        }
    }
}

/// Exact start, delta, or authoritative completion observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderItemObservationV1 {
    Started {
        observed_at: ProviderLifecycleTimestampMsV1,
        item: ProviderItemV1,
    },
    Delta(ProviderItemDeltaV1),
    Completed {
        observed_at: ProviderLifecycleTimestampMsV1,
        item: ProviderItemV1,
    },
}

impl ProviderItemObservationV1 {
    #[must_use]
    pub const fn kind(&self) -> ProviderItemKind {
        match self {
            Self::Started { item, .. } | Self::Completed { item, .. } => item.kind(),
            Self::Delta(delta) => delta.expected_kind(),
        }
    }

    #[must_use]
    pub const fn history_support(&self) -> ProviderFrameHistorySupportV1 {
        match self {
            Self::Started { item, .. } | Self::Completed { item, .. } => item.history_support(),
            Self::Delta(_) => ProviderFrameHistorySupportV1::Supported,
        }
    }
}

/// One immutable frame in an item-owned provider content stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderItemFrameV1 {
    ordinal: ProviderFrameOrdinalV1,
    item_id: CasItemId,
    observation: ProviderItemObservationV1,
}

impl ProviderItemFrameV1 {
    #[must_use]
    pub const fn new(
        ordinal: ProviderFrameOrdinalV1,
        item_id: CasItemId,
        observation: ProviderItemObservationV1,
    ) -> Self {
        Self {
            ordinal,
            item_id,
            observation,
        }
    }

    #[must_use]
    pub const fn ordinal(&self) -> ProviderFrameOrdinalV1 {
        self.ordinal
    }

    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }

    #[must_use]
    pub const fn observation(&self) -> &ProviderItemObservationV1 {
        &self.observation
    }

    #[must_use]
    pub const fn kind(&self) -> ProviderItemKind {
        self.observation.kind()
    }

    #[must_use]
    pub const fn history_support(&self) -> ProviderFrameHistorySupportV1 {
        self.observation.history_support()
    }

    pub fn validate(&self, prior_frontier: u64) -> Result<(), ProviderItemValidationError> {
        super::validate::validate_frame(self, prior_frontier)
    }
}

/// Semantic category of one frame-selected logical UTF-8 span.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProviderLogicalTextRoleV1 {
    Narrative,
    Operational,
    Activity,
}

/// Frame-specific logical view over bytes stored once in the provider stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderFrameTextSpanV1 {
    frame_ordinal: ProviderFrameOrdinalV1,
    logical_start: u64,
    logical_end: u64,
    source_start: u64,
    source_end: u64,
    source_digest: [u8; 32],
    role: ProviderLogicalTextRoleV1,
}

impl ProviderFrameTextSpanV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        frame_ordinal: ProviderFrameOrdinalV1,
        logical_start: u64,
        logical_end: u64,
        source_start: u64,
        source_end: u64,
        source_digest: [u8; 32],
        role: ProviderLogicalTextRoleV1,
    ) -> Result<Self, ProviderItemValidationError> {
        let logical_len = logical_end.checked_sub(logical_start);
        let source_len = source_end.checked_sub(source_start);
        if logical_len.is_none() || logical_len == Some(0) || logical_len != source_len {
            return Err(ProviderItemValidationError::InvalidFrameTextSpan);
        }
        Ok(Self {
            frame_ordinal,
            logical_start,
            logical_end,
            source_start,
            source_end,
            source_digest,
            role,
        })
    }

    #[must_use]
    pub const fn frame_ordinal(self) -> ProviderFrameOrdinalV1 {
        self.frame_ordinal
    }
    #[must_use]
    pub const fn logical_start(self) -> u64 {
        self.logical_start
    }
    #[must_use]
    pub const fn logical_end(self) -> u64 {
        self.logical_end
    }
    #[must_use]
    pub const fn source_start(self) -> u64 {
        self.source_start
    }
    #[must_use]
    pub const fn source_end(self) -> u64 {
        self.source_end
    }
    #[must_use]
    pub const fn source_digest(self) -> [u8; 32] {
        self.source_digest
    }
    #[must_use]
    pub const fn role(self) -> ProviderLogicalTextRoleV1 {
        self.role
    }
}

/// Exact sealed reference produced by streaming one frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFrameReferenceV1 {
    item_id: CasItemId,
    item_kind: ProviderItemKind,
    ordinal: ProviderFrameOrdinalV1,
    encoded_start: u64,
    encoded_end: u64,
    encoded_digest: [u8; 32],
    logical_utf8_bytes: u64,
    text_span_count: u64,
}

impl ProviderFrameReferenceV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        item_id: CasItemId,
        item_kind: ProviderItemKind,
        ordinal: ProviderFrameOrdinalV1,
        encoded_start: u64,
        encoded_end: u64,
        encoded_digest: [u8; 32],
        logical_utf8_bytes: u64,
        text_span_count: u64,
    ) -> Result<Self, ProviderItemValidationError> {
        if encoded_start >= encoded_end {
            return Err(ProviderItemValidationError::InvalidFrameRange {
                start: encoded_start,
                end: encoded_end,
            });
        }
        if (logical_utf8_bytes == 0) != (text_span_count == 0) {
            return Err(ProviderItemValidationError::FrameTextSpanSummaryMismatch);
        }
        Ok(Self {
            item_id,
            item_kind,
            ordinal,
            encoded_start,
            encoded_end,
            encoded_digest,
            logical_utf8_bytes,
            text_span_count,
        })
    }

    #[must_use]
    pub const fn encoded_len(&self) -> u64 {
        self.encoded_end - self.encoded_start
    }

    #[must_use]
    pub const fn item_id(&self) -> &CasItemId {
        &self.item_id
    }
    #[must_use]
    pub const fn item_kind(&self) -> ProviderItemKind {
        self.item_kind
    }
    #[must_use]
    pub const fn ordinal(&self) -> ProviderFrameOrdinalV1 {
        self.ordinal
    }
    #[must_use]
    pub const fn encoded_start(&self) -> u64 {
        self.encoded_start
    }
    #[must_use]
    pub const fn encoded_end(&self) -> u64 {
        self.encoded_end
    }
    #[must_use]
    pub const fn encoded_digest(&self) -> [u8; 32] {
        self.encoded_digest
    }
    #[must_use]
    pub const fn logical_utf8_bytes(&self) -> u64 {
        self.logical_utf8_bytes
    }
    #[must_use]
    pub const fn text_span_count(&self) -> u64 {
        self.text_span_count
    }
}

/// Constant-resident proof that emitted spans agree with their sealed frame reference.
#[derive(Clone, Copy, Debug)]
pub struct ProviderFrameTextSpanValidatorV1 {
    ordinal: ProviderFrameOrdinalV1,
    logical_frontier: u64,
    count: u64,
    maximum_source_end: u64,
}

impl ProviderFrameTextSpanValidatorV1 {
    #[must_use]
    pub const fn new(ordinal: ProviderFrameOrdinalV1) -> Self {
        Self {
            ordinal,
            logical_frontier: 0,
            count: 0,
            maximum_source_end: 0,
        }
    }

    pub fn observe(
        &mut self,
        span: ProviderFrameTextSpanV1,
    ) -> Result<(), ProviderItemValidationError> {
        if span.frame_ordinal() != self.ordinal {
            return Err(ProviderItemValidationError::FrameTextSpanOrdinalMismatch);
        }
        if span.logical_start() != self.logical_frontier {
            return Err(ProviderItemValidationError::FrameTextSpanFrontierConflict {
                expected: self.logical_frontier,
            });
        }
        self.logical_frontier = span.logical_end();
        self.maximum_source_end = self.maximum_source_end.max(span.source_end());
        self.count = self
            .count
            .checked_add(1)
            .ok_or(ProviderItemValidationError::FrameLengthOverflow)?;
        Ok(())
    }

    pub fn finish(
        self,
        frame: &ProviderFrameReferenceV1,
    ) -> Result<(), ProviderItemValidationError> {
        if frame.ordinal() != self.ordinal
            || frame.logical_utf8_bytes() != self.logical_frontier
            || frame.text_span_count() != self.count
            || self.maximum_source_end > frame.encoded_end()
        {
            return Err(ProviderItemValidationError::FrameTextSpanSummaryMismatch);
        }
        Ok(())
    }
}

mod lifecycle;

pub use lifecycle::*;

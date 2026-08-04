use crate::{ProviderFrameTextSpanV1, ProviderNarrativeReference, provider_narrative_chain_seed};

/// Exact verified prefix of one bounded completion-to-live narrative comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderNarrativeComparisonFrontier {
    compared_utf8_bytes: u64,
    verified_span_count: u64,
    verified_chain_digest: [u8; 32],
}

impl ProviderNarrativeComparisonFrontier {
    #[must_use]
    pub fn initial(narrative: ProviderNarrativeReference) -> Self {
        Self {
            compared_utf8_bytes: 0,
            verified_span_count: 0,
            verified_chain_digest: provider_narrative_chain_seed(
                narrative.content_id(),
                narrative.generation(),
            ),
        }
    }

    #[must_use]
    pub const fn from_stored_parts(
        compared_utf8_bytes: u64,
        verified_span_count: u64,
        verified_chain_digest: [u8; 32],
    ) -> Self {
        Self {
            compared_utf8_bytes,
            verified_span_count,
            verified_chain_digest,
        }
    }

    #[must_use]
    pub const fn compared_utf8_bytes(self) -> u64 {
        self.compared_utf8_bytes
    }

    #[must_use]
    pub const fn verified_span_count(self) -> u64 {
        self.verified_span_count
    }

    #[must_use]
    pub const fn verified_chain_digest(self) -> [u8; 32] {
        self.verified_chain_digest
    }
}

/// Durable state of one completion equality fence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderNarrativeCompletionState {
    Pending(ProviderNarrativeComparisonFrontier),
    Equal,
    Mismatch { utf8_byte_offset: u64 },
}

/// Terminal canonical result of one provider narrative completion fence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderNarrativeCompletionDisposition {
    Equal,
    Mismatch { utf8_byte_offset: u64 },
}

impl ProviderNarrativeCompletionDisposition {
    #[must_use]
    pub const fn is_mismatch(self) -> bool {
        matches!(self, Self::Mismatch { .. })
    }
}

impl ProviderNarrativeCompletionState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending(_))
    }
}

/// Completion's exact frame-local narrative evidence and equality state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderNarrativeCompletionCheck {
    source: Option<ProviderFrameTextSpanV1>,
    state: ProviderNarrativeCompletionState,
}

impl ProviderNarrativeCompletionCheck {
    #[must_use]
    pub const fn new(
        source: Option<ProviderFrameTextSpanV1>,
        state: ProviderNarrativeCompletionState,
    ) -> Self {
        Self { source, state }
    }

    #[must_use]
    pub const fn source(self) -> Option<ProviderFrameTextSpanV1> {
        self.source
    }

    #[must_use]
    pub const fn state(self) -> ProviderNarrativeCompletionState {
        self.state
    }

    #[must_use]
    pub const fn with_state(self, state: ProviderNarrativeCompletionState) -> Self {
        Self {
            source: self.source,
            state,
        }
    }

    #[must_use]
    pub const fn disposition(self) -> Option<ProviderNarrativeCompletionDisposition> {
        match self.state {
            ProviderNarrativeCompletionState::Pending(_) => None,
            ProviderNarrativeCompletionState::Equal => {
                Some(ProviderNarrativeCompletionDisposition::Equal)
            }
            ProviderNarrativeCompletionState::Mismatch { utf8_byte_offset } => {
                Some(ProviderNarrativeCompletionDisposition::Mismatch { utf8_byte_offset })
            }
        }
    }
}

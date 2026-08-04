use beryl_model::{CasItemId, ProviderObservationId};

use crate::{
    CasTurnSource, ProviderDeltaKind, ProviderFrameObservationSummaryV1, ProviderItemKind,
    ProviderObservationBegin, ProviderObservationBuildLifecycle, ProviderObservationBuildRecord,
    ProviderObservationDigest, ProviderObservationIssueReason, ProviderObservationItemKind,
    ProviderObservationItemLifecycle, SyndicRecordError,
};

/// Compact exact reference to one immutable sealed provider observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedProviderObservationReference {
    identity: ProviderObservationId,
    begin: ProviderObservationBegin,
    revision: u64,
    chunk_count: u64,
    canonical_bytes: u64,
    digest: ProviderObservationDigest,
}

impl SealedProviderObservationReference {
    pub(crate) fn from_build(build: &ProviderObservationBuildRecord) -> Self {
        debug_assert_eq!(build.lifecycle(), ProviderObservationBuildLifecycle::Sealed);
        Self {
            identity: build.identity(),
            begin: build.begin(),
            revision: build.revision(),
            chunk_count: build.chunk_count(),
            canonical_bytes: build.canonical_bytes(),
            digest: build.digest(),
        }
    }

    pub(crate) fn from_stored_parts(
        identity: ProviderObservationId,
        begin: ProviderObservationBegin,
        revision: u64,
        chunk_count: u64,
        canonical_bytes: u64,
        digest: ProviderObservationDigest,
    ) -> Result<Self, SyndicRecordError> {
        let expected_revision = chunk_count
            .checked_add(2)
            .ok_or(SyndicRecordError::InvalidProviderObservationFrontier)?;
        if revision != expected_revision {
            return Err(SyndicRecordError::InvalidProviderObservationFrontier);
        }
        Ok(Self {
            identity,
            begin,
            revision,
            chunk_count,
            canonical_bytes,
            digest,
        })
    }

    #[must_use]
    pub const fn identity(&self) -> ProviderObservationId {
        self.identity
    }

    #[must_use]
    pub const fn begin(&self) -> ProviderObservationBegin {
        self.begin
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn chunk_count(&self) -> u64 {
        self.chunk_count
    }

    #[must_use]
    pub const fn canonical_bytes(&self) -> u64 {
        self.canonical_bytes
    }

    #[must_use]
    pub const fn digest(&self) -> ProviderObservationDigest {
        self.digest
    }

    pub(crate) fn matches_build(&self, build: &ProviderObservationBuildRecord) -> bool {
        build.lifecycle() == ProviderObservationBuildLifecycle::Sealed
            && self.identity == build.identity()
            && self.begin == build.begin()
            && self.revision == build.revision()
            && self.chunk_count == build.chunk_count()
            && self.canonical_bytes == build.canonical_bytes()
            && self.digest == build.digest()
    }
}

/// Durable evidence that one sealed exact-route observation conflicts with item lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObservationIssue {
    observation: SealedProviderObservationReference,
    source: CasTurnSource,
    item_id: CasItemId,
    item_kind: ProviderItemKind,
    lifecycle: ProviderFrameObservationSummaryV1,
    reason: ProviderObservationIssueReason,
}

impl ProviderObservationIssue {
    pub(crate) fn from_inspected(
        build: &ProviderObservationBuildRecord,
        source: CasTurnSource,
        item_id: CasItemId,
        item_kind: ProviderItemKind,
        lifecycle: ProviderFrameObservationSummaryV1,
        reason: ProviderObservationIssueReason,
    ) -> Self {
        Self {
            observation: SealedProviderObservationReference::from_build(build),
            source,
            item_id,
            item_kind,
            lifecycle,
            reason,
        }
    }

    pub(crate) fn from_stored_parts(
        observation: SealedProviderObservationReference,
        source: CasTurnSource,
        item_id: CasItemId,
        item_kind: ProviderItemKind,
        lifecycle: ProviderFrameObservationSummaryV1,
        reason: ProviderObservationIssueReason,
    ) -> Result<Self, SyndicRecordError> {
        if provider_observation_item_kind(observation.begin()) != item_kind
            || !observation_lifecycle_matches(observation.begin(), lifecycle)
        {
            return Err(SyndicRecordError::InvalidProviderObservationFrontier);
        }
        Ok(Self {
            observation,
            source,
            item_id,
            item_kind,
            lifecycle,
            reason,
        })
    }

    #[must_use]
    pub const fn observation(&self) -> &SealedProviderObservationReference {
        &self.observation
    }

    #[must_use]
    pub const fn source(&self) -> &CasTurnSource {
        &self.source
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
    pub const fn lifecycle(&self) -> ProviderFrameObservationSummaryV1 {
        self.lifecycle
    }

    #[must_use]
    pub const fn reason(&self) -> ProviderObservationIssueReason {
        self.reason
    }
}

pub(crate) const fn provider_observation_item_kind(
    begin: ProviderObservationBegin,
) -> ProviderItemKind {
    match begin {
        ProviderObservationBegin::Item { kind, .. } => match kind {
            ProviderObservationItemKind::HookPrompt => ProviderItemKind::HookPrompt,
            ProviderObservationItemKind::AgentMessage => ProviderItemKind::AgentMessage,
            ProviderObservationItemKind::Plan => ProviderItemKind::Plan,
            ProviderObservationItemKind::Reasoning => ProviderItemKind::Reasoning,
            ProviderObservationItemKind::CommandExecution => ProviderItemKind::CommandExecution,
            ProviderObservationItemKind::FileChange => ProviderItemKind::FileChange,
            ProviderObservationItemKind::McpToolCall => ProviderItemKind::McpToolCall,
            ProviderObservationItemKind::DynamicToolCall => ProviderItemKind::DynamicToolCall,
            ProviderObservationItemKind::CollabAgentToolCall => {
                ProviderItemKind::CollabAgentToolCall
            }
            ProviderObservationItemKind::SubAgentActivity => ProviderItemKind::SubAgentActivity,
            ProviderObservationItemKind::WebSearch => ProviderItemKind::WebSearch,
            ProviderObservationItemKind::ImageView => ProviderItemKind::ImageView,
            ProviderObservationItemKind::Sleep => ProviderItemKind::Sleep,
            ProviderObservationItemKind::StandaloneImageGeneration => {
                ProviderItemKind::StandaloneImageGeneration
            }
            ProviderObservationItemKind::EnteredReviewMode => ProviderItemKind::EnteredReviewMode,
            ProviderObservationItemKind::ExitedReviewMode => ProviderItemKind::ExitedReviewMode,
            ProviderObservationItemKind::ContextCompaction => ProviderItemKind::ContextCompaction,
        },
        ProviderObservationBegin::Delta { kind } => match kind {
            ProviderDeltaKind::AgentMessage => ProviderItemKind::AgentMessage,
            ProviderDeltaKind::Plan => ProviderItemKind::Plan,
            ProviderDeltaKind::ReasoningSummaryPartAdded
            | ProviderDeltaKind::ReasoningSummaryText
            | ProviderDeltaKind::ReasoningTextObserved => ProviderItemKind::Reasoning,
            ProviderDeltaKind::CommandExecutionOutput => ProviderItemKind::CommandExecution,
            ProviderDeltaKind::FileChangeOutput | ProviderDeltaKind::FileChangePatchUpdated => {
                ProviderItemKind::FileChange
            }
            ProviderDeltaKind::McpToolCallProgress => ProviderItemKind::McpToolCall,
        },
    }
}

const fn observation_lifecycle_matches(
    begin: ProviderObservationBegin,
    lifecycle: ProviderFrameObservationSummaryV1,
) -> bool {
    matches!(
        (begin, lifecycle),
        (
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Started,
                ..
            },
            ProviderFrameObservationSummaryV1::Started(_)
        ) | (
            ProviderObservationBegin::Item {
                lifecycle: ProviderObservationItemLifecycle::Completed,
                ..
            },
            ProviderFrameObservationSummaryV1::Completed(_)
        ) | (
            ProviderObservationBegin::Delta { .. },
            ProviderFrameObservationSummaryV1::Delta
        )
    )
}

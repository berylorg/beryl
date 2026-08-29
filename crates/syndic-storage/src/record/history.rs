use beryl_model::{
    DiscussionContextOwnerId, DraftRevision, InputGateRevision, SealedAssetReferenceSetProof,
    SyndicAcceptedInputId, SyndicDraftId, SyndicPathDigest, SyndicThreadId, SyndicTurnId,
    ThreadRevision,
};

use crate::{
    AcceptedInputOrdinal, AcceptedRouteGeneration, ContentReference, ContextEnvelopeRevision,
    ConversationParent, CurrentTranscriptEntryProof, DiscussionContextEnvelope, SelectedPathProof,
    SyndicTimestamp, ThreadLineageDepth, TurnDepth, TurnEndStatus, TurnIncompleteReason, TurnKind,
    TurnLifecycle, TurnStateRevision, TurnTerminalOutcome,
};

mod accepted;
mod thread;

pub use accepted::*;
pub use thread::*;

/// Durable replacement-edit intent kept separate from mutable composer content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplacementEditIntent {
    target_turn_id: SyndicTurnId,
    selected_path: SelectedPathProof,
    transcript_entry: CurrentTranscriptEntryProof,
}

impl ReplacementEditIntent {
    #[must_use]
    pub fn new(
        target_turn_id: SyndicTurnId,
        selected_path: SelectedPathProof,
        transcript_entry: CurrentTranscriptEntryProof,
    ) -> Self {
        Self {
            target_turn_id,
            selected_path,
            transcript_entry,
        }
    }

    #[must_use]
    pub const fn target_turn_id(self) -> SyndicTurnId {
        self.target_turn_id
    }

    #[must_use]
    pub const fn selected_path(self) -> SelectedPathProof {
        self.selected_path
    }

    #[must_use]
    pub const fn transcript_entry(self) -> CurrentTranscriptEntryProof {
        self.transcript_entry
    }
}

/// Closed submission behavior owned by one current draft.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftSubmissionIntent {
    /// Submit against the transaction-current thread tail.
    Ordinary,
    /// Submit the first discussion turn against the immutable context source.
    DiscussionContext(DiscussionContextOwnerId),
    /// Replace one exact selected-path user turn.
    Replacement(ReplacementEditIntent),
}

/// Exactly one durable mutable pre-submission record owned by a thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftRecord {
    id: SyndicDraftId,
    thread_id: SyndicThreadId,
    revision: DraftRevision,
    submission_intent: DraftSubmissionIntent,
    root_history: crate::DraftRootHistoryPairV1,
    created_at: SyndicTimestamp,
    updated_at: SyndicTimestamp,
}

impl DraftRecord {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        id: SyndicDraftId,
        thread_id: SyndicThreadId,
        revision: DraftRevision,
        submission_intent: DraftSubmissionIntent,
        root_history: crate::DraftRootHistoryPairV1,
        created_at: SyndicTimestamp,
        updated_at: SyndicTimestamp,
    ) -> Self {
        Self {
            id,
            thread_id,
            revision,
            submission_intent,
            root_history,
            created_at,
            updated_at,
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn id(&self) -> SyndicDraftId {
        self.id
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn revision(&self) -> DraftRevision {
        self.revision
    }
    #[must_use]
    pub const fn submission_intent(&self) -> DraftSubmissionIntent {
        self.submission_intent
    }
    #[must_use]
    pub const fn piece_root(&self) -> crate::DraftPieceRootReferenceV1 {
        self.root_history.root()
    }
    #[must_use]
    pub const fn history(&self) -> crate::DraftEditHistoryFrontierReferenceV1 {
        self.root_history.history()
    }
    #[must_use]
    pub const fn root_history(&self) -> crate::DraftRootHistoryPairV1 {
        self.root_history
    }
    #[must_use]
    pub const fn created_at(&self) -> SyndicTimestamp {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> SyndicTimestamp {
        self.updated_at
    }
}

/// Immutable selected-context envelope keyed by its typed owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextEnvelopeRecord {
    owner: DiscussionContextOwnerId,
    revision: ContextEnvelopeRevision,
    envelope: DiscussionContextEnvelope,
}

impl ContextEnvelopeRecord {
    #[must_use]
    pub const fn new(
        owner: DiscussionContextOwnerId,
        revision: ContextEnvelopeRevision,
        envelope: DiscussionContextEnvelope,
    ) -> Self {
        Self {
            owner,
            revision,
            envelope,
        }
    }
    #[must_use]
    pub const fn owner(&self) -> DiscussionContextOwnerId {
        self.owner
    }
    #[must_use]
    pub const fn revision(&self) -> ContextEnvelopeRevision {
        self.revision
    }
    #[must_use]
    pub const fn envelope(&self) -> &DiscussionContextEnvelope {
        &self.envelope
    }
}

/// Immutable topology and ownership header of one submitted turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnRecord {
    id: SyndicTurnId,
    origin_thread_id: SyndicThreadId,
    kind: TurnKind,
    parent: ConversationParent,
    ancestor_skip: Option<SyndicTurnId>,
    depth: TurnDepth,
    chain_digest: SyndicPathDigest,
    submitted_at: SyndicTimestamp,
}

impl TurnRecord {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        id: SyndicTurnId,
        origin_thread_id: SyndicThreadId,
        kind: TurnKind,
        parent: ConversationParent,
        ancestor_skip: Option<SyndicTurnId>,
        depth: TurnDepth,
        chain_digest: SyndicPathDigest,
        submitted_at: SyndicTimestamp,
    ) -> Self {
        Self {
            id,
            origin_thread_id,
            kind,
            parent,
            ancestor_skip,
            depth,
            chain_digest,
            submitted_at,
        }
    }
    #[must_use]
    pub const fn id(&self) -> SyndicTurnId {
        self.id
    }
    #[must_use]
    pub const fn origin_thread_id(&self) -> SyndicThreadId {
        self.origin_thread_id
    }
    #[must_use]
    pub const fn kind(&self) -> TurnKind {
        self.kind
    }
    #[must_use]
    pub const fn parent(&self) -> ConversationParent {
        self.parent
    }
    #[must_use]
    pub const fn ancestor_skip(&self) -> Option<SyndicTurnId> {
        self.ancestor_skip
    }
    #[must_use]
    pub const fn depth(&self) -> TurnDepth {
        self.depth
    }
    #[must_use]
    pub const fn chain_digest(&self) -> SyndicPathDigest {
        self.chain_digest
    }
    #[must_use]
    pub const fn submitted_at(&self) -> SyndicTimestamp {
        self.submitted_at
    }
}

/// Mutable lifecycle and contiguous frontier facts kept separate from topology.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnStateRecord {
    turn_id: SyndicTurnId,
    revision: TurnStateRevision,
    lifecycle: TurnLifecycle,
    source_event_count: u64,
    item_count: u64,
    finalized_item_count: u64,
    open_item_count: u64,
    history_blocking_item_count: u64,
    provider_observation_issue: Option<crate::ProviderObservationIssueReason>,
    end_status: Option<TurnEndStatus>,
    updated_at: SyndicTimestamp,
}

impl TurnStateRecord {
    pub fn new(
        turn_id: SyndicTurnId,
        revision: TurnStateRevision,
        lifecycle: TurnLifecycle,
        source_event_count: u64,
        item_count: u64,
        end_status: Option<TurnEndStatus>,
        updated_at: SyndicTimestamp,
    ) -> Result<Self, crate::SyndicRecordError> {
        Self::with_capture_frontiers(
            turn_id,
            revision,
            lifecycle,
            source_event_count,
            item_count,
            item_count,
            0,
            0,
            end_status,
            updated_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_finalization_frontier(
        turn_id: SyndicTurnId,
        revision: TurnStateRevision,
        lifecycle: TurnLifecycle,
        source_event_count: u64,
        item_count: u64,
        finalized_item_count: u64,
        end_status: Option<TurnEndStatus>,
        updated_at: SyndicTimestamp,
    ) -> Result<Self, crate::SyndicRecordError> {
        Self::with_capture_frontiers(
            turn_id,
            revision,
            lifecycle,
            source_event_count,
            item_count,
            finalized_item_count,
            0,
            0,
            end_status,
            updated_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_capture_frontiers(
        turn_id: SyndicTurnId,
        revision: TurnStateRevision,
        lifecycle: TurnLifecycle,
        source_event_count: u64,
        item_count: u64,
        finalized_item_count: u64,
        open_item_count: u64,
        history_blocking_item_count: u64,
        end_status: Option<TurnEndStatus>,
        updated_at: SyndicTimestamp,
    ) -> Result<Self, crate::SyndicRecordError> {
        Self::with_capture_frontiers_and_issue(
            turn_id,
            revision,
            lifecycle,
            source_event_count,
            item_count,
            finalized_item_count,
            open_item_count,
            history_blocking_item_count,
            None,
            end_status,
            updated_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_capture_frontiers_and_issue(
        turn_id: SyndicTurnId,
        revision: TurnStateRevision,
        lifecycle: TurnLifecycle,
        source_event_count: u64,
        item_count: u64,
        finalized_item_count: u64,
        open_item_count: u64,
        history_blocking_item_count: u64,
        provider_observation_issue: Option<crate::ProviderObservationIssueReason>,
        end_status: Option<TurnEndStatus>,
        updated_at: SyndicTimestamp,
    ) -> Result<Self, crate::SyndicRecordError> {
        if finalized_item_count > item_count
            || open_item_count > item_count
            || history_blocking_item_count > item_count
            || !turn_end_status_matches_lifecycle(lifecycle, end_status)
            || (provider_observation_issue.is_some()
                && end_status.is_some_and(|status| status.incomplete_reason().is_none()))
        {
            return Err(crate::SyndicRecordError::InvalidTurnCaptureFrontier);
        }
        Ok(Self {
            turn_id,
            revision,
            lifecycle,
            source_event_count,
            item_count,
            finalized_item_count,
            open_item_count,
            history_blocking_item_count,
            provider_observation_issue,
            end_status,
            updated_at,
        })
    }
    #[must_use]
    pub const fn turn_id(&self) -> SyndicTurnId {
        self.turn_id
    }
    #[must_use]
    pub const fn revision(&self) -> TurnStateRevision {
        self.revision
    }
    #[must_use]
    pub const fn lifecycle(&self) -> TurnLifecycle {
        self.lifecycle
    }
    #[must_use]
    pub const fn source_event_count(&self) -> u64 {
        self.source_event_count
    }
    #[must_use]
    pub const fn item_count(&self) -> u64 {
        self.item_count
    }
    #[must_use]
    pub const fn finalized_item_count(&self) -> u64 {
        self.finalized_item_count
    }
    #[must_use]
    pub const fn open_item_count(&self) -> u64 {
        self.open_item_count
    }
    #[must_use]
    pub const fn history_blocking_item_count(&self) -> u64 {
        self.history_blocking_item_count
    }
    #[must_use]
    pub const fn provider_observation_issue(
        &self,
    ) -> Option<crate::ProviderObservationIssueReason> {
        self.provider_observation_issue
    }
    #[must_use]
    pub const fn end_status(&self) -> Option<TurnEndStatus> {
        self.end_status
    }
    #[must_use]
    pub const fn terminal_outcome(&self) -> Option<TurnTerminalOutcome> {
        match self.end_status {
            Some(status) => Some(status.outcome()),
            None => None,
        }
    }
    #[must_use]
    pub const fn incomplete_reason(&self) -> Option<TurnIncompleteReason> {
        match self.end_status {
            Some(status) => status.incomplete_reason(),
            None => None,
        }
    }
    #[must_use]
    pub const fn updated_at(&self) -> SyndicTimestamp {
        self.updated_at
    }
}

fn turn_end_status_matches_lifecycle(
    lifecycle: TurnLifecycle,
    end_status: Option<TurnEndStatus>,
) -> bool {
    match end_status {
        Some(status) => status.lifecycle() == lifecycle,
        None => matches!(lifecycle, TurnLifecycle::Pending | TurnLifecycle::Active),
    }
}

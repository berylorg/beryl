use beryl_model::{
    AcceptedInputRevision, DiscussionContextOwnerId, DraftRevision, InputGateRevision,
    SyndicAcceptedInputId, SyndicDraftId, SyndicPathDigest, SyndicThreadId, SyndicTurnId,
    ThreadRevision,
};

use crate::{
    AcceptedInputDisposition, AcceptedInputLifecycle, AcceptedInputOrdinal, ContentReference,
    ContextEnvelopeRevision, ConversationParent, CurrentTranscriptEntryProof,
    DiscussionContextEnvelope, SelectedPathProof, SyndicTimestamp, TurnDepth, TurnEndStatus,
    TurnIncompleteReason, TurnKind, TurnLifecycle, TurnStateRevision, TurnTerminalOutcome,
};

/// Durable replacement-edit intent kept separate from mutable composer content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplacementEditIntent {
    target_turn_id: SyndicTurnId,
    selected_path: SelectedPathProof,
    transcript_entry: CurrentTranscriptEntryProof,
}

impl ReplacementEditIntent {
    #[must_use]
    pub const fn new(
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

/// Authoritative mutable bindings for one named Syndic thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadRecord {
    id: SyndicThreadId,
    revision: ThreadRevision,
    committed_tail: Option<SyndicTurnId>,
    current_draft_id: SyndicDraftId,
    parent_thread_id: Option<SyndicThreadId>,
    context_owner_id: Option<DiscussionContextOwnerId>,
    selected_path_digest: SyndicPathDigest,
}

impl ThreadRecord {
    #[must_use]
    pub const fn new(
        id: SyndicThreadId,
        revision: ThreadRevision,
        committed_tail: Option<SyndicTurnId>,
        current_draft_id: SyndicDraftId,
        parent_thread_id: Option<SyndicThreadId>,
        context_owner_id: Option<DiscussionContextOwnerId>,
        selected_path_digest: SyndicPathDigest,
    ) -> Self {
        Self {
            id,
            revision,
            committed_tail,
            current_draft_id,
            parent_thread_id,
            context_owner_id,
            selected_path_digest,
        }
    }
    #[must_use]
    pub const fn id(&self) -> SyndicThreadId {
        self.id
    }
    #[must_use]
    pub const fn revision(&self) -> ThreadRevision {
        self.revision
    }
    #[must_use]
    pub const fn committed_tail(&self) -> Option<SyndicTurnId> {
        self.committed_tail
    }
    #[must_use]
    pub const fn current_draft_id(&self) -> SyndicDraftId {
        self.current_draft_id
    }
    #[must_use]
    pub const fn parent_thread_id(&self) -> Option<SyndicThreadId> {
        self.parent_thread_id
    }
    #[must_use]
    pub const fn context_owner_id(&self) -> Option<DiscussionContextOwnerId> {
        self.context_owner_id
    }
    #[must_use]
    pub const fn selected_path_digest(&self) -> SyndicPathDigest {
        self.selected_path_digest
    }
}

/// Exactly one durable mutable pre-submission record owned by a thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftRecord {
    id: SyndicDraftId,
    thread_id: SyndicThreadId,
    revision: DraftRevision,
    parent: ConversationParent,
    context_owner_id: Option<DiscussionContextOwnerId>,
    replacement_edit_intent: Option<ReplacementEditIntent>,
    content: ContentReference,
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
        parent: ConversationParent,
        context_owner_id: Option<DiscussionContextOwnerId>,
        replacement_edit_intent: Option<ReplacementEditIntent>,
        content: ContentReference,
        created_at: SyndicTimestamp,
        updated_at: SyndicTimestamp,
    ) -> Self {
        Self {
            id,
            thread_id,
            revision,
            parent,
            context_owner_id,
            replacement_edit_intent,
            content,
            created_at,
            updated_at,
        }
    }
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
    pub const fn parent(&self) -> ConversationParent {
        self.parent
    }
    #[must_use]
    pub const fn context_owner_id(&self) -> Option<DiscussionContextOwnerId> {
        self.context_owner_id
    }
    #[must_use]
    pub const fn replacement_edit_intent(&self) -> Option<ReplacementEditIntent> {
        self.replacement_edit_intent
    }
    #[must_use]
    pub const fn content(&self) -> ContentReference {
        self.content
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
    end_status: Option<TurnEndStatus>,
    updated_at: SyndicTimestamp,
}

impl TurnStateRecord {
    #[must_use]
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

    #[must_use]
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
        if finalized_item_count > item_count
            || open_item_count > item_count
            || history_blocking_item_count > item_count
            || !turn_end_status_matches_lifecycle(lifecycle, end_status)
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

/// One identity-preserving input fragment accepted during an active or queued lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedInputRecord {
    id: SyndicAcceptedInputId,
    thread_id: SyndicThreadId,
    revision: AcceptedInputRevision,
    ordinal: AcceptedInputOrdinal,
    gate_revision: InputGateRevision,
    disposition: AcceptedInputDisposition,
    lifecycle: AcceptedInputLifecycle,
    content: ContentReference,
    marker_count: u64,
    admitted_at: SyndicTimestamp,
}

impl AcceptedInputRecord {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        id: SyndicAcceptedInputId,
        thread_id: SyndicThreadId,
        revision: AcceptedInputRevision,
        ordinal: AcceptedInputOrdinal,
        gate_revision: InputGateRevision,
        disposition: AcceptedInputDisposition,
        lifecycle: AcceptedInputLifecycle,
        content: ContentReference,
        marker_count: u64,
        admitted_at: SyndicTimestamp,
    ) -> Self {
        Self {
            id,
            thread_id,
            revision,
            ordinal,
            gate_revision,
            disposition,
            lifecycle,
            content,
            marker_count,
            admitted_at,
        }
    }
    #[must_use]
    pub const fn id(&self) -> SyndicAcceptedInputId {
        self.id
    }
    #[must_use]
    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn revision(&self) -> AcceptedInputRevision {
        self.revision
    }
    #[must_use]
    pub const fn ordinal(&self) -> AcceptedInputOrdinal {
        self.ordinal
    }
    #[must_use]
    pub const fn gate_revision(&self) -> InputGateRevision {
        self.gate_revision
    }
    #[must_use]
    pub const fn disposition(&self) -> &AcceptedInputDisposition {
        &self.disposition
    }
    #[must_use]
    pub const fn lifecycle(&self) -> AcceptedInputLifecycle {
        self.lifecycle
    }
    #[must_use]
    pub const fn content(&self) -> ContentReference {
        self.content
    }
    #[must_use]
    pub const fn marker_count(&self) -> u64 {
        self.marker_count
    }
    #[must_use]
    pub const fn admitted_at(&self) -> SyndicTimestamp {
        self.admitted_at
    }
}

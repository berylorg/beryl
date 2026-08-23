use super::{
    builder_model::DraftPieceDurableBuildContinuationV1,
    staging_model::DraftMutationStagingProgressReceiptReferenceV1,
};
use beryl_model::{
    DraftRevision, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId, SyndicThreadId,
    ThreadRevision,
};

use super::history::{
    DraftEditHistoryFrontierReferenceV1, DraftEditHistoryFrontierV1, DraftEditHistoryTransitionV1,
    DraftRootHistoryPairV1,
};
use crate::SyndicTimestamp;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPieceOperationIdV1([u8; 16]);

impl DraftPieceOperationIdV1 {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftEditorCandidateSessionIdV1([u8; 16]);

impl DraftEditorCandidateSessionIdV1 {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DraftEditorCandidateSessionRecordKeyV1 {
    Head {
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
    },
    OpenReceipt {
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    },
    PublicationReceipt {
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    },
    DisposalReceipt {
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    },
}

impl DraftEditorCandidateSessionRecordKeyV1 {
    pub const fn head(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
    ) -> Self {
        Self::Head {
            draft_id,
            session_id,
        }
    }

    pub const fn open_receipt(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    ) -> Self {
        Self::OpenReceipt {
            draft_id,
            session_id,
            operation_id,
        }
    }

    pub const fn publication_receipt(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    ) -> Self {
        Self::PublicationReceipt {
            draft_id,
            session_id,
            operation_id,
        }
    }

    pub const fn disposal_receipt(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    ) -> Self {
        Self::DisposalReceipt {
            draft_id,
            session_id,
            operation_id,
        }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        match self {
            Self::Head { draft_id, .. }
            | Self::OpenReceipt { draft_id, .. }
            | Self::PublicationReceipt { draft_id, .. }
            | Self::DisposalReceipt { draft_id, .. } => draft_id,
        }
    }

    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        match self {
            Self::Head { session_id, .. }
            | Self::OpenReceipt { session_id, .. }
            | Self::PublicationReceipt { session_id, .. }
            | Self::DisposalReceipt { session_id, .. } => session_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftEditorCandidateSessionLifecycleV1 {
    Active,
    Disposed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftEditorActiveOperationV1 {
    Staging {
        operation_id: DraftPieceOperationIdV1,
        begin_digest: DraftPieceDigestV1,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        receipt: DraftMutationStagingProgressReceiptReferenceV1,
    },
    Building {
        operation_id: DraftPieceOperationIdV1,
        proposal_digest: DraftPieceDigestV1,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        receipt: DraftPieceBuildProgressReceiptReferenceV1,
    },
}

impl DraftEditorActiveOperationV1 {
    pub const fn staging(
        operation_id: DraftPieceOperationIdV1,
        begin_digest: DraftPieceDigestV1,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        receipt: DraftMutationStagingProgressReceiptReferenceV1,
    ) -> Self {
        Self::Staging {
            operation_id,
            begin_digest,
            predecessor_candidate_generation,
            predecessor_root,
            predecessor_history,
            receipt,
        }
    }

    pub const fn building(
        operation_id: DraftPieceOperationIdV1,
        proposal_digest: DraftPieceDigestV1,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        receipt: DraftPieceBuildProgressReceiptReferenceV1,
    ) -> Self {
        Self::Building {
            operation_id,
            proposal_digest,
            predecessor_candidate_generation,
            predecessor_root,
            predecessor_history,
            receipt,
        }
    }

    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        match self {
            Self::Staging { operation_id, .. } | Self::Building { operation_id, .. } => {
                operation_id
            }
        }
    }

    pub const fn proposal_digest(self) -> Option<DraftPieceDigestV1> {
        match self {
            Self::Staging { .. } => None,
            Self::Building {
                proposal_digest, ..
            } => Some(proposal_digest),
        }
    }

    pub const fn begin_digest(self) -> Option<DraftPieceDigestV1> {
        match self {
            Self::Staging { begin_digest, .. } => Some(begin_digest),
            Self::Building { .. } => None,
        }
    }

    pub const fn predecessor_candidate_generation(self) -> u64 {
        match self {
            Self::Staging {
                predecessor_candidate_generation,
                ..
            }
            | Self::Building {
                predecessor_candidate_generation,
                ..
            } => predecessor_candidate_generation,
        }
    }

    pub const fn predecessor_root(self) -> DraftPieceRootReferenceV1 {
        match self {
            Self::Staging {
                predecessor_root, ..
            }
            | Self::Building {
                predecessor_root, ..
            } => predecessor_root,
        }
    }

    pub const fn build_receipt(self) -> Option<DraftPieceBuildProgressReceiptReferenceV1> {
        match self {
            Self::Staging { .. } => None,
            Self::Building { receipt, .. } => Some(receipt),
        }
    }

    pub const fn predecessor_history(self) -> DraftEditHistoryFrontierReferenceV1 {
        match self {
            Self::Staging {
                predecessor_history,
                ..
            }
            | Self::Building {
                predecessor_history,
                ..
            } => predecessor_history,
        }
    }

    pub const fn staging_receipt(self) -> Option<DraftMutationStagingProgressReceiptReferenceV1> {
        match self {
            Self::Staging { receipt, .. } => Some(receipt),
            Self::Building { .. } => None,
        }
    }

    pub fn same_operation(&self, other: &Self) -> bool {
        self.operation_id() == other.operation_id()
            && self.predecessor_candidate_generation() == other.predecessor_candidate_generation()
            && self.predecessor_root() == other.predecessor_root()
            && self.predecessor_history() == other.predecessor_history()
    }

    pub const fn is_staging(self) -> bool {
        matches!(self, Self::Staging { .. })
    }

    pub const fn is_building(self) -> bool {
        matches!(self, Self::Building { .. })
    }

    fn endpoint_is_owned(
        self,
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
    ) -> bool {
        match self {
            Self::Staging { receipt, .. } => {
                receipt.identity().draft_id() == draft_id
                    && receipt.identity().session_id() == session_id
                    && receipt.identity().operation_id().as_piece_operation() == self.operation_id()
            }
            Self::Building { receipt, .. } => {
                receipt.key().draft_id() == draft_id
                    && receipt.key().session_id() == session_id
                    && receipt.key().operation_id() == self.operation_id()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditorCurrentSelectorV1 {
    thread_id: SyndicThreadId,
    thread_revision: ThreadRevision,
    draft_id: SyndicDraftId,
    selector_revision: DraftRevision,
    root: DraftPieceRootReferenceV1,
    history: DraftEditHistoryFrontierReferenceV1,
}

impl DraftEditorCurrentSelectorV1 {
    pub const fn new(
        thread_id: SyndicThreadId,
        thread_revision: ThreadRevision,
        draft_id: SyndicDraftId,
        selector_revision: DraftRevision,
        root: DraftPieceRootReferenceV1,
        history: DraftEditHistoryFrontierReferenceV1,
    ) -> Self {
        Self {
            thread_id,
            thread_revision,
            draft_id,
            selector_revision,
            root,
            history,
        }
    }

    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn thread_revision(self) -> ThreadRevision {
        self.thread_revision
    }
    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn selector_revision(self) -> DraftRevision {
        self.selector_revision
    }
    pub const fn root(self) -> DraftPieceRootReferenceV1 {
        self.root
    }
    pub const fn history(self) -> DraftEditHistoryFrontierReferenceV1 {
        self.history
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidateSessionOpenRequestV1 {
    selector: DraftEditorCurrentSelectorV1,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
}

impl DraftEditorCandidateSessionOpenRequestV1 {
    pub const fn new(
        selector: DraftEditorCurrentSelectorV1,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    ) -> Self {
        Self {
            selector,
            session_id,
            operation_id,
        }
    }

    pub const fn selector(self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }
    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidatePublicationRequestV1 {
    selector: DraftEditorCurrentSelectorV1,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
    candidate_generation: u64,
    candidate: DraftRootHistoryPairV1,
    published_at: SyndicTimestamp,
}

impl DraftEditorCandidatePublicationRequestV1 {
    pub const fn new(
        selector: DraftEditorCurrentSelectorV1,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
        candidate_generation: u64,
        candidate: DraftRootHistoryPairV1,
        published_at: SyndicTimestamp,
    ) -> Self {
        Self {
            selector,
            session_id,
            operation_id,
            candidate_generation,
            candidate,
            published_at,
        }
    }
    pub const fn selector(self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }
    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }
    pub const fn candidate_generation(self) -> u64 {
        self.candidate_generation
    }
    pub const fn candidate(self) -> DraftRootHistoryPairV1 {
        self.candidate
    }
    pub const fn published_at(self) -> SyndicTimestamp {
        self.published_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidateSessionDisposeRequestV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
    expected_session_generation: u64,
    expected_pair: DraftRootHistoryPairV1,
}

impl DraftEditorCandidateSessionDisposeRequestV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
        expected_session_generation: u64,
        expected_pair: DraftRootHistoryPairV1,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            operation_id,
            expected_session_generation,
            expected_pair,
        }
    }
    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }
    pub const fn expected_session_generation(self) -> u64 {
        self.expected_session_generation
    }
    pub const fn expected_pair(self) -> DraftRootHistoryPairV1 {
        self.expected_pair
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidateSessionV1 {
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    open_operation_id: DraftPieceOperationIdV1,
    session_generation: u64,
    durable_base_selector_revision: DraftRevision,
    durable_base_root: DraftPieceRootReferenceV1,
    durable_base_history: DraftEditHistoryFrontierReferenceV1,
    published_candidate_generation: u64,
    published_selector_revision: DraftRevision,
    published_root: DraftPieceRootReferenceV1,
    published_history: DraftEditHistoryFrontierReferenceV1,
    newest_candidate_generation: u64,
    newest_root: DraftPieceRootReferenceV1,
    newest_history: DraftEditHistoryFrontierReferenceV1,
    dirty_generation: u64,
    logical_extent: DraftLogicalExtentV1,
    lifecycle: DraftEditorCandidateSessionLifecycleV1,
    disposal_operation_id: Option<DraftPieceOperationIdV1>,
    active_operation: Option<DraftEditorActiveOperationV1>,
}

impl DraftEditorCandidateSessionV1 {
    pub const fn opened(
        request: DraftEditorCandidateSessionOpenRequestV1,
        forked_history: DraftEditHistoryFrontierReferenceV1,
    ) -> Self {
        let selector = request.selector();
        let root = selector.root();
        let candidate_generation = selector.history().candidate_generation();
        let session_generation = match candidate_generation.checked_add(1) {
            Some(value) => value,
            None => 0,
        };
        Self {
            thread_id: selector.thread_id(),
            draft_id: selector.draft_id(),
            session_id: request.session_id(),
            open_operation_id: request.operation_id(),
            session_generation,
            durable_base_selector_revision: selector.selector_revision(),
            durable_base_root: root,
            durable_base_history: selector.history(),
            published_candidate_generation: candidate_generation,
            published_selector_revision: selector.selector_revision(),
            published_root: root,
            published_history: selector.history(),
            newest_candidate_generation: candidate_generation,
            newest_root: root,
            newest_history: forked_history,
            dirty_generation: 0,
            logical_extent: root.summary().logical_extent(),
            lifecycle: DraftEditorCandidateSessionLifecycleV1::Active,
            disposal_operation_id: None,
            active_operation: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_parts(
        thread_id: SyndicThreadId,
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        open_operation_id: DraftPieceOperationIdV1,
        session_generation: u64,
        durable_base_selector_revision: DraftRevision,
        durable_base_root: DraftPieceRootReferenceV1,
        durable_base_history: DraftEditHistoryFrontierReferenceV1,
        published_candidate_generation: u64,
        published_selector_revision: DraftRevision,
        published_root: DraftPieceRootReferenceV1,
        published_history: DraftEditHistoryFrontierReferenceV1,
        newest_candidate_generation: u64,
        newest_root: DraftPieceRootReferenceV1,
        newest_history: DraftEditHistoryFrontierReferenceV1,
        dirty_generation: u64,
        logical_extent: DraftLogicalExtentV1,
        lifecycle: DraftEditorCandidateSessionLifecycleV1,
        active_operation: Option<DraftEditorActiveOperationV1>,
    ) -> Self {
        Self::from_parts_with_disposal(
            thread_id,
            draft_id,
            session_id,
            open_operation_id,
            session_generation,
            durable_base_selector_revision,
            durable_base_root,
            durable_base_history,
            published_candidate_generation,
            published_selector_revision,
            published_root,
            published_history,
            newest_candidate_generation,
            newest_root,
            newest_history,
            dirty_generation,
            logical_extent,
            lifecycle,
            None,
            active_operation,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_parts_with_disposal(
        thread_id: SyndicThreadId,
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        open_operation_id: DraftPieceOperationIdV1,
        session_generation: u64,
        durable_base_selector_revision: DraftRevision,
        durable_base_root: DraftPieceRootReferenceV1,
        durable_base_history: DraftEditHistoryFrontierReferenceV1,
        published_candidate_generation: u64,
        published_selector_revision: DraftRevision,
        published_root: DraftPieceRootReferenceV1,
        published_history: DraftEditHistoryFrontierReferenceV1,
        newest_candidate_generation: u64,
        newest_root: DraftPieceRootReferenceV1,
        newest_history: DraftEditHistoryFrontierReferenceV1,
        dirty_generation: u64,
        logical_extent: DraftLogicalExtentV1,
        lifecycle: DraftEditorCandidateSessionLifecycleV1,
        disposal_operation_id: Option<DraftPieceOperationIdV1>,
        active_operation: Option<DraftEditorActiveOperationV1>,
    ) -> Self {
        Self {
            thread_id,
            draft_id,
            session_id,
            open_operation_id,
            session_generation,
            durable_base_selector_revision,
            durable_base_root,
            durable_base_history,
            published_candidate_generation,
            published_selector_revision,
            published_root,
            published_history,
            newest_candidate_generation,
            newest_root,
            newest_history,
            dirty_generation,
            logical_extent,
            lifecycle,
            disposal_operation_id,
            active_operation,
        }
    }

    pub const fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn session_id(&self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
    pub const fn open_operation_id(&self) -> DraftPieceOperationIdV1 {
        self.open_operation_id
    }
    pub const fn session_generation(&self) -> u64 {
        self.session_generation
    }
    pub const fn durable_base_selector_revision(&self) -> DraftRevision {
        self.durable_base_selector_revision
    }
    pub const fn durable_base_root(&self) -> DraftPieceRootReferenceV1 {
        self.durable_base_root
    }
    pub const fn durable_base_history(&self) -> DraftEditHistoryFrontierReferenceV1 {
        self.durable_base_history
    }
    pub const fn published_candidate_generation(&self) -> u64 {
        self.published_candidate_generation
    }
    pub const fn published_selector_revision(&self) -> DraftRevision {
        self.published_selector_revision
    }
    pub const fn published_root(&self) -> DraftPieceRootReferenceV1 {
        self.published_root
    }
    pub const fn published_history(&self) -> DraftEditHistoryFrontierReferenceV1 {
        self.published_history
    }
    pub const fn newest_candidate_generation(&self) -> u64 {
        self.newest_candidate_generation
    }
    pub const fn newest_root(&self) -> DraftPieceRootReferenceV1 {
        self.newest_root
    }
    pub const fn newest_history(&self) -> DraftEditHistoryFrontierReferenceV1 {
        self.newest_history
    }
    pub const fn dirty_generation(&self) -> u64 {
        self.dirty_generation
    }
    pub const fn logical_extent(&self) -> DraftLogicalExtentV1 {
        self.logical_extent
    }
    pub const fn lifecycle(&self) -> DraftEditorCandidateSessionLifecycleV1 {
        self.lifecycle
    }
    pub const fn disposal_operation_id(&self) -> Option<DraftPieceOperationIdV1> {
        self.disposal_operation_id
    }

    pub const fn active_operation(&self) -> Option<&DraftEditorActiveOperationV1> {
        self.active_operation.as_ref()
    }

    pub(crate) fn with_active_operation(
        &self,
        active_operation: DraftEditorActiveOperationV1,
    ) -> Option<Self> {
        if self.lifecycle != DraftEditorCandidateSessionLifecycleV1::Active
            || self
                .active_operation
                .as_ref()
                .is_some_and(|active| !active.same_operation(&active_operation))
        {
            return None;
        }
        let mut next = self.clone();
        next.session_generation = next.session_generation.checked_add(1)?;
        next.active_operation = Some(active_operation);
        Some(next)
    }

    pub(crate) fn with_active_operation_at_transition(
        &self,
        transition_ordinal: u64,
        active_operation: DraftEditorActiveOperationV1,
    ) -> Option<Self> {
        if transition_ordinal == 0
            || self.lifecycle != DraftEditorCandidateSessionLifecycleV1::Active
            || self.active_operation.is_some()
        {
            return None;
        }
        let mut next = self.clone();
        next.session_generation = next.session_generation.checked_add(transition_ordinal)?;
        next.active_operation = Some(active_operation);
        Some(next)
    }

    pub(crate) fn staging_to_building_at_transition(
        &self,
        expected_staging: &DraftEditorActiveOperationV1,
        target_building: DraftEditorActiveOperationV1,
        transition_ordinal: u64,
    ) -> Option<Self> {
        if transition_ordinal == 0
            || self.lifecycle != DraftEditorCandidateSessionLifecycleV1::Active
            || self.active_operation.as_ref() != Some(expected_staging)
            || !expected_staging.is_staging()
            || !target_building.is_building()
            || !expected_staging.same_operation(&target_building)
        {
            return None;
        }
        let mut next = self.clone();
        next.session_generation = next.session_generation.checked_add(transition_ordinal)?;
        next.active_operation = Some(target_building);
        Some(next)
    }

    pub(crate) fn advance_active_operation(
        &self,
        expected: &DraftEditorActiveOperationV1,
        target: DraftEditorActiveOperationV1,
    ) -> Option<Self> {
        if self.lifecycle != DraftEditorCandidateSessionLifecycleV1::Active
            || self.active_operation.as_ref() != Some(expected)
            || !expected.same_operation(&target)
        {
            return None;
        }
        let mut next = self.clone();
        next.session_generation = next.session_generation.checked_add(1)?;
        next.active_operation = Some(target);
        Some(next)
    }

    pub(crate) fn clear_active_operation(
        &self,
        expected: &DraftEditorActiveOperationV1,
    ) -> Option<Self> {
        if self.lifecycle != DraftEditorCandidateSessionLifecycleV1::Active
            || self.active_operation.as_ref() != Some(expected)
        {
            return None;
        }
        let mut next = self.clone();
        next.session_generation = next.session_generation.checked_add(1)?;
        next.active_operation = None;
        Some(next)
    }

    pub(crate) fn adopted(
        &self,
        successor: DraftPieceRootReferenceV1,
        successor_history: DraftEditHistoryFrontierReferenceV1,
    ) -> Option<Self> {
        if self.active_operation.is_none() {
            return None;
        }
        Some(Self {
            thread_id: self.thread_id,
            draft_id: self.draft_id,
            session_id: self.session_id,
            open_operation_id: self.open_operation_id,
            session_generation: self.session_generation.checked_add(1)?,
            durable_base_selector_revision: self.durable_base_selector_revision,
            durable_base_root: self.durable_base_root,
            durable_base_history: self.durable_base_history,
            published_candidate_generation: self.published_candidate_generation,
            published_selector_revision: self.published_selector_revision,
            published_root: self.published_root,
            published_history: self.published_history,
            newest_candidate_generation: self.newest_candidate_generation.checked_add(1)?,
            newest_root: successor,
            newest_history: successor_history,
            dirty_generation: self.dirty_generation.checked_add(1)?,
            logical_extent: successor.summary().logical_extent(),
            lifecycle: self.lifecycle,
            disposal_operation_id: self.disposal_operation_id,
            active_operation: None,
        })
    }

    pub(crate) fn adopted_without_custody(
        &self,
        successor: DraftPieceRootReferenceV1,
        successor_history: DraftEditHistoryFrontierReferenceV1,
    ) -> Option<Self> {
        if self.lifecycle != DraftEditorCandidateSessionLifecycleV1::Active
            || self.active_operation.is_some()
        {
            return None;
        }
        Some(Self {
            thread_id: self.thread_id,
            draft_id: self.draft_id,
            session_id: self.session_id,
            open_operation_id: self.open_operation_id,
            session_generation: self.session_generation.checked_add(1)?,
            durable_base_selector_revision: self.durable_base_selector_revision,
            durable_base_root: self.durable_base_root,
            durable_base_history: self.durable_base_history,
            published_candidate_generation: self.published_candidate_generation,
            published_selector_revision: self.published_selector_revision,
            published_root: self.published_root,
            published_history: self.published_history,
            newest_candidate_generation: self.newest_candidate_generation.checked_add(1)?,
            newest_root: successor,
            newest_history: successor_history,
            dirty_generation: self.dirty_generation.checked_add(1)?,
            logical_extent: successor.summary().logical_extent(),
            lifecycle: self.lifecycle,
            disposal_operation_id: self.disposal_operation_id,
            active_operation: None,
        })
    }

    pub(crate) fn published(
        &self,
        candidate_generation: u64,
        candidate: DraftRootHistoryPairV1,
        selector_revision: DraftRevision,
    ) -> Option<Self> {
        if self.lifecycle != DraftEditorCandidateSessionLifecycleV1::Active
            || self.active_operation.is_some()
            || candidate_generation <= self.published_candidate_generation
            || candidate_generation > self.newest_candidate_generation
            || candidate.history().candidate_generation() != candidate_generation
            || candidate.root() != candidate.history().root()
            || selector_revision <= self.published_selector_revision
        {
            return None;
        }
        let mut next = self.clone();
        next.session_generation = next.session_generation.checked_add(1)?;
        next.published_candidate_generation = candidate_generation;
        next.published_selector_revision = selector_revision;
        next.published_root = candidate.root();
        next.published_history = candidate.history();
        if candidate_generation == next.newest_candidate_generation {
            next.newest_root = candidate.root();
            next.newest_history = candidate.history();
        }
        Some(next)
    }

    pub(crate) fn disposed(&self, operation_id: DraftPieceOperationIdV1) -> Option<Self> {
        if self.lifecycle != DraftEditorCandidateSessionLifecycleV1::Active
            || self.active_operation.is_some()
            || self.published_candidate_generation != self.newest_candidate_generation
            || self.published_root != self.newest_root
            || self.published_history != self.newest_history
        {
            return None;
        }
        let mut next = self.clone();
        next.session_generation = next.session_generation.checked_add(1)?;
        next.lifecycle = DraftEditorCandidateSessionLifecycleV1::Disposed;
        next.disposal_operation_id = Some(operation_id);
        Some(next)
    }

    pub(crate) fn is_coherent(&self) -> bool {
        let generations_are_ordered = self.published_candidate_generation
            <= self.newest_candidate_generation
            && self.session_generation > self.newest_candidate_generation;
        let roots_are_owned = self.durable_base_root.key().draft_id() == self.draft_id
            && self.published_root.key().draft_id() == self.draft_id
            && self.newest_root.key().draft_id() == self.draft_id;
        let history_is_owned = self.durable_base_history.key().draft_id() == self.draft_id
            && self.published_history.key().draft_id() == self.draft_id
            && self.newest_history.key().draft_id() == self.draft_id;
        let published_is_coherent = if self.published_root == self.durable_base_root
            && self.published_history == self.durable_base_history
        {
            self.published_selector_revision == self.durable_base_selector_revision
        } else {
            self.published_selector_revision > self.durable_base_selector_revision
        };
        let newest_is_coherent = if self.newest_candidate_generation == 0 {
            self.newest_root == self.durable_base_root
                && self.newest_history.root() == self.durable_base_root
        } else {
            true
        };
        let shared_frontier_is_coherent = self.published_candidate_generation
            != self.newest_candidate_generation
            || (self.published_root == self.newest_root
                && self.published_history.root() == self.newest_history.root());
        let pairs_are_coherent = self.durable_base_history.root() == self.durable_base_root
            && self.published_history.root() == self.published_root
            && self.published_history.candidate_generation() == self.published_candidate_generation
            && self.newest_history.root() == self.newest_root
            && self.newest_history.candidate_generation() == self.newest_candidate_generation;
        let lifecycle_is_coherent = match self.lifecycle {
            DraftEditorCandidateSessionLifecycleV1::Active => self.disposal_operation_id.is_none(),
            DraftEditorCandidateSessionLifecycleV1::Disposed => {
                self.disposal_operation_id
                    .is_some_and(|operation_id| operation_id != self.open_operation_id)
                    && self.published_candidate_generation == self.newest_candidate_generation
                    && self.published_root == self.newest_root
                    && self.published_history == self.newest_history
                    && self.active_operation.is_none()
            }
        };
        self.session_generation != 0
            && generations_are_ordered
            && roots_are_owned
            && history_is_owned
            && pairs_are_coherent
            && published_is_coherent
            && newest_is_coherent
            && shared_frontier_is_coherent
            && lifecycle_is_coherent
            && self.logical_extent == self.newest_root.summary().logical_extent()
            && self.active_operation.as_ref().is_none_or(|operation| {
                operation.operation_id() != self.open_operation_id
                    && operation.predecessor_candidate_generation()
                        == self.newest_candidate_generation
                    && operation.predecessor_root() == self.newest_root
                    && operation.predecessor_history() == self.newest_history
                    && operation.endpoint_is_owned(self.draft_id, self.session_id)
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidateSessionOpenReceiptV1 {
    payload: DraftEditorCandidateSessionReceiptPayloadV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DraftEditorCandidateSessionReceiptPayloadV1 {
    Open {
        request_bytes: Vec<u8>,
        head: DraftEditorCandidateSessionV1,
    },
    Publication(Box<DraftEditorCandidatePublicationReceiptV1>),
    Disposal(Box<DraftEditorCandidateSessionDisposeReceiptV1>),
}

impl DraftEditorCandidateSessionOpenReceiptV1 {
    pub fn new(request_bytes: Vec<u8>, head: DraftEditorCandidateSessionV1) -> Self {
        Self {
            payload: DraftEditorCandidateSessionReceiptPayloadV1::Open {
                request_bytes,
                head,
            },
        }
    }
    pub fn request_bytes(&self) -> &[u8] {
        match &self.payload {
            DraftEditorCandidateSessionReceiptPayloadV1::Open { request_bytes, .. } => {
                request_bytes
            }
            DraftEditorCandidateSessionReceiptPayloadV1::Publication(receipt) => {
                receipt.request_bytes()
            }
            DraftEditorCandidateSessionReceiptPayloadV1::Disposal(receipt) => {
                receipt.request_bytes()
            }
        }
    }
    pub fn head(&self) -> &DraftEditorCandidateSessionV1 {
        match &self.payload {
            DraftEditorCandidateSessionReceiptPayloadV1::Open { head, .. } => head,
            DraftEditorCandidateSessionReceiptPayloadV1::Publication(receipt) => {
                receipt.before_head()
            }
            DraftEditorCandidateSessionReceiptPayloadV1::Disposal(receipt) => receipt.before_head(),
        }
    }
    pub(crate) fn from_publication(receipt: DraftEditorCandidatePublicationReceiptV1) -> Self {
        Self {
            payload: DraftEditorCandidateSessionReceiptPayloadV1::Publication(Box::new(receipt)),
        }
    }
    pub(crate) fn from_disposal(receipt: DraftEditorCandidateSessionDisposeReceiptV1) -> Self {
        Self {
            payload: DraftEditorCandidateSessionReceiptPayloadV1::Disposal(Box::new(receipt)),
        }
    }
    pub(crate) fn publication(&self) -> Option<&DraftEditorCandidatePublicationReceiptV1> {
        match &self.payload {
            DraftEditorCandidateSessionReceiptPayloadV1::Publication(v) => Some(v),
            _ => None,
        }
    }
    pub(crate) fn disposal(&self) -> Option<&DraftEditorCandidateSessionDisposeReceiptV1> {
        match &self.payload {
            DraftEditorCandidateSessionReceiptPayloadV1::Disposal(v) => Some(v),
            _ => None,
        }
    }
    pub(crate) fn is_open(&self) -> bool {
        matches!(
            self.payload,
            DraftEditorCandidateSessionReceiptPayloadV1::Open { .. }
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidatePublicationReceiptV1 {
    request_bytes: Vec<u8>,
    prior_selector: DraftEditorCurrentSelectorV1,
    successor_selector: DraftEditorCurrentSelectorV1,
    before_head: DraftEditorCandidateSessionV1,
    after_head: DraftEditorCandidateSessionV1,
    captured_frontier: DraftEditHistoryFrontierV1,
}

impl DraftEditorCandidatePublicationReceiptV1 {
    pub(crate) fn new(
        request_bytes: Vec<u8>,
        prior_selector: DraftEditorCurrentSelectorV1,
        successor_selector: DraftEditorCurrentSelectorV1,
        before_head: DraftEditorCandidateSessionV1,
        after_head: DraftEditorCandidateSessionV1,
        captured_frontier: DraftEditHistoryFrontierV1,
    ) -> Self {
        Self {
            request_bytes,
            prior_selector,
            successor_selector,
            before_head,
            after_head,
            captured_frontier,
        }
    }
    pub fn request_bytes(&self) -> &[u8] {
        &self.request_bytes
    }
    pub const fn prior_selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.prior_selector
    }
    pub const fn successor_selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.successor_selector
    }
    pub const fn before_head(&self) -> &DraftEditorCandidateSessionV1 {
        &self.before_head
    }
    pub const fn after_head(&self) -> &DraftEditorCandidateSessionV1 {
        &self.after_head
    }
    pub const fn captured_frontier(&self) -> &DraftEditHistoryFrontierV1 {
        &self.captured_frontier
    }
    pub const fn published_pair(&self) -> DraftRootHistoryPairV1 {
        DraftRootHistoryPairV1::new(
            self.successor_selector.root(),
            self.successor_selector.history(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidateSessionDisposeReceiptV1 {
    request_bytes: Vec<u8>,
    before_head: DraftEditorCandidateSessionV1,
    after_head: DraftEditorCandidateSessionV1,
    frontier: DraftEditHistoryFrontierV1,
}

impl DraftEditorCandidateSessionDisposeReceiptV1 {
    pub(crate) fn new(
        request_bytes: Vec<u8>,
        before_head: DraftEditorCandidateSessionV1,
        after_head: DraftEditorCandidateSessionV1,
        frontier: DraftEditHistoryFrontierV1,
    ) -> Self {
        Self {
            request_bytes,
            before_head,
            after_head,
            frontier,
        }
    }
    pub fn request_bytes(&self) -> &[u8] {
        &self.request_bytes
    }
    pub const fn before_head(&self) -> &DraftEditorCandidateSessionV1 {
        &self.before_head
    }
    pub const fn after_head(&self) -> &DraftEditorCandidateSessionV1 {
        &self.after_head
    }
    pub const fn frontier(&self) -> &DraftEditHistoryFrontierV1 {
        &self.frontier
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftEditorCandidateSessionRecordV1 {
    Head(DraftEditorCandidateSessionV1),
    OpenReceipt(DraftEditorCandidateSessionOpenReceiptV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidatePublicationCollisionProofV1 {
    requested: DraftEditorCandidatePublicationRequestV1,
    occupied: DraftEditorCandidatePublicationReceiptV1,
}

impl DraftEditorCandidatePublicationCollisionProofV1 {
    pub(crate) const fn new(
        requested: DraftEditorCandidatePublicationRequestV1,
        occupied: DraftEditorCandidatePublicationReceiptV1,
    ) -> Self {
        Self {
            requested,
            occupied,
        }
    }
    pub const fn requested(&self) -> DraftEditorCandidatePublicationRequestV1 {
        self.requested
    }
    pub const fn occupied(&self) -> &DraftEditorCandidatePublicationReceiptV1 {
        &self.occupied
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftEditorCandidatePublicationOutcomeV1 {
    Published(DraftEditorCurrentSelectorV1, DraftRootHistoryPairV1),
    ExactReplay(DraftEditorCandidatePublicationReceiptV1),
    Superseded(u64, DraftRootHistoryPairV1),
    DurableBaseConflict(DraftEditorCurrentSelectorV1),
    SessionDisposed,
    OccupiedIdentityCollision(DraftEditorCandidatePublicationCollisionProofV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidateSessionDisposeCollisionProofV1 {
    requested: DraftEditorCandidateSessionDisposeRequestV1,
    occupied: DraftEditorCandidateSessionDisposeReceiptV1,
}

impl DraftEditorCandidateSessionDisposeCollisionProofV1 {
    pub(crate) const fn new(
        requested: DraftEditorCandidateSessionDisposeRequestV1,
        occupied: DraftEditorCandidateSessionDisposeReceiptV1,
    ) -> Self {
        Self {
            requested,
            occupied,
        }
    }
    pub const fn requested(&self) -> DraftEditorCandidateSessionDisposeRequestV1 {
        self.requested
    }
    pub const fn occupied(&self) -> &DraftEditorCandidateSessionDisposeReceiptV1 {
        &self.occupied
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftEditorCandidateSessionDisposeOutcomeV1 {
    Disposed(DraftEditorCandidateSessionV1),
    ExactReplay(DraftEditorCandidateSessionDisposeReceiptV1),
    DirtyConflict(DraftEditorCandidateSessionV1),
    AlreadyDisposed(DraftEditorCandidateSessionV1),
    OccupiedIdentityCollision(DraftEditorCandidateSessionDisposeCollisionProofV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidateSessionCollisionProofV1 {
    requested: DraftEditorCandidateSessionOpenRequestV1,
    occupied: DraftEditorCandidateSessionOpenReceiptV1,
}

impl DraftEditorCandidateSessionCollisionProofV1 {
    pub const fn new(
        requested: DraftEditorCandidateSessionOpenRequestV1,
        occupied: DraftEditorCandidateSessionOpenReceiptV1,
    ) -> Self {
        Self {
            requested,
            occupied,
        }
    }
    pub const fn requested(&self) -> DraftEditorCandidateSessionOpenRequestV1 {
        self.requested
    }
    pub const fn occupied(&self) -> &DraftEditorCandidateSessionOpenReceiptV1 {
        &self.occupied
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftEditorCandidateSessionOpenOutcomeV1 {
    Opened(DraftEditorCandidateSessionV1),
    ExactReplay(DraftEditorCandidateSessionV1),
    StaleDisposed(DraftEditorCandidateSessionV1),
    SelectorConflict(DraftEditorCurrentSelectorV1),
    OccupiedIdentityCollision(DraftEditorCandidateSessionCollisionProofV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftEditorCandidateSessionReadOutcomeV1 {
    Active(DraftEditorCandidateSessionV1),
    Disposed(DraftEditorCandidateSessionV1),
    Absent,
    ConcurrentChange,
    InvariantFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditorCandidateActivationBindingV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    session_generation: u64,
    candidate_generation: u64,
    root: DraftPieceRootReferenceV1,
    history: DraftEditHistoryFrontierReferenceV1,
    logical_extent: DraftLogicalExtentV1,
}

impl DraftEditorCandidateActivationBindingV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        session_generation: u64,
        candidate_generation: u64,
        root: DraftPieceRootReferenceV1,
        history: DraftEditHistoryFrontierReferenceV1,
        logical_extent: DraftLogicalExtentV1,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            session_generation,
            candidate_generation,
            root,
            history,
            logical_extent,
        }
    }

    pub const fn from_head(head: &DraftEditorCandidateSessionV1) -> Self {
        Self::new(
            head.draft_id(),
            head.session_id(),
            head.session_generation(),
            head.newest_candidate_generation(),
            head.newest_root(),
            head.newest_history(),
            head.logical_extent(),
        )
    }
    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
    pub const fn session_generation(self) -> u64 {
        self.session_generation
    }
    pub const fn candidate_generation(self) -> u64 {
        self.candidate_generation
    }
    pub const fn root(self) -> DraftPieceRootReferenceV1 {
        self.root
    }
    pub const fn history(self) -> DraftEditHistoryFrontierReferenceV1 {
        self.history
    }
    pub const fn logical_extent(self) -> DraftLogicalExtentV1 {
        self.logical_extent
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPieceRecordIdV1([u8; 16]);

impl DraftPieceRecordIdV1 {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPieceDigestV1([u8; 32]);

impl DraftPieceDigestV1 {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DraftPieceRootBuildIdentityV1 {
    DirectCanonicalEmpty {
        operation_id: DraftPieceOperationIdV1,
    },
    EditorCandidate {
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    },
}

impl DraftPieceRootBuildIdentityV1 {
    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        match self {
            Self::DirectCanonicalEmpty { operation_id }
            | Self::EditorCandidate { operation_id, .. } => operation_id,
        }
    }

    pub const fn session_id(self) -> Option<DraftEditorCandidateSessionIdV1> {
        match self {
            Self::DirectCanonicalEmpty { .. } => None,
            Self::EditorCandidate { session_id, .. } => Some(session_id),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPieceRootKeyV1 {
    draft_id: SyndicDraftId,
    build_identity: DraftPieceRootBuildIdentityV1,
}

impl DraftPieceRootKeyV1 {
    pub const fn direct_canonical_empty(
        draft_id: SyndicDraftId,
        operation_id: DraftPieceOperationIdV1,
    ) -> Self {
        Self {
            draft_id,
            build_identity: DraftPieceRootBuildIdentityV1::DirectCanonicalEmpty { operation_id },
        }
    }

    pub const fn editor_candidate(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    ) -> Self {
        Self {
            draft_id,
            build_identity: DraftPieceRootBuildIdentityV1::EditorCandidate {
                session_id,
                operation_id,
            },
        }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }

    pub const fn build_identity(self) -> DraftPieceRootBuildIdentityV1 {
        self.build_identity
    }

    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        self.build_identity.operation_id()
    }

    pub const fn session_id(self) -> Option<DraftEditorCandidateSessionIdV1> {
        self.build_identity.session_id()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPieceRecordKeyV1 {
    draft_id: SyndicDraftId,
    id: DraftPieceRecordIdV1,
}

impl DraftPieceRecordKeyV1 {
    pub const fn new(draft_id: SyndicDraftId, id: DraftPieceRecordIdV1) -> Self {
        Self { draft_id, id }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }

    pub const fn id(self) -> DraftPieceRecordIdV1 {
        self.id
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPieceSettlementKeyV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
}

impl DraftPieceSettlementKeyV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            operation_id,
        }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }

    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }

    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPieceBuildProgressReceiptKeyV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
    transition_ordinal: u64,
}

impl DraftPieceBuildProgressReceiptKeyV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
        transition_ordinal: u64,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            operation_id,
            transition_ordinal,
        }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }

    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }

    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }

    pub const fn transition_ordinal(self) -> u64 {
        self.transition_ordinal
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftLogicalExtentV1 {
    logical_utf8_bytes: u64,
    logical_line_count: u64,
}

impl DraftLogicalExtentV1 {
    pub const fn new(logical_utf8_bytes: u64, logical_line_count: u64) -> Self {
        Self {
            logical_utf8_bytes,
            logical_line_count,
        }
    }

    pub const fn logical_utf8_bytes(self) -> u64 {
        self.logical_utf8_bytes
    }

    pub const fn logical_line_count(self) -> u64 {
        self.logical_line_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceTextSummaryV1 {
    logical_utf8_bytes: u64,
    newline_count: u64,
    logical_line_count: u64,
}

impl DraftPieceTextSummaryV1 {
    pub const fn new(logical_utf8_bytes: u64, newline_count: u64, logical_line_count: u64) -> Self {
        Self {
            logical_utf8_bytes,
            newline_count,
            logical_line_count,
        }
    }

    pub const fn empty() -> Self {
        Self::new(0, 0, 0)
    }

    pub fn from_utf8(text: &str) -> Self {
        let logical_utf8_bytes = text.len() as u64;
        if logical_utf8_bytes == 0 {
            return Self::empty();
        }
        let newline_count = text.bytes().filter(|byte| *byte == b'\n').count() as u64;
        Self::new(logical_utf8_bytes, newline_count, newline_count + 1)
    }

    pub const fn logical_utf8_bytes(self) -> u64 {
        self.logical_utf8_bytes
    }

    pub const fn newline_count(self) -> u64 {
        self.newline_count
    }

    pub const fn logical_line_count(self) -> u64 {
        self.logical_line_count
    }

    pub const fn logical_extent(self) -> DraftLogicalExtentV1 {
        DraftLogicalExtentV1::new(self.logical_utf8_bytes, self.logical_line_count)
    }

    pub const fn is_canonical(self) -> bool {
        if self.logical_utf8_bytes == 0 {
            self.newline_count == 0 && self.logical_line_count == 0
        } else {
            match self.newline_count.checked_add(1) {
                Some(lines) => self.logical_line_count == lines,
                None => false,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceSummaryV1 {
    logical_utf8_bytes: u64,
    newline_count: u64,
    logical_line_count: u64,
    piece_count: u64,
    marker_count: u64,
    marker_digest: DraftPieceDigestV1,
    height: u8,
    root_digest: DraftPieceDigestV1,
}

impl DraftPieceSummaryV1 {
    pub const fn new(
        logical_utf8_bytes: u64,
        newline_count: u64,
        logical_line_count: u64,
        piece_count: u64,
        marker_count: u64,
        marker_digest: DraftPieceDigestV1,
        height: u8,
        root_digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            logical_utf8_bytes,
            newline_count,
            logical_line_count,
            piece_count,
            marker_count,
            marker_digest,
            height,
            root_digest,
        }
    }

    pub const fn logical_utf8_bytes(self) -> u64 {
        self.logical_utf8_bytes
    }

    pub const fn newline_count(self) -> u64 {
        self.newline_count
    }

    pub const fn logical_line_count(self) -> u64 {
        self.logical_line_count
    }

    pub const fn logical_extent(self) -> DraftLogicalExtentV1 {
        DraftLogicalExtentV1::new(self.logical_utf8_bytes, self.logical_line_count)
    }

    pub const fn text_summary(self) -> DraftPieceTextSummaryV1 {
        DraftPieceTextSummaryV1::new(
            self.logical_utf8_bytes,
            self.newline_count,
            self.logical_line_count,
        )
    }

    pub const fn piece_count(self) -> u64 {
        self.piece_count
    }

    pub const fn marker_count(self) -> u64 {
        self.marker_count
    }

    pub const fn marker_digest(self) -> DraftPieceDigestV1 {
        self.marker_digest
    }

    pub const fn height(self) -> u8 {
        self.height
    }

    pub const fn root_digest(self) -> DraftPieceDigestV1 {
        self.root_digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceRootReferenceV1 {
    key: DraftPieceRootKeyV1,
    root_node: Option<DraftPieceRecordIdV1>,
    summary: DraftPieceSummaryV1,
    marker_index_root: Option<DraftPieceRecordIdV1>,
    marker_index_summary: DraftMarkerIdentityIndexSummaryV1,
    combined_digest: DraftPieceDigestV1,
}

impl DraftPieceRootReferenceV1 {
    pub const fn new(
        key: DraftPieceRootKeyV1,
        root_node: Option<DraftPieceRecordIdV1>,
        summary: DraftPieceSummaryV1,
        marker_index_root: Option<DraftPieceRecordIdV1>,
        marker_index_summary: DraftMarkerIdentityIndexSummaryV1,
        combined_digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            key,
            root_node,
            summary,
            marker_index_root,
            marker_index_summary,
            combined_digest,
        }
    }

    pub const fn key(self) -> DraftPieceRootKeyV1 {
        self.key
    }

    pub const fn root_node(self) -> Option<DraftPieceRecordIdV1> {
        self.root_node
    }

    pub const fn summary(self) -> DraftPieceSummaryV1 {
        self.summary
    }

    pub const fn marker_index_root(self) -> Option<DraftPieceRecordIdV1> {
        self.marker_index_root
    }

    pub const fn marker_index_summary(self) -> DraftMarkerIdentityIndexSummaryV1 {
        self.marker_index_summary
    }

    pub const fn combined_digest(self) -> DraftPieceDigestV1 {
        self.combined_digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerIdentityIndexSummaryV1 {
    record_count: u64,
    height: u8,
    root_digest: DraftPieceDigestV1,
}

impl DraftMarkerIdentityIndexSummaryV1 {
    pub const fn new(record_count: u64, height: u8, root_digest: DraftPieceDigestV1) -> Self {
        Self {
            record_count,
            height,
            root_digest,
        }
    }

    pub const fn record_count(self) -> u64 {
        self.record_count
    }
    pub const fn height(self) -> u8 {
        self.height
    }
    pub const fn root_digest(self) -> DraftPieceDigestV1 {
        self.root_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceRootRecordV1 {
    reference: DraftPieceRootReferenceV1,
}

impl DraftPieceRootRecordV1 {
    pub const fn new(reference: DraftPieceRootReferenceV1) -> Self {
        Self { reference }
    }

    pub const fn reference(&self) -> DraftPieceRootReferenceV1 {
        self.reference
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DraftMarkerIdentityRecordKindV1 {
    Internal,
    Leaf,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMarkerIdentityRecordKeyV1 {
    draft_id: SyndicDraftId,
    kind: DraftMarkerIdentityRecordKindV1,
    id: DraftPieceRecordIdV1,
}

impl DraftMarkerIdentityRecordKeyV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        kind: DraftMarkerIdentityRecordKindV1,
        id: DraftPieceRecordIdV1,
    ) -> Self {
        Self { draft_id, kind, id }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn kind(self) -> DraftMarkerIdentityRecordKindV1 {
        self.kind
    }
    pub const fn id(self) -> DraftPieceRecordIdV1 {
        self.id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerIdentityOccurrenceV1 {
    marker_id: SyndicDraftMarkerId,
    label: ImageLabelOrdinal,
    order_key: u64,
    sequence_leaf_id: DraftPieceRecordIdV1,
    sequence_leaf_digest: DraftPieceDigestV1,
}

impl DraftMarkerIdentityOccurrenceV1 {
    pub const fn new(
        marker_id: SyndicDraftMarkerId,
        label: ImageLabelOrdinal,
        order_key: u64,
        sequence_leaf_id: DraftPieceRecordIdV1,
        sequence_leaf_digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            marker_id,
            label,
            order_key,
            sequence_leaf_id,
            sequence_leaf_digest,
        }
    }

    pub const fn marker_id(self) -> SyndicDraftMarkerId {
        self.marker_id
    }
    pub const fn label(self) -> ImageLabelOrdinal {
        self.label
    }
    pub const fn order_key(self) -> u64 {
        self.order_key
    }
    pub const fn sequence_leaf_id(self) -> DraftPieceRecordIdV1 {
        self.sequence_leaf_id
    }
    pub const fn sequence_leaf_digest(self) -> DraftPieceDigestV1 {
        self.sequence_leaf_digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMarkerIdentityChildV1 {
    id: DraftPieceRecordIdV1,
    digest: DraftPieceDigestV1,
    record_count: u64,
    first: SyndicDraftMarkerId,
    last: SyndicDraftMarkerId,
}

impl DraftMarkerIdentityChildV1 {
    pub const fn new(
        id: DraftPieceRecordIdV1,
        digest: DraftPieceDigestV1,
        record_count: u64,
        first: SyndicDraftMarkerId,
        last: SyndicDraftMarkerId,
    ) -> Self {
        Self {
            id,
            digest,
            record_count,
            first,
            last,
        }
    }

    pub const fn id(self) -> DraftPieceRecordIdV1 {
        self.id
    }
    pub const fn digest(self) -> DraftPieceDigestV1 {
        self.digest
    }
    pub const fn record_count(self) -> u64 {
        self.record_count
    }
    pub const fn first(self) -> SyndicDraftMarkerId {
        self.first
    }
    pub const fn last(self) -> SyndicDraftMarkerId {
        self.last
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftMarkerIdentityRecordV1 {
    Internal {
        key: DraftMarkerIdentityRecordKeyV1,
        height: u8,
        children: Vec<DraftMarkerIdentityChildV1>,
        digest: DraftPieceDigestV1,
    },
    Leaf {
        key: DraftMarkerIdentityRecordKeyV1,
        occurrence: DraftMarkerIdentityOccurrenceV1,
        digest: DraftPieceDigestV1,
    },
}

impl DraftMarkerIdentityRecordV1 {
    pub const fn key(&self) -> DraftMarkerIdentityRecordKeyV1 {
        match self {
            Self::Internal { key, .. } | Self::Leaf { key, .. } => *key,
        }
    }

    pub const fn digest(&self) -> DraftPieceDigestV1 {
        match self {
            Self::Internal { digest, .. } | Self::Leaf { digest, .. } => *digest,
        }
    }

    pub const fn height(&self) -> u8 {
        match self {
            Self::Internal { height, .. } => *height,
            Self::Leaf { .. } => 0,
        }
    }

    pub fn children(&self) -> Option<&[DraftMarkerIdentityChildV1]> {
        match self {
            Self::Internal { children, .. } => Some(children),
            Self::Leaf { .. } => None,
        }
    }

    pub const fn occurrence(&self) -> Option<DraftMarkerIdentityOccurrenceV1> {
        match self {
            Self::Leaf { occurrence, .. } => Some(*occurrence),
            Self::Internal { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DraftCompositeSearchKeyV1 {
    BeforeMarkers(u64),
    Marker {
        anchor: u64,
        order_key: u64,
        marker_id: SyndicDraftMarkerId,
    },
    AfterMarkers(u64),
}

impl Ord for DraftCompositeSearchKeyV1 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.anchor()
            .cmp(&other.anchor())
            .then_with(|| match (self, other) {
                (Self::BeforeMarkers(_), Self::BeforeMarkers(_))
                | (Self::AfterMarkers(_), Self::AfterMarkers(_)) => std::cmp::Ordering::Equal,
                (Self::BeforeMarkers(_), _) | (Self::Marker { .. }, Self::AfterMarkers(_)) => {
                    std::cmp::Ordering::Less
                }
                (Self::AfterMarkers(_), _) | (Self::Marker { .. }, Self::BeforeMarkers(_)) => {
                    std::cmp::Ordering::Greater
                }
                (
                    Self::Marker {
                        order_key: left_order,
                        marker_id: left_id,
                        ..
                    },
                    Self::Marker {
                        order_key: right_order,
                        marker_id: right_id,
                        ..
                    },
                ) => left_order
                    .cmp(right_order)
                    .then_with(|| left_id.cmp(right_id)),
            })
    }
}

impl PartialOrd for DraftCompositeSearchKeyV1 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl DraftCompositeSearchKeyV1 {
    pub const fn anchor(self) -> u64 {
        match self {
            Self::BeforeMarkers(anchor)
            | Self::Marker { anchor, .. }
            | Self::AfterMarkers(anchor) => anchor,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftCompositeGapWitnessV1 {
    Unambiguous,
    BeforeAll,
    Between {
        left_order_key: u64,
        left_marker_id: SyndicDraftMarkerId,
        right_order_key: u64,
        right_marker_id: SyndicDraftMarkerId,
    },
    AfterAll,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftCompositePositionV1 {
    utf8_offset: u64,
    gap: DraftCompositeGapWitnessV1,
}

impl DraftCompositePositionV1 {
    pub const fn new(utf8_offset: u64, gap: DraftCompositeGapWitnessV1) -> Self {
        Self { utf8_offset, gap }
    }

    pub const fn utf8_offset(self) -> u64 {
        self.utf8_offset
    }

    pub const fn gap(self) -> DraftCompositeGapWitnessV1 {
        self.gap
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceMarkerV1 {
    marker_id: SyndicDraftMarkerId,
    order_key: u64,
    label: ImageLabelOrdinal,
}

impl DraftPieceMarkerV1 {
    pub const fn new(
        marker_id: SyndicDraftMarkerId,
        order_key: u64,
        label: ImageLabelOrdinal,
    ) -> Self {
        Self {
            marker_id,
            order_key,
            label,
        }
    }

    pub const fn marker_id(self) -> SyndicDraftMarkerId {
        self.marker_id
    }

    pub const fn order_key(self) -> u64 {
        self.order_key
    }

    pub const fn label(self) -> ImageLabelOrdinal {
        self.label
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPieceV1 {
    Text(String),
    Marker(DraftPieceMarkerV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceReplacementV1 {
    start: DraftCompositePositionV1,
    end: DraftCompositePositionV1,
    inserted: Vec<DraftPieceV1>,
    marker_effect: Option<super::DraftPieceMarkerEffectV1>,
    continuation: bool,
}

impl DraftPieceReplacementV1 {
    pub const fn new(
        start: DraftCompositePositionV1,
        end: DraftCompositePositionV1,
        inserted: Vec<DraftPieceV1>,
    ) -> Self {
        Self {
            start,
            end,
            inserted,
            marker_effect: None,
            continuation: false,
        }
    }

    pub fn continuation(
        start: DraftCompositePositionV1,
        end: DraftCompositePositionV1,
        inserted: Vec<DraftPieceV1>,
    ) -> Self {
        Self {
            start,
            end,
            inserted,
            marker_effect: None,
            continuation: true,
        }
    }

    pub const fn start(&self) -> DraftCompositePositionV1 {
        self.start
    }

    pub const fn end(&self) -> DraftCompositePositionV1 {
        self.end
    }

    pub fn inserted(&self) -> &[DraftPieceV1] {
        &self.inserted
    }

    pub fn with_marker_effect(mut self, marker_effect: super::DraftPieceMarkerEffectV1) -> Self {
        self.marker_effect = Some(marker_effect);
        self
    }

    pub const fn marker_effect(&self) -> Option<super::DraftPieceMarkerEffectV1> {
        self.marker_effect
    }

    pub const fn is_continuation(&self) -> bool {
        self.continuation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPieceLeafValueV1 {
    Text(String),
    Marker(DraftPieceMarkerV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceLeafRecordV1 {
    key: DraftPieceRecordKeyV1,
    value: DraftPieceLeafValueV1,
    text_summary: DraftPieceTextSummaryV1,
    digest: DraftPieceDigestV1,
}

impl DraftPieceLeafRecordV1 {
    pub const fn new(
        key: DraftPieceRecordKeyV1,
        value: DraftPieceLeafValueV1,
        text_summary: DraftPieceTextSummaryV1,
        digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            key,
            value,
            text_summary,
            digest,
        }
    }

    pub const fn key(&self) -> DraftPieceRecordKeyV1 {
        self.key
    }

    pub const fn value(&self) -> &DraftPieceLeafValueV1 {
        &self.value
    }

    pub const fn text_summary(&self) -> DraftPieceTextSummaryV1 {
        self.text_summary
    }

    pub const fn digest(&self) -> DraftPieceDigestV1 {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceChildV1 {
    id: DraftPieceRecordIdV1,
    digest: DraftPieceDigestV1,
    logical_utf8_bytes: u64,
    newline_count: u64,
    logical_line_count: u64,
    piece_count: u64,
    marker_count: u64,
    marker_digest: DraftPieceDigestV1,
    first: DraftCompositeSearchKeyV1,
    last: DraftCompositeSearchKeyV1,
}

impl DraftPieceChildV1 {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        id: DraftPieceRecordIdV1,
        digest: DraftPieceDigestV1,
        logical_utf8_bytes: u64,
        newline_count: u64,
        logical_line_count: u64,
        piece_count: u64,
        marker_count: u64,
        marker_digest: DraftPieceDigestV1,
        first: DraftCompositeSearchKeyV1,
        last: DraftCompositeSearchKeyV1,
    ) -> Self {
        Self {
            id,
            digest,
            logical_utf8_bytes,
            newline_count,
            logical_line_count,
            piece_count,
            marker_count,
            marker_digest,
            first,
            last,
        }
    }

    pub const fn id(self) -> DraftPieceRecordIdV1 {
        self.id
    }
    pub const fn digest(self) -> DraftPieceDigestV1 {
        self.digest
    }
    pub const fn logical_utf8_bytes(self) -> u64 {
        self.logical_utf8_bytes
    }
    pub const fn newline_count(self) -> u64 {
        self.newline_count
    }
    pub const fn logical_line_count(self) -> u64 {
        self.logical_line_count
    }
    pub const fn text_summary(self) -> DraftPieceTextSummaryV1 {
        DraftPieceTextSummaryV1::new(
            self.logical_utf8_bytes,
            self.newline_count,
            self.logical_line_count,
        )
    }
    pub const fn piece_count(self) -> u64 {
        self.piece_count
    }
    pub const fn marker_count(self) -> u64 {
        self.marker_count
    }
    pub const fn marker_digest(self) -> DraftPieceDigestV1 {
        self.marker_digest
    }
    pub const fn first(self) -> DraftCompositeSearchKeyV1 {
        self.first
    }
    pub const fn last(self) -> DraftCompositeSearchKeyV1 {
        self.last
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceNodeRecordV1 {
    key: DraftPieceRecordKeyV1,
    height: u8,
    children: Vec<DraftPieceChildV1>,
    digest: DraftPieceDigestV1,
}

impl DraftPieceNodeRecordV1 {
    pub fn new(
        key: DraftPieceRecordKeyV1,
        height: u8,
        children: Vec<DraftPieceChildV1>,
        digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            key,
            height,
            children,
            digest,
        }
    }

    pub const fn key(&self) -> DraftPieceRecordKeyV1 {
        self.key
    }
    pub const fn height(&self) -> u8 {
        self.height
    }
    pub fn children(&self) -> &[DraftPieceChildV1] {
        &self.children
    }
    pub const fn digest(&self) -> DraftPieceDigestV1 {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceBuildLifecycleV1 {
    Open,
    Complete,
    Committed,
    Rejected,
    Conflict,
    Cancelled,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceEditHeaderV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    predecessor_candidate_generation: u64,
    predecessor_root: DraftPieceRootReferenceV1,
    predecessor_history: DraftEditHistoryFrontierReferenceV1,
    operation_id: DraftPieceOperationIdV1,
    predecessor_caret: DraftCompositePositionV1,
    predecessor_selection: DraftCompositePositionV1,
    caret: DraftCompositePositionV1,
    selection: DraftCompositePositionV1,
    fragment_count: u64,
    fragment_chain: DraftPieceDigestV1,
}

impl DraftPieceEditHeaderV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        operation_id: DraftPieceOperationIdV1,
        predecessor_caret: DraftCompositePositionV1,
        predecessor_selection: DraftCompositePositionV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
        fragment_count: u64,
        fragment_chain: DraftPieceDigestV1,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            predecessor_candidate_generation,
            predecessor_root,
            predecessor_history,
            operation_id,
            predecessor_caret,
            predecessor_selection,
            caret,
            selection,
            fragment_count,
            fragment_chain,
        }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
    pub const fn predecessor_candidate_generation(self) -> u64 {
        self.predecessor_candidate_generation
    }
    pub const fn predecessor_root(self) -> DraftPieceRootReferenceV1 {
        self.predecessor_root
    }
    pub const fn predecessor_history(self) -> DraftEditHistoryFrontierReferenceV1 {
        self.predecessor_history
    }
    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }
    pub const fn predecessor_caret(self) -> DraftCompositePositionV1 {
        self.predecessor_caret
    }
    pub const fn predecessor_selection(self) -> DraftCompositePositionV1 {
        self.predecessor_selection
    }
    pub const fn caret(self) -> DraftCompositePositionV1 {
        self.caret
    }
    pub const fn selection(self) -> DraftCompositePositionV1 {
        self.selection
    }
    pub const fn fragment_count(self) -> u64 {
        self.fragment_count
    }
    pub const fn fragment_chain(self) -> DraftPieceDigestV1 {
        self.fragment_chain
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceBuildRootsV1 {
    sequence_root: Option<DraftPieceRecordIdV1>,
    sequence_summary: DraftPieceSummaryV1,
    marker_index_root: Option<DraftPieceRecordIdV1>,
    marker_index_summary: DraftMarkerIdentityIndexSummaryV1,
}

impl DraftPieceBuildRootsV1 {
    pub const fn new(
        sequence_root: Option<DraftPieceRecordIdV1>,
        sequence_summary: DraftPieceSummaryV1,
        marker_index_root: Option<DraftPieceRecordIdV1>,
        marker_index_summary: DraftMarkerIdentityIndexSummaryV1,
    ) -> Self {
        Self {
            sequence_root,
            sequence_summary,
            marker_index_root,
            marker_index_summary,
        }
    }

    pub const fn from_root(root: DraftPieceRootReferenceV1) -> Self {
        Self::new(
            root.root_node(),
            root.summary(),
            root.marker_index_root(),
            root.marker_index_summary(),
        )
    }

    pub const fn sequence_root(self) -> Option<DraftPieceRecordIdV1> {
        self.sequence_root
    }
    pub const fn sequence_summary(self) -> DraftPieceSummaryV1 {
        self.sequence_summary
    }
    pub const fn marker_index_root(self) -> Option<DraftPieceRecordIdV1> {
        self.marker_index_root
    }
    pub const fn marker_index_summary(self) -> DraftMarkerIdentityIndexSummaryV1 {
        self.marker_index_summary
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceBuildBoundaryV1 {
    rank: u64,
    inner: u64,
}

impl DraftPieceBuildBoundaryV1 {
    pub const fn new(rank: u64, inner: u64) -> Self {
        Self { rank, inner }
    }

    pub const fn rank(self) -> u64 {
        self.rank
    }

    pub const fn inner(self) -> u64 {
        self.inner
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceBuildFrontierV1 {
    Receiving {
        next_ordinal: u64,
        chain: DraftPieceDigestV1,
    },
    ReconcilingMoves {
        fragment_ordinal: u64,
        next_move: u64,
    },
    Planning {
        fragment_ordinal: u64,
    },
    Removing {
        fragment_ordinal: u64,
        next_rank: u64,
        end_rank: u64,
        base_end: DraftPieceBuildBoundaryV1,
        successor_start: DraftPieceBuildBoundaryV1,
        successor_end: DraftPieceBuildBoundaryV1,
    },
    Applying {
        fragment_ordinal: u64,
        base_end: DraftPieceBuildBoundaryV1,
        successor_start: DraftPieceBuildBoundaryV1,
        successor_end: DraftPieceBuildBoundaryV1,
    },
    Inserting {
        fragment_ordinal: u64,
        next_piece: u64,
        next_byte: u64,
        base_end: DraftPieceBuildBoundaryV1,
        successor_end: DraftPieceBuildBoundaryV1,
    },
    CrossValidating,
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPieceBuildProgressReceiptReferenceV1 {
    key: DraftPieceBuildProgressReceiptKeyV1,
    digest: DraftPieceDigestV1,
}

impl DraftPieceBuildProgressReceiptReferenceV1 {
    pub const fn new(key: DraftPieceBuildProgressReceiptKeyV1, digest: DraftPieceDigestV1) -> Self {
        Self { key, digest }
    }

    pub const fn key(self) -> DraftPieceBuildProgressReceiptKeyV1 {
        self.key
    }

    pub const fn digest(self) -> DraftPieceDigestV1 {
        self.digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceBuildRecordV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    predecessor_candidate_generation: u64,
    predecessor_root: DraftPieceRootReferenceV1,
    predecessor_history: DraftEditHistoryFrontierReferenceV1,
    operation_id: DraftPieceOperationIdV1,
    predecessor_caret: DraftCompositePositionV1,
    predecessor_selection: DraftCompositePositionV1,
    caret: DraftCompositePositionV1,
    selection: DraftCompositePositionV1,
    fragment_count: u64,
    fragment_chain: DraftPieceDigestV1,
    canonical_header: Vec<u8>,
    staged_fragment_count: u64,
    staged_fragment_chain: DraftPieceDigestV1,
    proposal_digest: DraftPieceDigestV1,
    working_roots: DraftPieceBuildRootsV1,
    base_frontier: DraftPieceBuildBoundaryV1,
    successor_frontier: DraftPieceBuildBoundaryV1,
    next_record_ordinal: u64,
    frontier: DraftPieceBuildFrontierV1,
    progress_digest: DraftPieceDigestV1,
    progress_receipt: DraftPieceBuildProgressReceiptReferenceV1,
    durable_continuation: Option<DraftPieceDurableBuildContinuationV1>,
    successor: Option<DraftPieceRootReferenceV1>,
    build_digest: Option<DraftPieceDigestV1>,
    lifecycle: DraftPieceBuildLifecycleV1,
}

impl DraftPieceBuildRecordV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        operation_id: DraftPieceOperationIdV1,
        predecessor_caret: DraftCompositePositionV1,
        predecessor_selection: DraftCompositePositionV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
        fragment_count: u64,
        fragment_chain: DraftPieceDigestV1,
        canonical_header: Vec<u8>,
        staged_fragment_count: u64,
        staged_fragment_chain: DraftPieceDigestV1,
        proposal_digest: DraftPieceDigestV1,
        working_roots: DraftPieceBuildRootsV1,
        base_frontier: DraftPieceBuildBoundaryV1,
        successor_frontier: DraftPieceBuildBoundaryV1,
        next_record_ordinal: u64,
        frontier: DraftPieceBuildFrontierV1,
        progress_digest: DraftPieceDigestV1,
        progress_receipt: DraftPieceBuildProgressReceiptReferenceV1,
        successor: Option<DraftPieceRootReferenceV1>,
        build_digest: Option<DraftPieceDigestV1>,
        lifecycle: DraftPieceBuildLifecycleV1,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            predecessor_candidate_generation,
            predecessor_root,
            predecessor_history,
            operation_id,
            predecessor_caret,
            predecessor_selection,
            caret,
            selection,
            fragment_count,
            fragment_chain,
            canonical_header,
            staged_fragment_count,
            staged_fragment_chain,
            proposal_digest,
            working_roots,
            base_frontier,
            successor_frontier,
            next_record_ordinal,
            frontier,
            progress_digest,
            progress_receipt,
            durable_continuation: None,
            successor,
            build_digest,
            lifecycle,
        }
    }

    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn session_id(&self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
    pub const fn predecessor_candidate_generation(&self) -> u64 {
        self.predecessor_candidate_generation
    }
    pub const fn predecessor_root(&self) -> DraftPieceRootReferenceV1 {
        self.predecessor_root
    }
    pub const fn predecessor_history(&self) -> DraftEditHistoryFrontierReferenceV1 {
        self.predecessor_history
    }
    pub const fn operation_id(&self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }
    pub const fn predecessor_caret(&self) -> DraftCompositePositionV1 {
        self.predecessor_caret
    }
    pub const fn predecessor_selection(&self) -> DraftCompositePositionV1 {
        self.predecessor_selection
    }
    pub const fn caret(&self) -> DraftCompositePositionV1 {
        self.caret
    }
    pub const fn selection(&self) -> DraftCompositePositionV1 {
        self.selection
    }
    pub const fn fragment_count(&self) -> u64 {
        self.fragment_count
    }
    pub const fn fragment_chain(&self) -> DraftPieceDigestV1 {
        self.fragment_chain
    }
    pub fn canonical_header(&self) -> &[u8] {
        &self.canonical_header
    }
    pub const fn staged_fragment_count(&self) -> u64 {
        self.staged_fragment_count
    }
    pub const fn staged_fragment_chain(&self) -> DraftPieceDigestV1 {
        self.staged_fragment_chain
    }
    pub const fn proposal_digest(&self) -> DraftPieceDigestV1 {
        self.proposal_digest
    }
    pub const fn working_roots(&self) -> DraftPieceBuildRootsV1 {
        self.working_roots
    }
    pub const fn base_frontier(&self) -> DraftPieceBuildBoundaryV1 {
        self.base_frontier
    }
    pub const fn successor_frontier(&self) -> DraftPieceBuildBoundaryV1 {
        self.successor_frontier
    }
    pub const fn next_record_ordinal(&self) -> u64 {
        self.next_record_ordinal
    }
    pub const fn frontier(&self) -> DraftPieceBuildFrontierV1 {
        self.frontier
    }
    pub const fn progress_digest(&self) -> DraftPieceDigestV1 {
        self.progress_digest
    }
    pub(crate) fn with_progress_digest(mut self, progress_digest: DraftPieceDigestV1) -> Self {
        self.progress_digest = progress_digest;
        self
    }
    pub const fn progress_receipt(&self) -> DraftPieceBuildProgressReceiptReferenceV1 {
        self.progress_receipt
    }
    pub(crate) fn with_progress_receipt(
        mut self,
        progress_receipt: DraftPieceBuildProgressReceiptReferenceV1,
    ) -> Self {
        self.progress_receipt = progress_receipt;
        self
    }
    pub const fn durable_continuation(&self) -> Option<DraftPieceDurableBuildContinuationV1> {
        self.durable_continuation
    }
    pub(crate) fn with_durable_continuation(
        mut self,
        durable_continuation: Option<DraftPieceDurableBuildContinuationV1>,
    ) -> Self {
        self.durable_continuation = durable_continuation;
        self
    }
    pub const fn successor(&self) -> Option<DraftPieceRootReferenceV1> {
        self.successor
    }
    pub const fn build_digest(&self) -> Option<DraftPieceDigestV1> {
        self.build_digest
    }
    pub const fn lifecycle(&self) -> DraftPieceBuildLifecycleV1 {
        self.lifecycle
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceCanonicalFragmentEndpointV1 {
    key: DraftPieceBuildFragmentKeyV1,
    digest: DraftPieceDigestV1,
    chain: DraftPieceDigestV1,
}

impl DraftPieceCanonicalFragmentEndpointV1 {
    pub const fn new(
        key: DraftPieceBuildFragmentKeyV1,
        digest: DraftPieceDigestV1,
        chain: DraftPieceDigestV1,
    ) -> Self {
        Self { key, digest, chain }
    }

    pub const fn key(self) -> DraftPieceBuildFragmentKeyV1 {
        self.key
    }

    pub const fn digest(self) -> DraftPieceDigestV1 {
        self.digest
    }

    pub const fn chain(self) -> DraftPieceDigestV1 {
        self.chain
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceBuildProgressReceiptV1 {
    reference: DraftPieceBuildProgressReceiptReferenceV1,
    previous: Option<DraftPieceBuildProgressReceiptReferenceV1>,
    fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
    state_digest: DraftPieceDigestV1,
    working_roots: DraftPieceBuildRootsV1,
    base_frontier: DraftPieceBuildBoundaryV1,
    successor_frontier: DraftPieceBuildBoundaryV1,
    next_record_ordinal: u64,
    frontier: DraftPieceBuildFrontierV1,
    durable_continuation: Option<DraftPieceDurableBuildContinuationV1>,
    successor: Option<DraftPieceRootReferenceV1>,
    build_digest: Option<DraftPieceDigestV1>,
    lifecycle: DraftPieceBuildLifecycleV1,
}

impl DraftPieceBuildProgressReceiptV1 {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        reference: DraftPieceBuildProgressReceiptReferenceV1,
        previous: Option<DraftPieceBuildProgressReceiptReferenceV1>,
        fragment_endpoint: Option<DraftPieceCanonicalFragmentEndpointV1>,
        state_digest: DraftPieceDigestV1,
        working_roots: DraftPieceBuildRootsV1,
        base_frontier: DraftPieceBuildBoundaryV1,
        successor_frontier: DraftPieceBuildBoundaryV1,
        next_record_ordinal: u64,
        frontier: DraftPieceBuildFrontierV1,
        successor: Option<DraftPieceRootReferenceV1>,
        build_digest: Option<DraftPieceDigestV1>,
        lifecycle: DraftPieceBuildLifecycleV1,
    ) -> Self {
        Self {
            reference,
            previous,
            fragment_endpoint,
            state_digest,
            working_roots,
            base_frontier,
            successor_frontier,
            next_record_ordinal,
            frontier,
            durable_continuation: None,
            successor,
            build_digest,
            lifecycle,
        }
    }

    pub const fn key(&self) -> DraftPieceBuildProgressReceiptKeyV1 {
        self.reference.key()
    }
    pub const fn reference(&self) -> DraftPieceBuildProgressReceiptReferenceV1 {
        self.reference
    }
    pub const fn previous(&self) -> Option<DraftPieceBuildProgressReceiptReferenceV1> {
        self.previous
    }
    pub const fn fragment_endpoint(&self) -> Option<DraftPieceCanonicalFragmentEndpointV1> {
        self.fragment_endpoint
    }
    pub const fn state_digest(&self) -> DraftPieceDigestV1 {
        self.state_digest
    }
    pub const fn working_roots(&self) -> DraftPieceBuildRootsV1 {
        self.working_roots
    }
    pub const fn base_frontier(&self) -> DraftPieceBuildBoundaryV1 {
        self.base_frontier
    }
    pub const fn successor_frontier(&self) -> DraftPieceBuildBoundaryV1 {
        self.successor_frontier
    }
    pub const fn next_record_ordinal(&self) -> u64 {
        self.next_record_ordinal
    }
    pub const fn frontier(&self) -> DraftPieceBuildFrontierV1 {
        self.frontier
    }
    pub const fn durable_continuation(&self) -> Option<DraftPieceDurableBuildContinuationV1> {
        self.durable_continuation
    }
    pub(crate) fn with_durable_continuation(
        mut self,
        durable_continuation: Option<DraftPieceDurableBuildContinuationV1>,
    ) -> Self {
        self.durable_continuation = durable_continuation;
        self
    }
    pub const fn successor(&self) -> Option<DraftPieceRootReferenceV1> {
        self.successor
    }
    pub const fn build_digest(&self) -> Option<DraftPieceDigestV1> {
        self.build_digest
    }
    pub const fn lifecycle(&self) -> DraftPieceBuildLifecycleV1 {
        self.lifecycle
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPieceBuildFragmentKeyV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
    ordinal: u64,
}

impl DraftPieceBuildFragmentKeyV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
        ordinal: u64,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            operation_id,
            ordinal,
        }
    }

    pub const fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }
    pub const fn operation_id(&self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }
    pub const fn session_id(&self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    pub(crate) const fn is_locally_valid(&self) -> bool {
        self.ordinal != 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceBuildFragmentV1 {
    key: DraftPieceBuildFragmentKeyV1,
    replacement: DraftPieceReplacementV1,
    preceding_chain: DraftPieceDigestV1,
    chain_digest: DraftPieceDigestV1,
}

impl DraftPieceBuildFragmentV1 {
    pub const fn new(
        key: DraftPieceBuildFragmentKeyV1,
        replacement: DraftPieceReplacementV1,
        preceding_chain: DraftPieceDigestV1,
        chain_digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            key,
            replacement,
            preceding_chain,
            chain_digest,
        }
    }

    pub const fn key(&self) -> DraftPieceBuildFragmentKeyV1 {
        self.key
    }
    pub const fn replacement(&self) -> &DraftPieceReplacementV1 {
        &self.replacement
    }
    pub const fn preceding_chain(&self) -> DraftPieceDigestV1 {
        self.preceding_chain
    }
    pub const fn chain_digest(&self) -> DraftPieceDigestV1 {
        self.chain_digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceRejectedReasonV1 {
    EmptyTransaction,
    TooManyReplacements,
    InsertedPayloadTooLarge,
    EmptyTextLeaf,
    InvalidUtf8Boundary,
    InvalidGapWitness,
    OutOfOrder,
    Overlap,
    DuplicateEmptyRange,
    DuplicateMarkerIdentity,
    DuplicateMarkerOrder,
    AggregateOverflow,
    TreeLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceErrorReasonV1 {
    OccupiedIdentity,
    UnsettledOperation,
    MissingRecord,
    CorruptRecord,
    ResourceLimit,
    OccupiedIdentityNoncommit,
    HistoryCapacityUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPieceSettlementOutcomeV1 {
    Committed {
        candidate_generation: u64,
        successor: DraftPieceRootReferenceV1,
        history: DraftEditHistoryFrontierReferenceV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
    },
    Rejected(DraftPieceRejectedReasonV1),
    Conflict {
        current_candidate_generation: u64,
        current_root: DraftPieceRootReferenceV1,
        current_history: DraftEditHistoryFrontierReferenceV1,
    },
    Cancelled,
    Error(DraftPieceErrorReasonV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceCommittedAdoptionV1 {
    predecessor_session: DraftEditorCandidateSessionV1,
    adopted_session: DraftEditorCandidateSessionV1,
    adopted_root: DraftPieceRootRecordV1,
    predecessor_history: DraftEditHistoryFrontierV1,
    transition: DraftEditHistoryTransitionV1,
    adopted_history: DraftEditHistoryFrontierV1,
}

impl DraftPieceCommittedAdoptionV1 {
    pub const fn new(
        predecessor_session: DraftEditorCandidateSessionV1,
        adopted_session: DraftEditorCandidateSessionV1,
        adopted_root: DraftPieceRootRecordV1,
        predecessor_history: DraftEditHistoryFrontierV1,
        transition: DraftEditHistoryTransitionV1,
        adopted_history: DraftEditHistoryFrontierV1,
    ) -> Self {
        Self {
            predecessor_session,
            adopted_session,
            adopted_root,
            predecessor_history,
            transition,
            adopted_history,
        }
    }

    pub const fn predecessor_session(&self) -> &DraftEditorCandidateSessionV1 {
        &self.predecessor_session
    }

    pub const fn adopted_session(&self) -> &DraftEditorCandidateSessionV1 {
        &self.adopted_session
    }

    pub const fn adopted_root(&self) -> &DraftPieceRootRecordV1 {
        &self.adopted_root
    }
    pub const fn predecessor_history(&self) -> &DraftEditHistoryFrontierV1 {
        &self.predecessor_history
    }
    pub const fn transition(&self) -> &DraftEditHistoryTransitionV1 {
        &self.transition
    }
    pub const fn adopted_history(&self) -> &DraftEditHistoryFrontierV1 {
        &self.adopted_history
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceNoncommitClosureV1 {
    observed_session: DraftEditorCandidateSessionV1,
    observed_history: DraftEditHistoryFrontierV1,
    proposed_successor: Option<DraftPieceRootReferenceV1>,
    occupied_identity: Option<OccupiedIdentityNoncommitProofV1>,
}

impl DraftPieceNoncommitClosureV1 {
    pub const fn new(
        observed_session: DraftEditorCandidateSessionV1,
        observed_history: DraftEditHistoryFrontierV1,
        proposed_successor: Option<DraftPieceRootReferenceV1>,
    ) -> Self {
        Self {
            observed_session,
            observed_history,
            proposed_successor,
            occupied_identity: None,
        }
    }

    pub const fn with_occupied_identity(
        observed_session: DraftEditorCandidateSessionV1,
        observed_history: DraftEditHistoryFrontierV1,
        proposed_successor: DraftPieceRootReferenceV1,
        occupied_identity: OccupiedIdentityNoncommitProofV1,
    ) -> Self {
        Self {
            observed_session,
            observed_history,
            proposed_successor: Some(proposed_successor),
            occupied_identity: Some(occupied_identity),
        }
    }

    pub const fn observed_session(&self) -> &DraftEditorCandidateSessionV1 {
        &self.observed_session
    }
    pub const fn observed_history(&self) -> &DraftEditHistoryFrontierV1 {
        &self.observed_history
    }

    pub const fn proposed_successor(&self) -> Option<DraftPieceRootReferenceV1> {
        self.proposed_successor
    }

    pub const fn occupied_identity(&self) -> Option<&OccupiedIdentityNoncommitProofV1> {
        self.occupied_identity.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPieceSettlementClosureV1 {
    Committed(DraftPieceCommittedAdoptionV1),
    Noncommit(DraftPieceNoncommitClosureV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceSettlementV1 {
    key: DraftPieceSettlementKeyV1,
    proposal_digest: DraftPieceDigestV1,
    predecessor_candidate_generation: u64,
    predecessor_root: DraftPieceRootReferenceV1,
    predecessor_history: DraftEditHistoryFrontierReferenceV1,
    fragment_count: u64,
    fragment_chain: DraftPieceDigestV1,
    predecessor_caret: DraftCompositePositionV1,
    predecessor_selection: DraftCompositePositionV1,
    caret: DraftCompositePositionV1,
    selection: DraftCompositePositionV1,
    build_digest: Option<DraftPieceDigestV1>,
    canonical_header: Vec<u8>,
    terminal_source: Option<DraftPieceBuildRecordV1>,
    terminal_receipt: DraftPieceBuildProgressReceiptReferenceV1,
    outcome: DraftPieceSettlementOutcomeV1,
    closure: Box<DraftPieceSettlementClosureV1>,
}

impl DraftPieceSettlementV1 {
    pub fn new(
        key: DraftPieceSettlementKeyV1,
        proposal_digest: DraftPieceDigestV1,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        fragment_count: u64,
        fragment_chain: DraftPieceDigestV1,
        predecessor_caret: DraftCompositePositionV1,
        predecessor_selection: DraftCompositePositionV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
        build_digest: Option<DraftPieceDigestV1>,
        canonical_header: Vec<u8>,
        terminal_source: Option<DraftPieceBuildRecordV1>,
        terminal_receipt: DraftPieceBuildProgressReceiptReferenceV1,
        outcome: DraftPieceSettlementOutcomeV1,
        closure: DraftPieceSettlementClosureV1,
    ) -> Self {
        Self {
            key,
            proposal_digest,
            predecessor_candidate_generation,
            predecessor_root,
            predecessor_history,
            fragment_count,
            fragment_chain,
            predecessor_caret,
            predecessor_selection,
            caret,
            selection,
            build_digest,
            canonical_header,
            terminal_source,
            terminal_receipt,
            outcome,
            closure: Box::new(closure),
        }
    }

    pub(crate) fn new_boxed(
        key: DraftPieceSettlementKeyV1,
        proposal_digest: DraftPieceDigestV1,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        fragment_count: u64,
        fragment_chain: DraftPieceDigestV1,
        predecessor_caret: DraftCompositePositionV1,
        predecessor_selection: DraftCompositePositionV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
        build_digest: Option<DraftPieceDigestV1>,
        canonical_header: Vec<u8>,
        terminal_source: Option<DraftPieceBuildRecordV1>,
        terminal_receipt: DraftPieceBuildProgressReceiptReferenceV1,
        outcome: DraftPieceSettlementOutcomeV1,
        closure: Box<DraftPieceSettlementClosureV1>,
    ) -> Self {
        Self {
            key,
            proposal_digest,
            predecessor_candidate_generation,
            predecessor_root,
            predecessor_history,
            fragment_count,
            fragment_chain,
            predecessor_caret,
            predecessor_selection,
            caret,
            selection,
            build_digest,
            canonical_header,
            terminal_source,
            terminal_receipt,
            outcome,
            closure,
        }
    }

    pub const fn key(&self) -> DraftPieceSettlementKeyV1 {
        self.key
    }
    pub const fn proposal_digest(&self) -> DraftPieceDigestV1 {
        self.proposal_digest
    }
    pub const fn predecessor_candidate_generation(&self) -> u64 {
        self.predecessor_candidate_generation
    }
    pub const fn predecessor_root(&self) -> DraftPieceRootReferenceV1 {
        self.predecessor_root
    }
    pub const fn predecessor_history(&self) -> DraftEditHistoryFrontierReferenceV1 {
        self.predecessor_history
    }
    pub const fn fragment_count(&self) -> u64 {
        self.fragment_count
    }
    pub const fn fragment_chain(&self) -> DraftPieceDigestV1 {
        self.fragment_chain
    }
    pub const fn predecessor_caret(&self) -> DraftCompositePositionV1 {
        self.predecessor_caret
    }
    pub const fn predecessor_selection(&self) -> DraftCompositePositionV1 {
        self.predecessor_selection
    }
    pub const fn caret(&self) -> DraftCompositePositionV1 {
        self.caret
    }
    pub const fn selection(&self) -> DraftCompositePositionV1 {
        self.selection
    }
    pub const fn build_digest(&self) -> Option<DraftPieceDigestV1> {
        self.build_digest
    }
    pub fn canonical_header(&self) -> &[u8] {
        &self.canonical_header
    }
    pub const fn terminal_source(&self) -> Option<&DraftPieceBuildRecordV1> {
        self.terminal_source.as_ref()
    }
    pub const fn terminal_receipt(&self) -> DraftPieceBuildProgressReceiptReferenceV1 {
        self.terminal_receipt
    }
    pub const fn outcome(&self) -> &DraftPieceSettlementOutcomeV1 {
        &self.outcome
    }
    pub const fn closure(&self) -> &DraftPieceSettlementClosureV1 {
        &self.closure
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OccupiedIdentityDifferenceV1 {
    Header {
        offset: u64,
        requested: Option<u8>,
        occupied: Option<u8>,
    },
    Fragment {
        key: DraftPieceBuildFragmentKeyV1,
        requested: Option<DraftPieceBuildFragmentV1>,
        occupied: Option<DraftPieceBuildFragmentV1>,
    },
    Root {
        key: DraftPieceRootKeyV1,
        requested: DraftPieceRootRecordV1,
        occupied: DraftPieceRootRecordV1,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OccupiedIdentityNoncommitProofV1 {
    requested_proposal_digest: DraftPieceDigestV1,
    occupied_proposal_digest: DraftPieceDigestV1,
    key: DraftPieceSettlementKeyV1,
    difference: OccupiedIdentityDifferenceV1,
}

impl OccupiedIdentityNoncommitProofV1 {
    pub const fn new(
        requested_proposal_digest: DraftPieceDigestV1,
        occupied_proposal_digest: DraftPieceDigestV1,
        key: DraftPieceSettlementKeyV1,
        difference: OccupiedIdentityDifferenceV1,
    ) -> Self {
        Self {
            requested_proposal_digest,
            occupied_proposal_digest,
            key,
            difference,
        }
    }

    pub const fn requested_proposal_digest(&self) -> DraftPieceDigestV1 {
        self.requested_proposal_digest
    }
    pub const fn occupied_proposal_digest(&self) -> DraftPieceDigestV1 {
        self.occupied_proposal_digest
    }
    pub const fn key(&self) -> DraftPieceSettlementKeyV1 {
        self.key
    }

    pub const fn difference(&self) -> &OccupiedIdentityDifferenceV1 {
        &self.difference
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPieceSettlementProofV1 {
    Settlement(DraftPieceSettlementV1),
    OccupiedIdentityNoncommit(OccupiedIdentityNoncommitProofV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPieceTransactionOutcomeV1 {
    Committed(DraftPieceSettlementProofV1),
    Rejected(DraftPieceSettlementProofV1),
    Conflict(DraftPieceSettlementProofV1),
    Cancelled(DraftPieceSettlementProofV1),
    Error(DraftPieceSettlementProofV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPieceReconciledCommandV1 {
    Pending(DraftPieceOperationStatusV1),
    Terminal(DraftPieceTransactionOutcomeV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPieceOperationStatusV1 {
    Absent,
    Open(DraftPieceBuildRecordV1),
    Complete(DraftPieceBuildRecordV1),
    Settled(DraftPieceSettlementV1),
    Collision(OccupiedIdentityNoncommitProofV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftPieceOperationVerificationV1 {
    More { next_ordinal: u64 },
    Status(DraftPieceOperationStatusV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceTextDemandV1 {
    Forward(u64),
    Backward(u64),
    Validate(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceTextEdgeFactV1 {
    DocumentStart,
    DocumentEnd,
    Continuation(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceTextDemandResultV1 {
    root: DraftPieceRootReferenceV1,
    demand: DraftPieceTextDemandV1,
    start: u64,
    end: u64,
    bytes: Vec<u8>,
    summary: DraftPieceTextSummaryV1,
    preceding: DraftPieceTextEdgeFactV1,
    following: DraftPieceTextEdgeFactV1,
    records_read: u64,
}

impl DraftPieceTextDemandResultV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        root: DraftPieceRootReferenceV1,
        demand: DraftPieceTextDemandV1,
        start: u64,
        end: u64,
        bytes: Vec<u8>,
        preceding: DraftPieceTextEdgeFactV1,
        following: DraftPieceTextEdgeFactV1,
        records_read: u64,
    ) -> Self {
        let summary = DraftPieceTextSummaryV1::from_utf8(
            std::str::from_utf8(&bytes).expect("range source produced validated UTF-8"),
        );
        Self {
            root,
            demand,
            start,
            end,
            bytes,
            summary,
            preceding,
            following,
            records_read,
        }
    }
    pub const fn root(&self) -> DraftPieceRootReferenceV1 {
        self.root
    }
    pub const fn demand(&self) -> DraftPieceTextDemandV1 {
        self.demand
    }
    pub const fn start(&self) -> u64 {
        self.start
    }
    pub const fn end(&self) -> u64 {
        self.end
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub const fn summary(&self) -> DraftPieceTextSummaryV1 {
        self.summary
    }
    pub const fn preceding(&self) -> DraftPieceTextEdgeFactV1 {
        self.preceding
    }
    pub const fn following(&self) -> DraftPieceTextEdgeFactV1 {
        self.following
    }
    pub const fn records_read(&self) -> u64 {
        self.records_read
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceMarkerDirectionV1 {
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceMarkerScopeV1 {
    Range { start: u64, end: u64 },
    ExactAnchor(u64),
}

impl DraftPieceMarkerScopeV1 {
    pub const fn bounds(self) -> (u64, u64, bool) {
        match self {
            Self::Range { start, end } => (start, end, false),
            Self::ExactAnchor(anchor) => (anchor, anchor, true),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceMarkerEdgeFactV1 {
    RangeStart,
    RangeEnd,
    Marker(DraftCompositeSearchKeyV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceMarkerDemandV1 {
    scope: DraftPieceMarkerScopeV1,
    direction: DraftPieceMarkerDirectionV1,
    cursor: Option<DraftCompositeSearchKeyV1>,
    object_ceiling: usize,
    retained_byte_ceiling: usize,
}

impl DraftPieceMarkerDemandV1 {
    pub const fn new(
        scope: DraftPieceMarkerScopeV1,
        direction: DraftPieceMarkerDirectionV1,
        cursor: Option<DraftCompositeSearchKeyV1>,
        object_ceiling: usize,
        retained_byte_ceiling: usize,
    ) -> Self {
        Self {
            scope,
            direction,
            cursor,
            object_ceiling,
            retained_byte_ceiling,
        }
    }
    pub const fn scope(&self) -> DraftPieceMarkerScopeV1 {
        self.scope
    }
    pub const fn direction(&self) -> DraftPieceMarkerDirectionV1 {
        self.direction
    }
    pub const fn cursor(&self) -> Option<DraftCompositeSearchKeyV1> {
        self.cursor
    }
    pub const fn object_ceiling(&self) -> usize {
        self.object_ceiling
    }
    pub const fn retained_byte_ceiling(&self) -> usize {
        self.retained_byte_ceiling
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceMarkerDemandResultV1 {
    root: DraftPieceRootReferenceV1,
    scope: DraftPieceMarkerScopeV1,
    direction: DraftPieceMarkerDirectionV1,
    markers: Vec<DraftPieceMarkerAtV1>,
    preceding: DraftPieceMarkerEdgeFactV1,
    following: DraftPieceMarkerEdgeFactV1,
    requested_side_complete: bool,
    continuation: Option<DraftCompositeSearchKeyV1>,
    retained_bytes: usize,
    records_read: u64,
}

impl DraftPieceMarkerDemandResultV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        root: DraftPieceRootReferenceV1,
        demand: &DraftPieceMarkerDemandV1,
        markers: Vec<DraftPieceMarkerAtV1>,
        preceding: DraftPieceMarkerEdgeFactV1,
        following: DraftPieceMarkerEdgeFactV1,
        requested_side_complete: bool,
        continuation: Option<DraftCompositeSearchKeyV1>,
        retained_bytes: usize,
        records_read: u64,
    ) -> Self {
        Self {
            root,
            scope: demand.scope(),
            direction: demand.direction(),
            markers,
            preceding,
            following,
            requested_side_complete,
            continuation,
            retained_bytes,
            records_read,
        }
    }
    pub const fn root(&self) -> DraftPieceRootReferenceV1 {
        self.root
    }
    pub const fn scope(&self) -> DraftPieceMarkerScopeV1 {
        self.scope
    }
    pub const fn direction(&self) -> DraftPieceMarkerDirectionV1 {
        self.direction
    }
    pub fn markers(&self) -> &[DraftPieceMarkerAtV1] {
        &self.markers
    }
    pub const fn preceding(&self) -> DraftPieceMarkerEdgeFactV1 {
        self.preceding
    }
    pub const fn following(&self) -> DraftPieceMarkerEdgeFactV1 {
        self.following
    }
    pub const fn requested_side_complete(&self) -> bool {
        self.requested_side_complete
    }
    pub const fn continuation(&self) -> Option<DraftCompositeSearchKeyV1> {
        self.continuation
    }
    pub const fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }
    pub const fn records_read(&self) -> u64 {
        self.records_read
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceMarkerEdgeProofRequestV1 {
    Absence {
        anchor: u64,
    },
    First {
        marker: DraftPieceMarkerAtV1,
    },
    Last {
        marker: DraftPieceMarkerAtV1,
    },
    Adjacent {
        left: DraftPieceMarkerAtV1,
        right: DraftPieceMarkerAtV1,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceMarkerEdgeProofV1 {
    Absence {
        anchor: u64,
    },
    First {
        marker: DraftPieceMarkerAtV1,
    },
    Last {
        marker: DraftPieceMarkerAtV1,
    },
    Adjacent {
        left: DraftPieceMarkerAtV1,
        right: DraftPieceMarkerAtV1,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftPieceMalformedRangeRequestV1 {
    Coordinate,
    Range,
    Cursor,
    Limit,
    Utf8Boundary,
    MarkerFacts,
}

#[derive(Debug)]
pub enum DraftPieceRangeSourceErrorV1 {
    Malformed(DraftPieceMalformedRangeRequestV1),
    Limit,
    Absent,
    ConcurrentChange,
    StaleSession,
    StaleCandidate,
    Disposed(DraftEditorCandidateSessionV1),
    Operational(crate::SyndicReadError),
    Invariant,
}

impl std::fmt::Display for DraftPieceRangeSourceErrorV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "draft-piece range source failed: {self:?}")
    }
}

impl std::error::Error for DraftPieceRangeSourceErrorV1 {}

impl From<crate::SyndicReadError> for DraftPieceRangeSourceErrorV1 {
    fn from(value: crate::SyndicReadError) -> Self {
        match value {
            crate::SyndicReadError::Read(
                error @ (beryl_home_store::ReadError::HealthGate(_)
                | beryl_home_store::ReadError::GenerationPoisoned
                | beryl_home_store::ReadError::Storage { .. }),
            ) => Self::Operational(crate::SyndicReadError::Read(error)),
            crate::SyndicReadError::ConcurrentChange { .. } => Self::ConcurrentChange,
            _ => Self::Invariant,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceCurrentRangeResultV1<T> {
    selector: DraftEditorCurrentSelectorV1,
    value: T,
}

impl<T> DraftPieceCurrentRangeResultV1<T> {
    pub const fn new(selector: DraftEditorCurrentSelectorV1, value: T) -> Self {
        Self { selector, value }
    }
    pub const fn selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.selector
    }
    pub const fn value(&self) -> &T {
        &self.value
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceCandidateRangeResultV1<T> {
    binding: DraftEditorCandidateActivationBindingV1,
    value: T,
}

impl<T> DraftPieceCandidateRangeResultV1<T> {
    pub const fn new(binding: DraftEditorCandidateActivationBindingV1, value: T) -> Self {
        Self { binding, value }
    }
    pub const fn binding(&self) -> DraftEditorCandidateActivationBindingV1 {
        self.binding
    }
    pub const fn value(&self) -> &T {
        &self.value
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftPieceMarkerAtV1 {
    anchor: u64,
    marker: DraftPieceMarkerV1,
}

impl DraftPieceMarkerAtV1 {
    pub const fn new(anchor: u64, marker: DraftPieceMarkerV1) -> Self {
        Self { anchor, marker }
    }
    pub const fn anchor(&self) -> u64 {
        self.anchor
    }
    pub const fn marker(&self) -> DraftPieceMarkerV1 {
        self.marker
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftPieceRestorationV1 {
    root: DraftPieceRootReferenceV1,
    history: DraftEditHistoryFrontierReferenceV1,
    caret: DraftCompositePositionV1,
    selection: DraftCompositePositionV1,
    scroll: DraftCompositePositionV1,
}

impl DraftPieceRestorationV1 {
    pub const fn new(
        root: DraftPieceRootReferenceV1,
        history: DraftEditHistoryFrontierReferenceV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
        scroll: DraftCompositePositionV1,
    ) -> Self {
        Self {
            root,
            history,
            caret,
            selection,
            scroll,
        }
    }
    pub const fn root(&self) -> DraftPieceRootReferenceV1 {
        self.root
    }
    pub const fn history(&self) -> DraftEditHistoryFrontierReferenceV1 {
        self.history
    }
    pub const fn caret(&self) -> DraftCompositePositionV1 {
        self.caret
    }
    pub const fn selection(&self) -> DraftCompositePositionV1 {
        self.selection
    }
    pub const fn scroll(&self) -> DraftCompositePositionV1 {
        self.scroll
    }
}

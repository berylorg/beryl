use beryl_model::SyndicDraftId;

use super::super::super::{
    DraftCompositePositionV1, DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionV1,
    DraftPieceOperationIdV1, DraftPieceRootRecordV1, DraftPieceRootReferenceV1,
};
use super::super::{
    DraftEditHistoryFrontierReferenceV1, DraftEditHistoryFrontierV1,
    DraftEditHistoryTransitionKindV1, DraftEditHistoryTransitionReferenceV1,
    DraftEditHistoryTransitionV1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftHistoricalRootAdoptionKeyV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftPieceOperationIdV1,
}

impl DraftHistoricalRootAdoptionKeyV1 {
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
    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }
    pub const fn operation_id(self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftHistoricalRootDirectionV1 {
    Undo,
    Redo,
}

impl DraftHistoricalRootDirectionV1 {
    pub(crate) const fn transition_kind(self) -> DraftEditHistoryTransitionKindV1 {
        match self {
            Self::Undo => DraftEditHistoryTransitionKindV1::Undo,
            Self::Redo => DraftEditHistoryTransitionKindV1::Redo,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftHistoricalRootAdoptionRequestV1 {
    key: DraftHistoricalRootAdoptionKeyV1,
    source_history: DraftEditHistoryFrontierReferenceV1,
    selected_transition: DraftEditHistoryTransitionReferenceV1,
    direction: DraftHistoricalRootDirectionV1,
    target_root: DraftPieceRootReferenceV1,
    caret: DraftCompositePositionV1,
    selection: DraftCompositePositionV1,
}

impl DraftHistoricalRootAdoptionRequestV1 {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftPieceOperationIdV1,
        source_history: DraftEditHistoryFrontierReferenceV1,
        selected_transition: DraftEditHistoryTransitionReferenceV1,
        direction: DraftHistoricalRootDirectionV1,
        target_root: DraftPieceRootReferenceV1,
        caret: DraftCompositePositionV1,
        selection: DraftCompositePositionV1,
    ) -> Self {
        Self {
            key: DraftHistoricalRootAdoptionKeyV1::new(draft_id, session_id, operation_id),
            source_history,
            selected_transition,
            direction,
            target_root,
            caret,
            selection,
        }
    }

    pub const fn key(self) -> DraftHistoricalRootAdoptionKeyV1 {
        self.key
    }
    pub const fn source_history(self) -> DraftEditHistoryFrontierReferenceV1 {
        self.source_history
    }
    pub const fn selected_transition(self) -> DraftEditHistoryTransitionReferenceV1 {
        self.selected_transition
    }
    pub const fn direction(self) -> DraftHistoricalRootDirectionV1 {
        self.direction
    }
    pub const fn target_root(self) -> DraftPieceRootReferenceV1 {
        self.target_root
    }
    pub const fn caret(self) -> DraftCompositePositionV1 {
        self.caret
    }
    pub const fn selection(self) -> DraftCompositePositionV1 {
        self.selection
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftHistoricalRootAdoptionErrorReasonV1 {
    InvalidAuthority,
    HistoryCapacityUnavailable,
    OccupiedIdentity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftHistoricalRootAdoptionSettlementOutcomeV1 {
    Committed,
    Rejected,
    Conflict,
    Cancelled,
    Error(DraftHistoricalRootAdoptionErrorReasonV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftHistoricalRootAdoptionV1 {
    request: DraftHistoricalRootAdoptionRequestV1,
    request_bytes: Vec<u8>,
    source_history: DraftEditHistoryFrontierV1,
    selected_transition: DraftEditHistoryTransitionV1,
    target_root: DraftPieceRootRecordV1,
    outcome: DraftHistoricalRootAdoptionSettlementOutcomeV1,
    successor_transition: Option<DraftEditHistoryTransitionV1>,
    successor_history: Option<DraftEditHistoryFrontierV1>,
    successor_candidate: Option<DraftEditorCandidateSessionV1>,
}

impl DraftHistoricalRootAdoptionV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        request: DraftHistoricalRootAdoptionRequestV1,
        request_bytes: Vec<u8>,
        source_history: DraftEditHistoryFrontierV1,
        selected_transition: DraftEditHistoryTransitionV1,
        target_root: DraftPieceRootRecordV1,
        outcome: DraftHistoricalRootAdoptionSettlementOutcomeV1,
        successor_transition: Option<DraftEditHistoryTransitionV1>,
        successor_history: Option<DraftEditHistoryFrontierV1>,
        successor_candidate: Option<DraftEditorCandidateSessionV1>,
    ) -> Self {
        Self {
            request,
            request_bytes,
            source_history,
            selected_transition,
            target_root,
            outcome,
            successor_transition,
            successor_history,
            successor_candidate,
        }
    }

    pub const fn key(&self) -> DraftHistoricalRootAdoptionKeyV1 {
        self.request.key()
    }
    pub const fn request(&self) -> DraftHistoricalRootAdoptionRequestV1 {
        self.request
    }
    pub fn request_bytes(&self) -> &[u8] {
        &self.request_bytes
    }
    pub const fn source_history(&self) -> &DraftEditHistoryFrontierV1 {
        &self.source_history
    }
    pub const fn selected_transition(&self) -> &DraftEditHistoryTransitionV1 {
        &self.selected_transition
    }
    pub const fn target_root(&self) -> &DraftPieceRootRecordV1 {
        &self.target_root
    }
    pub const fn outcome(&self) -> DraftHistoricalRootAdoptionSettlementOutcomeV1 {
        self.outcome
    }
    pub const fn successor_transition(&self) -> Option<&DraftEditHistoryTransitionV1> {
        self.successor_transition.as_ref()
    }
    pub const fn successor_history(&self) -> Option<&DraftEditHistoryFrontierV1> {
        self.successor_history.as_ref()
    }
    pub const fn successor_candidate(&self) -> Option<&DraftEditorCandidateSessionV1> {
        self.successor_candidate.as_ref()
    }

    pub(crate) fn is_locally_valid(&self) -> bool {
        let request = self.request;
        let committed = self.outcome == DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed;
        let successor_shape = self.successor_transition.is_some()
            && self.successor_history.is_some()
            && self.successor_candidate.is_some();
        request.key().draft_id() == request.source_history().key().draft_id()
            && request.source_history().key().session_id() == Some(request.key().session_id())
            && request.selected_transition().key().draft_id() == request.key().draft_id()
            && request.target_root().key().draft_id() == request.key().draft_id()
            && self.source_history.reference() == request.source_history()
            && self.selected_transition.reference() == request.selected_transition()
            && self.target_root.reference() == request.target_root()
            && self.source_history.reference().root() == self.selected_transition.successor_root()
            && self.selected_transition.predecessor_root() == request.target_root()
            && self.selected_transition.before_caret() == request.caret()
            && self.selected_transition.before_selection() == request.selection()
            && committed == successor_shape
            && (!committed || self.committed_shape_is_exact())
    }

    fn committed_shape_is_exact(&self) -> bool {
        let Some(transition) = self.successor_transition.as_ref() else {
            return false;
        };
        let Some(history) = self.successor_history.as_ref() else {
            return false;
        };
        let Some(candidate) = self.successor_candidate.as_ref() else {
            return false;
        };
        transition.kind() == self.request.direction().transition_kind()
            && transition.operation_id() == self.request.key().operation_id()
            && transition.predecessor_root() == self.source_history.reference().root()
            && transition.successor_root() == self.request.target_root()
            && transition.prior_journal() == self.source_history.journal_head()
            && transition.prior_undo() == self.source_history.undo_head()
            && transition.prior_redo() == self.source_history.redo_head()
            && history.reference().root() == self.request.target_root()
            && history.reference().candidate_generation()
                == self
                    .source_history
                    .reference()
                    .candidate_generation()
                    .checked_add(1)
                    .unwrap_or(0)
            && history.journal_head() == Some(transition.reference())
            && candidate.draft_id() == self.request.key().draft_id()
            && candidate.session_id() == self.request.key().session_id()
            && candidate.newest_candidate_generation() == history.reference().candidate_generation()
            && candidate.newest_root() == self.request.target_root()
            && candidate.newest_history() == history.reference()
            && candidate.active_operation().is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftHistoricalRootAdoptionProofV1 {
    settlement: DraftHistoricalRootAdoptionV1,
}

impl DraftHistoricalRootAdoptionProofV1 {
    pub(crate) const fn new(settlement: DraftHistoricalRootAdoptionV1) -> Self {
        Self { settlement }
    }
    pub const fn settlement(&self) -> &DraftHistoricalRootAdoptionV1 {
        &self.settlement
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftHistoricalRootAdoptionOutcomeV1 {
    Committed(DraftHistoricalRootAdoptionProofV1),
    Rejected(DraftHistoricalRootAdoptionProofV1),
    Conflict(DraftHistoricalRootAdoptionProofV1),
    Cancelled(DraftHistoricalRootAdoptionProofV1),
    Error(DraftHistoricalRootAdoptionProofV1),
}

impl DraftHistoricalRootAdoptionOutcomeV1 {
    pub(crate) fn from_settlement(settlement: DraftHistoricalRootAdoptionV1) -> Self {
        let outcome = settlement.outcome();
        let proof = DraftHistoricalRootAdoptionProofV1::new(settlement);
        match outcome {
            DraftHistoricalRootAdoptionSettlementOutcomeV1::Committed => Self::Committed(proof),
            DraftHistoricalRootAdoptionSettlementOutcomeV1::Rejected => Self::Rejected(proof),
            DraftHistoricalRootAdoptionSettlementOutcomeV1::Conflict => Self::Conflict(proof),
            DraftHistoricalRootAdoptionSettlementOutcomeV1::Cancelled => Self::Cancelled(proof),
            DraftHistoricalRootAdoptionSettlementOutcomeV1::Error(_) => Self::Error(proof),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftHistoricalRootAdoptionStatusV1 {
    Absent,
    Settled(DraftHistoricalRootAdoptionOutcomeV1),
    Collision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftHistoricalRootAdoptionReconciliationV1 {
    ExactOld,
    ExactNew(DraftHistoricalRootAdoptionOutcomeV1),
    Collision,
}

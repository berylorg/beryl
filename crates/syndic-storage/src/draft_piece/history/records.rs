use super::super::{
    DraftCompositePositionV1, DraftEditorCandidateSessionIdV1, DraftPieceDigestV1,
    DraftPieceOperationIdV1, DraftPieceRootReferenceV1,
};
use super::{
    append::stored_frontier_charge,
    codec::{authenticated_frontier, frontier_digest, transition_digest},
    references::*,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditHistoryFrontierV1 {
    pub(super) reference: DraftEditHistoryFrontierReferenceV1,
    pub(super) journal_head: Option<DraftEditHistoryTransitionReferenceV1>,
    pub(super) undo_head: Option<DraftEditHistoryTransitionReferenceV1>,
    pub(super) redo_head: Option<DraftEditHistoryTransitionReferenceV1>,
    pub(super) oldest_eligible: Option<DraftEditHistoryTransitionReferenceV1>,
    pub(super) cumulative_encoded_bytes: u64,
    pub(super) retained_encoded_bytes: u64,
    pub(super) byte_budget: u64,
    pub(super) retention_policy_revision: u64,
}

impl DraftEditHistoryFrontierV1 {
    #[allow(clippy::too_many_arguments)]
    pub const fn from_parts(
        reference: DraftEditHistoryFrontierReferenceV1,
        journal_head: Option<DraftEditHistoryTransitionReferenceV1>,
        undo_head: Option<DraftEditHistoryTransitionReferenceV1>,
        redo_head: Option<DraftEditHistoryTransitionReferenceV1>,
        oldest_eligible: Option<DraftEditHistoryTransitionReferenceV1>,
        cumulative_encoded_bytes: u64,
        retained_encoded_bytes: u64,
        byte_budget: u64,
        retention_policy_revision: u64,
    ) -> Self {
        Self {
            reference,
            journal_head,
            undo_head,
            redo_head,
            oldest_eligible,
            cumulative_encoded_bytes,
            retained_encoded_bytes,
            byte_budget,
            retention_policy_revision,
        }
    }

    pub const fn reference(&self) -> DraftEditHistoryFrontierReferenceV1 {
        self.reference
    }

    pub const fn journal_head(&self) -> Option<DraftEditHistoryTransitionReferenceV1> {
        self.journal_head
    }

    pub const fn undo_head(&self) -> Option<DraftEditHistoryTransitionReferenceV1> {
        self.undo_head
    }

    pub const fn redo_head(&self) -> Option<DraftEditHistoryTransitionReferenceV1> {
        self.redo_head
    }

    pub const fn oldest_eligible(&self) -> Option<DraftEditHistoryTransitionReferenceV1> {
        self.oldest_eligible
    }

    pub const fn cumulative_encoded_bytes(&self) -> u64 {
        self.cumulative_encoded_bytes
    }

    pub const fn retained_encoded_bytes(&self) -> u64 {
        self.retained_encoded_bytes
    }

    pub const fn byte_budget(&self) -> u64 {
        self.byte_budget
    }

    pub const fn retention_policy_revision(&self) -> u64 {
        self.retention_policy_revision
    }

    pub fn fork_session(&self, session_id: DraftEditorCandidateSessionIdV1) -> Option<Self> {
        if !self.is_locally_valid() {
            return None;
        }
        let key =
            DraftEditHistoryFrontierKeyV1::session(self.reference.key().draft_id(), session_id);
        let retained_without_live_head = self
            .retained_encoded_bytes
            .checked_sub(stored_frontier_charge(self).ok()?)?;
        let provisional = Self::from_parts(
            DraftEditHistoryFrontierReferenceV1::new(
                key,
                self.reference.candidate_generation(),
                self.reference.root(),
                self.reference.frontier_revision(),
                self.byte_budget,
                self.retention_policy_revision,
                self.reference.availability(),
                DraftPieceDigestV1::from_bytes([0; 32]),
            ),
            self.journal_head,
            self.undo_head,
            self.redo_head,
            self.oldest_eligible,
            self.cumulative_encoded_bytes,
            0,
            self.byte_budget,
            self.retention_policy_revision,
        );
        let retained_encoded_bytes =
            retained_without_live_head.checked_add(stored_frontier_charge(&provisional).ok()?)?;
        if retained_encoded_bytes > self.byte_budget {
            return None;
        }
        Some(authenticated_frontier(Self::from_parts(
            provisional.reference,
            provisional.journal_head,
            provisional.undo_head,
            provisional.redo_head,
            provisional.oldest_eligible,
            provisional.cumulative_encoded_bytes,
            retained_encoded_bytes,
            provisional.byte_budget,
            provisional.retention_policy_revision,
        )))
    }

    pub(crate) fn is_locally_valid(&self) -> bool {
        let reference = self.reference;
        let key = reference.key();
        if key.draft_id() != reference.root().key().draft_id()
            || self.byte_budget == 0
            || reference.byte_budget() != self.byte_budget
            || reference.retention_policy_revision() != self.retention_policy_revision
            || self.retained_encoded_bytes > self.byte_budget
            || reference.availability().undo_available() != self.undo_head.is_some()
            || reference.availability().redo_available() != self.redo_head.is_some()
            || self.journal_head.is_some() != self.oldest_eligible.is_some()
            || frontier_digest(self) != reference.digest()
        {
            return false;
        }
        let links = [
            self.journal_head,
            self.undo_head,
            self.redo_head,
            self.oldest_eligible,
        ];
        if links.into_iter().flatten().any(|link| {
            link.key().draft_id() != key.draft_id()
                || Some(link.key().session_id()) != key.session_id()
                || link.key().journal_revision() == 0
                || link.cumulative_encoded_bytes() == 0
                || link.cumulative_encoded_bytes() > self.cumulative_encoded_bytes
        }) {
            return false;
        }
        match key {
            DraftEditHistoryFrontierKeyV1::CanonicalEmpty { .. } => {
                reference.candidate_generation() == 0
                    && reference.frontier_revision() == 0
                    && links.into_iter().all(|link| link.is_none())
                    && self.cumulative_encoded_bytes == 0
                    && stored_frontier_charge(self) == Ok(self.retained_encoded_bytes)
                    && reference.availability() == DraftEditHistoryAvailabilityV1::NONE
            }
            DraftEditHistoryFrontierKeyV1::Session { .. } => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftEditHistoryTransitionKindV1 {
    OrdinaryEdit,
    Undo,
    Redo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftEditHistoryTransitionV1 {
    key: DraftEditHistoryTransitionKeyV1,
    predecessor_root: DraftPieceRootReferenceV1,
    successor_root: DraftPieceRootReferenceV1,
    before_caret: DraftCompositePositionV1,
    before_selection: DraftCompositePositionV1,
    after_caret: DraftCompositePositionV1,
    after_selection: DraftCompositePositionV1,
    kind: DraftEditHistoryTransitionKindV1,
    prior_journal: Option<DraftEditHistoryTransitionReferenceV1>,
    prior_undo: Option<DraftEditHistoryTransitionReferenceV1>,
    prior_redo: Option<DraftEditHistoryTransitionReferenceV1>,
    operation_id: DraftPieceOperationIdV1,
    cumulative_encoded_bytes: u64,
    pub(super) digest: DraftPieceDigestV1,
}

impl DraftEditHistoryTransitionV1 {
    #[allow(clippy::too_many_arguments)]
    pub const fn from_parts(
        key: DraftEditHistoryTransitionKeyV1,
        predecessor_root: DraftPieceRootReferenceV1,
        successor_root: DraftPieceRootReferenceV1,
        before_caret: DraftCompositePositionV1,
        before_selection: DraftCompositePositionV1,
        after_caret: DraftCompositePositionV1,
        after_selection: DraftCompositePositionV1,
        kind: DraftEditHistoryTransitionKindV1,
        prior_journal: Option<DraftEditHistoryTransitionReferenceV1>,
        prior_undo: Option<DraftEditHistoryTransitionReferenceV1>,
        prior_redo: Option<DraftEditHistoryTransitionReferenceV1>,
        operation_id: DraftPieceOperationIdV1,
        cumulative_encoded_bytes: u64,
        digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            key,
            predecessor_root,
            successor_root,
            before_caret,
            before_selection,
            after_caret,
            after_selection,
            kind,
            prior_journal,
            prior_undo,
            prior_redo,
            operation_id,
            cumulative_encoded_bytes,
            digest,
        }
    }

    pub const fn key(&self) -> DraftEditHistoryTransitionKeyV1 {
        self.key
    }
    pub const fn predecessor_root(&self) -> DraftPieceRootReferenceV1 {
        self.predecessor_root
    }
    pub const fn successor_root(&self) -> DraftPieceRootReferenceV1 {
        self.successor_root
    }
    pub const fn before_caret(&self) -> DraftCompositePositionV1 {
        self.before_caret
    }
    pub const fn before_selection(&self) -> DraftCompositePositionV1 {
        self.before_selection
    }
    pub const fn after_caret(&self) -> DraftCompositePositionV1 {
        self.after_caret
    }
    pub const fn after_selection(&self) -> DraftCompositePositionV1 {
        self.after_selection
    }
    pub const fn kind(&self) -> DraftEditHistoryTransitionKindV1 {
        self.kind
    }
    pub const fn prior_journal(&self) -> Option<DraftEditHistoryTransitionReferenceV1> {
        self.prior_journal
    }
    pub const fn prior_undo(&self) -> Option<DraftEditHistoryTransitionReferenceV1> {
        self.prior_undo
    }
    pub const fn prior_redo(&self) -> Option<DraftEditHistoryTransitionReferenceV1> {
        self.prior_redo
    }
    pub const fn operation_id(&self) -> DraftPieceOperationIdV1 {
        self.operation_id
    }
    pub const fn cumulative_encoded_bytes(&self) -> u64 {
        self.cumulative_encoded_bytes
    }
    pub const fn digest(&self) -> DraftPieceDigestV1 {
        self.digest
    }

    pub const fn reference(&self) -> DraftEditHistoryTransitionReferenceV1 {
        DraftEditHistoryTransitionReferenceV1::new(
            self.key,
            self.cumulative_encoded_bytes,
            self.digest,
        )
    }

    pub(crate) fn is_locally_valid(&self) -> bool {
        let links = [self.prior_journal, self.prior_undo, self.prior_redo];
        self.key.journal_revision() != 0
            && self.cumulative_encoded_bytes != 0
            && self.predecessor_root.key().draft_id() == self.key.draft_id()
            && self.successor_root.key().draft_id() == self.key.draft_id()
            && links.into_iter().flatten().all(|link| {
                link.key().draft_id() == self.key.draft_id()
                    && link.key().session_id() == self.key.session_id()
                    && link.key().journal_revision() < self.key.journal_revision()
                    && link.cumulative_encoded_bytes() < self.cumulative_encoded_bytes
            })
            && transition_digest(self) == self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftEditHistoryAppendErrorV1 {
    GenerationOverflow,
    FrontierRevisionOverflow,
    CumulativePositionOverflow,
    RetainedSizeOverflow,
    EncodedSizeOverflow,
    BudgetExhausted,
    InvalidFrontier,
}

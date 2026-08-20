use beryl_model::SyndicDraftId;
use std::num::NonZeroU64;

use super::super::{
    DraftEditorCandidateSessionIdV1, DraftPieceDigestV1, DraftPieceRootReferenceV1,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditHistoryPolicyV1 {
    byte_budget: NonZeroU64,
    revision: NonZeroU64,
}

impl DraftEditHistoryPolicyV1 {
    pub const fn new(byte_budget: u64, revision: u64) -> Option<Self> {
        let Some(byte_budget) = NonZeroU64::new(byte_budget) else {
            return None;
        };
        let Some(revision) = NonZeroU64::new(revision) else {
            return None;
        };
        Some(Self {
            byte_budget,
            revision,
        })
    }

    pub const fn byte_budget(self) -> u64 {
        self.byte_budget.get()
    }

    pub const fn revision(self) -> u64 {
        self.revision.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftEditHistoryFrontierKeyV1 {
    CanonicalEmpty {
        draft_id: SyndicDraftId,
    },
    Session {
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
    },
}

impl DraftEditHistoryFrontierKeyV1 {
    pub const fn canonical_empty(draft_id: SyndicDraftId) -> Self {
        Self::CanonicalEmpty { draft_id }
    }

    pub const fn session(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
    ) -> Self {
        Self::Session {
            draft_id,
            session_id,
        }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        match self {
            Self::CanonicalEmpty { draft_id } | Self::Session { draft_id, .. } => draft_id,
        }
    }

    pub const fn session_id(self) -> Option<DraftEditorCandidateSessionIdV1> {
        match self {
            Self::CanonicalEmpty { .. } => None,
            Self::Session { session_id, .. } => Some(session_id),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditHistoryAvailabilityV1 {
    undo_available: bool,
    redo_available: bool,
}

impl DraftEditHistoryAvailabilityV1 {
    pub const NONE: Self = Self::new(false, false);

    pub const fn new(undo_available: bool, redo_available: bool) -> Self {
        Self {
            undo_available,
            redo_available,
        }
    }

    pub const fn undo_available(self) -> bool {
        self.undo_available
    }

    pub const fn redo_available(self) -> bool {
        self.redo_available
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditHistoryTransitionKeyV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    journal_revision: u64,
}

impl DraftEditHistoryTransitionKeyV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        journal_revision: u64,
    ) -> Self {
        Self {
            draft_id,
            session_id,
            journal_revision,
        }
    }

    pub const fn draft_id(self) -> SyndicDraftId {
        self.draft_id
    }

    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }

    pub const fn journal_revision(self) -> u64 {
        self.journal_revision
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditHistoryTransitionReferenceV1 {
    key: DraftEditHistoryTransitionKeyV1,
    cumulative_encoded_bytes: u64,
    digest: DraftPieceDigestV1,
}

impl DraftEditHistoryTransitionReferenceV1 {
    pub const fn new(
        key: DraftEditHistoryTransitionKeyV1,
        cumulative_encoded_bytes: u64,
        digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            key,
            cumulative_encoded_bytes,
            digest,
        }
    }

    pub const fn key(self) -> DraftEditHistoryTransitionKeyV1 {
        self.key
    }

    pub const fn cumulative_encoded_bytes(self) -> u64 {
        self.cumulative_encoded_bytes
    }

    pub const fn digest(self) -> DraftPieceDigestV1 {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftEditHistoryFrontierReferenceV1 {
    key: DraftEditHistoryFrontierKeyV1,
    candidate_generation: u64,
    root: DraftPieceRootReferenceV1,
    frontier_revision: u64,
    byte_budget: u64,
    retention_policy_revision: u64,
    availability: DraftEditHistoryAvailabilityV1,
    pub(super) digest: DraftPieceDigestV1,
}

impl DraftEditHistoryFrontierReferenceV1 {
    pub const fn new(
        key: DraftEditHistoryFrontierKeyV1,
        candidate_generation: u64,
        root: DraftPieceRootReferenceV1,
        frontier_revision: u64,
        byte_budget: u64,
        retention_policy_revision: u64,
        availability: DraftEditHistoryAvailabilityV1,
        digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            key,
            candidate_generation,
            root,
            frontier_revision,
            byte_budget,
            retention_policy_revision,
            availability,
            digest,
        }
    }

    pub const fn key(self) -> DraftEditHistoryFrontierKeyV1 {
        self.key
    }

    pub const fn candidate_generation(self) -> u64 {
        self.candidate_generation
    }

    pub const fn root(self) -> DraftPieceRootReferenceV1 {
        self.root
    }

    pub const fn frontier_revision(self) -> u64 {
        self.frontier_revision
    }

    pub const fn byte_budget(self) -> u64 {
        self.byte_budget
    }

    pub const fn retention_policy_revision(self) -> u64 {
        self.retention_policy_revision
    }

    pub const fn availability(self) -> DraftEditHistoryAvailabilityV1 {
        self.availability
    }

    pub const fn digest(self) -> DraftPieceDigestV1 {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftRootHistoryPairV1 {
    root: DraftPieceRootReferenceV1,
    history: DraftEditHistoryFrontierReferenceV1,
}

impl DraftRootHistoryPairV1 {
    pub const fn new(
        root: DraftPieceRootReferenceV1,
        history: DraftEditHistoryFrontierReferenceV1,
    ) -> Self {
        Self { root, history }
    }

    pub const fn root(self) -> DraftPieceRootReferenceV1 {
        self.root
    }

    pub const fn history(self) -> DraftEditHistoryFrontierReferenceV1 {
        self.history
    }

    pub(crate) fn is_coherent(self) -> bool {
        self.root == self.history.root()
            && self.root.key().draft_id() == self.history.key().draft_id()
    }
}

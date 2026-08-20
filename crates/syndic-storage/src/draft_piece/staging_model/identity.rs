use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMutationOperationIdV1([u8; 16]);

impl DraftMutationOperationIdV1 {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub const fn as_piece_operation(self) -> DraftPieceOperationIdV1 {
        DraftPieceOperationIdV1::from_bytes(self.0)
    }
}

impl From<DraftPieceOperationIdV1> for DraftMutationOperationIdV1 {
    fn from(value: DraftPieceOperationIdV1) -> Self {
        Self::from_bytes(*value.as_bytes())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMutationStagingIdentityV1 {
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
    operation_id: DraftMutationOperationIdV1,
}

impl DraftMutationStagingIdentityV1 {
    pub const fn new(
        draft_id: SyndicDraftId,
        session_id: DraftEditorCandidateSessionIdV1,
        operation_id: DraftMutationOperationIdV1,
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

    pub const fn operation_id(self) -> DraftMutationOperationIdV1 {
        self.operation_id
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMutationStagingEncodingVersionV1;

impl DraftMutationStagingEncodingVersionV1 {
    pub const VALUE: u8 = 1;
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DraftMutationStagingLaneV1 {
    Source,
    Proposal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMutationStagingLaneFrontierV1 {
    next_cursor: u64,
    next_ordinal: u64,
    item_total: u64,
    canonical_byte_total: u64,
    cumulative_identity: DraftPieceDigestV1,
}

impl DraftMutationStagingLaneFrontierV1 {
    pub const fn new(
        next_cursor: u64,
        next_ordinal: u64,
        item_total: u64,
        canonical_byte_total: u64,
        cumulative_identity: DraftPieceDigestV1,
    ) -> Option<Self> {
        if next_ordinal == 0 {
            return None;
        }
        Some(Self {
            next_cursor,
            next_ordinal,
            item_total,
            canonical_byte_total,
            cumulative_identity,
        })
    }

    pub const fn next_cursor(self) -> u64 {
        self.next_cursor
    }

    pub const fn next_ordinal(self) -> u64 {
        self.next_ordinal
    }

    pub const fn item_total(self) -> u64 {
        self.item_total
    }

    pub const fn canonical_byte_total(self) -> u64 {
        self.canonical_byte_total
    }

    pub const fn cumulative_identity(self) -> DraftPieceDigestV1 {
        self.cumulative_identity
    }
}

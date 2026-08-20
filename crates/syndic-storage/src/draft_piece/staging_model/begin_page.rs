use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMutationBeginV1 {
    identity: DraftMutationStagingIdentityV1,
    session_generation: u64,
    predecessor_candidate_generation: u64,
    predecessor_root: DraftPieceRootReferenceV1,
    predecessor_history: DraftEditHistoryFrontierReferenceV1,
    predecessor_extent: DraftLogicalExtentV1,
    predecessor_caret: DraftCompositePositionV1,
    predecessor_selection_anchor: DraftCompositePositionV1,
    predecessor_selection_head: DraftCompositePositionV1,
    replacement_start: DraftCompositePositionV1,
    replacement_end: DraftCompositePositionV1,
    source_initial_cursor: u64,
    proposal_initial_cursor: u64,
}

impl DraftMutationBeginV1 {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        identity: DraftMutationStagingIdentityV1,
        session_generation: u64,
        predecessor_candidate_generation: u64,
        predecessor_root: DraftPieceRootReferenceV1,
        predecessor_history: DraftEditHistoryFrontierReferenceV1,
        predecessor_extent: DraftLogicalExtentV1,
        predecessor_caret: DraftCompositePositionV1,
        predecessor_selection_anchor: DraftCompositePositionV1,
        predecessor_selection_head: DraftCompositePositionV1,
        replacement_start: DraftCompositePositionV1,
        replacement_end: DraftCompositePositionV1,
        source_initial_cursor: u64,
        proposal_initial_cursor: u64,
    ) -> Self {
        Self {
            identity,
            session_generation,
            predecessor_candidate_generation,
            predecessor_root,
            predecessor_history,
            predecessor_extent,
            predecessor_caret,
            predecessor_selection_anchor,
            predecessor_selection_head,
            replacement_start,
            replacement_end,
            source_initial_cursor,
            proposal_initial_cursor,
        }
    }

    pub const fn identity(self) -> DraftMutationStagingIdentityV1 {
        self.identity
    }
    pub const fn session_generation(self) -> u64 {
        self.session_generation
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
    pub const fn predecessor_extent(self) -> DraftLogicalExtentV1 {
        self.predecessor_extent
    }
    pub const fn predecessor_caret(self) -> DraftCompositePositionV1 {
        self.predecessor_caret
    }
    pub const fn predecessor_selection_anchor(self) -> DraftCompositePositionV1 {
        self.predecessor_selection_anchor
    }
    pub const fn predecessor_selection_head(self) -> DraftCompositePositionV1 {
        self.predecessor_selection_head
    }
    pub const fn replacement_start(self) -> DraftCompositePositionV1 {
        self.replacement_start
    }
    pub const fn replacement_end(self) -> DraftCompositePositionV1 {
        self.replacement_end
    }
    pub const fn source_initial_cursor(self) -> u64 {
        self.source_initial_cursor
    }
    pub const fn proposal_initial_cursor(self) -> u64 {
        self.proposal_initial_cursor
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingPageItemV1 {
    SourcePosition(DraftCompositePositionV1),
    Proposal(DraftPieceReplacementV1),
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMutationStagingPageKeyV1 {
    identity: DraftMutationStagingIdentityV1,
    lane: DraftMutationStagingLaneV1,
    ordinal: u64,
}

impl DraftMutationStagingPageKeyV1 {
    pub const fn new(
        identity: DraftMutationStagingIdentityV1,
        lane: DraftMutationStagingLaneV1,
        ordinal: u64,
    ) -> Option<Self> {
        if ordinal == 0 {
            return None;
        }
        Some(Self {
            identity,
            lane,
            ordinal,
        })
    }

    pub const fn identity(self) -> DraftMutationStagingIdentityV1 {
        self.identity
    }
    pub const fn lane(self) -> DraftMutationStagingLaneV1 {
        self.lane
    }
    pub const fn ordinal(self) -> u64 {
        self.ordinal
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMutationStagingPageV1 {
    key: DraftMutationStagingPageKeyV1,
    transition_ordinal: u64,
    input_cursor: u64,
    successor_cursor: u64,
    item_ceiling: u16,
    byte_ceiling: u32,
    prior_cumulative_identity: DraftPieceDigestV1,
    successor_cumulative_identity: DraftPieceDigestV1,
    cumulative_item_total: u64,
    cumulative_byte_total: u64,
    items: Box<[DraftMutationStagingPageItemV1]>,
    digest: DraftPieceDigestV1,
}

impl DraftMutationStagingPageV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        key: DraftMutationStagingPageKeyV1,
        transition_ordinal: u64,
        input_cursor: u64,
        successor_cursor: u64,
        item_ceiling: u16,
        byte_ceiling: u32,
        prior_cumulative_identity: DraftPieceDigestV1,
        successor_cumulative_identity: DraftPieceDigestV1,
        cumulative_item_total: u64,
        cumulative_byte_total: u64,
        items: Box<[DraftMutationStagingPageItemV1]>,
        digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            key,
            transition_ordinal,
            input_cursor,
            successor_cursor,
            item_ceiling,
            byte_ceiling,
            prior_cumulative_identity,
            successor_cumulative_identity,
            cumulative_item_total,
            cumulative_byte_total,
            items,
            digest,
        }
    }

    pub const fn key(&self) -> DraftMutationStagingPageKeyV1 {
        self.key
    }
    pub const fn transition_ordinal(&self) -> u64 {
        self.transition_ordinal
    }
    pub const fn input_cursor(&self) -> u64 {
        self.input_cursor
    }
    pub const fn successor_cursor(&self) -> u64 {
        self.successor_cursor
    }
    pub const fn item_ceiling(&self) -> u16 {
        self.item_ceiling
    }
    pub const fn byte_ceiling(&self) -> u32 {
        self.byte_ceiling
    }
    pub const fn prior_cumulative_identity(&self) -> DraftPieceDigestV1 {
        self.prior_cumulative_identity
    }
    pub const fn successor_cumulative_identity(&self) -> DraftPieceDigestV1 {
        self.successor_cumulative_identity
    }
    pub const fn cumulative_item_total(&self) -> u64 {
        self.cumulative_item_total
    }
    pub const fn cumulative_byte_total(&self) -> u64 {
        self.cumulative_byte_total
    }
    pub fn items(&self) -> &[DraftMutationStagingPageItemV1] {
        &self.items
    }
    pub const fn digest(&self) -> DraftPieceDigestV1 {
        self.digest
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMutationFinishInputV1 {
    source: DraftMutationStagingLaneFrontierV1,
    proposal: DraftMutationStagingLaneFrontierV1,
    intended_extent: DraftLogicalExtentV1,
    intended_caret: DraftCompositePositionV1,
    intended_selection_anchor: DraftCompositePositionV1,
    intended_selection_head: DraftCompositePositionV1,
    proposal_fragment_chain: DraftPieceDigestV1,
}

impl DraftMutationFinishInputV1 {
    pub const fn new(
        source: DraftMutationStagingLaneFrontierV1,
        proposal: DraftMutationStagingLaneFrontierV1,
        intended_extent: DraftLogicalExtentV1,
        intended_caret: DraftCompositePositionV1,
        intended_selection_anchor: DraftCompositePositionV1,
        intended_selection_head: DraftCompositePositionV1,
        proposal_fragment_chain: DraftPieceDigestV1,
    ) -> Self {
        Self {
            source,
            proposal,
            intended_extent,
            intended_caret,
            intended_selection_anchor,
            intended_selection_head,
            proposal_fragment_chain,
        }
    }

    pub const fn source(self) -> DraftMutationStagingLaneFrontierV1 {
        self.source
    }
    pub const fn proposal(self) -> DraftMutationStagingLaneFrontierV1 {
        self.proposal
    }
    pub const fn intended_extent(self) -> DraftLogicalExtentV1 {
        self.intended_extent
    }
    pub const fn intended_caret(self) -> DraftCompositePositionV1 {
        self.intended_caret
    }
    pub const fn intended_selection_anchor(self) -> DraftCompositePositionV1 {
        self.intended_selection_anchor
    }
    pub const fn intended_selection_head(self) -> DraftCompositePositionV1 {
        self.intended_selection_head
    }
    pub const fn proposal_fragment_chain(self) -> DraftPieceDigestV1 {
        self.proposal_fragment_chain
    }
}

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingLifecycleV1 {
    Receiving,
    Finished(DraftMutationFinishInputV1),
    Building(DraftPieceBuildProgressReceiptReferenceV1),
    Cancelled,
    Rejected,
    Conflict,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMutationStagingProgressReceiptReferenceV1 {
    identity: DraftMutationStagingIdentityV1,
    transition_ordinal: u64,
    digest: DraftPieceDigestV1,
}

impl DraftMutationStagingProgressReceiptReferenceV1 {
    pub const fn new(
        identity: DraftMutationStagingIdentityV1,
        transition_ordinal: u64,
        digest: DraftPieceDigestV1,
    ) -> Option<Self> {
        if transition_ordinal == 0 {
            return None;
        }
        Some(Self {
            identity,
            transition_ordinal,
            digest,
        })
    }
    pub const fn identity(self) -> DraftMutationStagingIdentityV1 {
        self.identity
    }
    pub const fn transition_ordinal(self) -> u64 {
        self.transition_ordinal
    }
    pub const fn digest(self) -> DraftPieceDigestV1 {
        self.digest
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMutationStagingHeadV1 {
    identity: DraftMutationStagingIdentityV1,
    begin: DraftMutationBeginV1,
    begin_digest: DraftPieceDigestV1,
    source: DraftMutationStagingLaneFrontierV1,
    proposal: DraftMutationStagingLaneFrontierV1,
    receipt: DraftMutationStagingProgressReceiptReferenceV1,
    lifecycle: DraftMutationStagingLifecycleV1,
    digest: DraftPieceDigestV1,
}

impl DraftMutationStagingHeadV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        identity: DraftMutationStagingIdentityV1,
        begin: DraftMutationBeginV1,
        begin_digest: DraftPieceDigestV1,
        source: DraftMutationStagingLaneFrontierV1,
        proposal: DraftMutationStagingLaneFrontierV1,
        receipt: DraftMutationStagingProgressReceiptReferenceV1,
        lifecycle: DraftMutationStagingLifecycleV1,
        digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            identity,
            begin,
            begin_digest,
            source,
            proposal,
            receipt,
            lifecycle,
            digest,
        }
    }
    pub const fn identity(&self) -> DraftMutationStagingIdentityV1 {
        self.identity
    }
    pub const fn begin(&self) -> DraftMutationBeginV1 {
        self.begin
    }
    pub const fn begin_digest(&self) -> DraftPieceDigestV1 {
        self.begin_digest
    }
    pub const fn source(&self) -> DraftMutationStagingLaneFrontierV1 {
        self.source
    }
    pub const fn proposal(&self) -> DraftMutationStagingLaneFrontierV1 {
        self.proposal
    }
    pub const fn receipt(&self) -> DraftMutationStagingProgressReceiptReferenceV1 {
        self.receipt
    }
    pub const fn lifecycle(&self) -> DraftMutationStagingLifecycleV1 {
        self.lifecycle
    }
    pub const fn digest(&self) -> DraftPieceDigestV1 {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingCommandKindV1 {
    Begin,
    SourcePage,
    ProposalPage,
    Finish,
    Transfer,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingCustodyTagV1 {
    None,
    Staging,
    Building,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingRejectedReasonV1 {
    InvalidEnvelope,
    InvalidPage,
    InvalidFinish,
    EmptyProposal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingErrorReasonV1 {
    Operational,
    Overflow,
    Corruption,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingTerminalAnchorV1 {
    Begin(DraftMutationStagingIdentityV1),
    Page(DraftMutationStagingPageKeyV1),
    Finish(DraftMutationStagingIdentityV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingOccupiedKeyV1 {
    Head(DraftMutationStagingIdentityV1),
    Page(DraftMutationStagingPageKeyV1),
    Progress(DraftMutationStagingProgressReceiptKeyV1),
    Build(DraftPieceSettlementKeyV1),
    Settlement(DraftPieceSettlementKeyV1),
    CandidateRoot(DraftPieceRootKeyV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingComparedByteV1 {
    Byte(u8),
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingErrorEvidenceV1 {
    Operational {
        reason: DraftMutationStagingErrorReasonV1,
        anchor: DraftMutationStagingTerminalAnchorV1,
    },
    OccupiedIdentity {
        key: DraftMutationStagingOccupiedKeyV1,
        stored_digest: DraftPieceDigestV1,
        requested_digest: DraftPieceDigestV1,
        first_difference: u64,
        stored: DraftMutationStagingComparedByteV1,
        requested: DraftMutationStagingComparedByteV1,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingTerminalEvidenceV1 {
    Rejected {
        reason: DraftMutationStagingRejectedReasonV1,
        anchor: DraftMutationStagingTerminalAnchorV1,
        digest: DraftPieceDigestV1,
        candidate_generation: u64,
        root: DraftPieceRootReferenceV1,
        history: DraftEditHistoryFrontierReferenceV1,
        session_revision: u64,
    },
    Conflict {
        expected_generation: u64,
        expected_root: DraftPieceRootReferenceV1,
        expected_history: DraftEditHistoryFrontierReferenceV1,
        observed_generation: u64,
        observed_root: DraftPieceRootReferenceV1,
        observed_history: DraftEditHistoryFrontierReferenceV1,
        session_revision: u64,
    },
    Cancelled {
        request_id: DraftMutationOperationIdV1,
        source_lifecycle: DraftMutationStagingLifecycleV1,
        writer_admitted: bool,
        candidate_generation: u64,
        root: DraftPieceRootReferenceV1,
        history: DraftEditHistoryFrontierReferenceV1,
        session_revision: u64,
    },
    Error {
        error: DraftMutationStagingErrorEvidenceV1,
        candidate_generation: u64,
        root: DraftPieceRootReferenceV1,
        history: DraftEditHistoryFrontierReferenceV1,
        session_revision: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftMutationStagingProgressReceiptKeyV1 {
    identity: DraftMutationStagingIdentityV1,
    transition_ordinal: u64,
}

impl DraftMutationStagingProgressReceiptKeyV1 {
    pub const fn new(
        identity: DraftMutationStagingIdentityV1,
        transition_ordinal: u64,
    ) -> Option<Self> {
        if transition_ordinal == 0 {
            return None;
        }
        Some(Self {
            identity,
            transition_ordinal,
        })
    }
    pub const fn identity(self) -> DraftMutationStagingIdentityV1 {
        self.identity
    }
    pub const fn transition_ordinal(self) -> u64 {
        self.transition_ordinal
    }
}

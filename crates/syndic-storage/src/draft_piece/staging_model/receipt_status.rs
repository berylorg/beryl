use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftMutationStagingProgressReceiptV1 {
    key: DraftMutationStagingProgressReceiptKeyV1,
    prior: Option<DraftMutationStagingProgressReceiptReferenceV1>,
    command: DraftMutationStagingCommandKindV1,
    page: Option<(DraftMutationStagingPageKeyV1, DraftPieceDigestV1)>,
    finish_digest: Option<DraftPieceDigestV1>,
    before_source: DraftMutationStagingLaneFrontierV1,
    after_source: DraftMutationStagingLaneFrontierV1,
    before_proposal: DraftMutationStagingLaneFrontierV1,
    after_proposal: DraftMutationStagingLaneFrontierV1,
    before_head_digest: Option<DraftPieceDigestV1>,
    after_head_digest: DraftPieceDigestV1,
    before_lifecycle: Option<DraftMutationStagingLifecycleV1>,
    after_lifecycle: DraftMutationStagingLifecycleV1,
    custody_before: DraftMutationStagingCustodyTagV1,
    custody_after: DraftMutationStagingCustodyTagV1,
    build_endpoint: Option<DraftPieceBuildProgressReceiptReferenceV1>,
    terminal_evidence: Option<DraftMutationStagingTerminalEvidenceV1>,
    digest: DraftPieceDigestV1,
}

impl DraftMutationStagingProgressReceiptV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn from_parts(
        key: DraftMutationStagingProgressReceiptKeyV1,
        prior: Option<DraftMutationStagingProgressReceiptReferenceV1>,
        command: DraftMutationStagingCommandKindV1,
        page: Option<(DraftMutationStagingPageKeyV1, DraftPieceDigestV1)>,
        finish_digest: Option<DraftPieceDigestV1>,
        before_source: DraftMutationStagingLaneFrontierV1,
        after_source: DraftMutationStagingLaneFrontierV1,
        before_proposal: DraftMutationStagingLaneFrontierV1,
        after_proposal: DraftMutationStagingLaneFrontierV1,
        before_head_digest: Option<DraftPieceDigestV1>,
        after_head_digest: DraftPieceDigestV1,
        before_lifecycle: Option<DraftMutationStagingLifecycleV1>,
        after_lifecycle: DraftMutationStagingLifecycleV1,
        custody_before: DraftMutationStagingCustodyTagV1,
        custody_after: DraftMutationStagingCustodyTagV1,
        build_endpoint: Option<DraftPieceBuildProgressReceiptReferenceV1>,
        terminal_evidence: Option<DraftMutationStagingTerminalEvidenceV1>,
        digest: DraftPieceDigestV1,
    ) -> Self {
        Self {
            key,
            prior,
            command,
            page,
            finish_digest,
            before_source,
            after_source,
            before_proposal,
            after_proposal,
            before_head_digest,
            after_head_digest,
            before_lifecycle,
            after_lifecycle,
            custody_before,
            custody_after,
            build_endpoint,
            terminal_evidence,
            digest,
        }
    }
    pub const fn key(&self) -> DraftMutationStagingProgressReceiptKeyV1 {
        self.key
    }
    pub const fn prior(&self) -> Option<DraftMutationStagingProgressReceiptReferenceV1> {
        self.prior
    }
    pub const fn command(&self) -> DraftMutationStagingCommandKindV1 {
        self.command
    }
    pub const fn page(&self) -> Option<(DraftMutationStagingPageKeyV1, DraftPieceDigestV1)> {
        self.page
    }
    pub const fn finish_digest(&self) -> Option<DraftPieceDigestV1> {
        self.finish_digest
    }
    pub const fn before_source(&self) -> DraftMutationStagingLaneFrontierV1 {
        self.before_source
    }
    pub const fn after_source(&self) -> DraftMutationStagingLaneFrontierV1 {
        self.after_source
    }
    pub const fn before_proposal(&self) -> DraftMutationStagingLaneFrontierV1 {
        self.before_proposal
    }
    pub const fn after_proposal(&self) -> DraftMutationStagingLaneFrontierV1 {
        self.after_proposal
    }
    pub const fn before_head_digest(&self) -> Option<DraftPieceDigestV1> {
        self.before_head_digest
    }
    pub const fn after_head_digest(&self) -> DraftPieceDigestV1 {
        self.after_head_digest
    }
    pub const fn before_lifecycle(&self) -> Option<DraftMutationStagingLifecycleV1> {
        self.before_lifecycle
    }
    pub const fn after_lifecycle(&self) -> DraftMutationStagingLifecycleV1 {
        self.after_lifecycle
    }
    pub const fn custody_before(&self) -> DraftMutationStagingCustodyTagV1 {
        self.custody_before
    }
    pub const fn custody_after(&self) -> DraftMutationStagingCustodyTagV1 {
        self.custody_after
    }
    pub const fn build_endpoint(&self) -> Option<DraftPieceBuildProgressReceiptReferenceV1> {
        self.build_endpoint
    }
    pub const fn terminal_evidence(&self) -> Option<DraftMutationStagingTerminalEvidenceV1> {
        self.terminal_evidence
    }
    pub const fn digest(&self) -> DraftPieceDigestV1 {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingStatusV1 {
    Absent,
    Receiving {
        head: DraftMutationStagingProgressReceiptReferenceV1,
    },
    Finished {
        head: DraftMutationStagingProgressReceiptReferenceV1,
    },
    Building {
        staging: DraftMutationStagingProgressReceiptReferenceV1,
        build: DraftPieceBuildProgressReceiptReferenceV1,
    },
    Cancelled {
        head: DraftMutationStagingProgressReceiptReferenceV1,
        evidence: DraftMutationStagingTerminalEvidenceV1,
    },
    Rejected {
        head: DraftMutationStagingProgressReceiptReferenceV1,
        evidence: DraftMutationStagingTerminalEvidenceV1,
    },
    Conflict {
        head: DraftMutationStagingProgressReceiptReferenceV1,
        evidence: DraftMutationStagingTerminalEvidenceV1,
    },
    Error {
        head: DraftMutationStagingProgressReceiptReferenceV1,
        evidence: DraftMutationStagingTerminalEvidenceV1,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftMutationStagingReconcileV1 {
    SourceSelected,
    TargetSelected,
    Terminal(DraftMutationStagingStatusV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftMutationSourceMarkerV1 {
    marker_id: SyndicDraftMarkerId,
    position: DraftCompositePositionV1,
}

impl DraftMutationSourceMarkerV1 {
    pub const fn new(marker_id: SyndicDraftMarkerId, position: DraftCompositePositionV1) -> Self {
        Self {
            marker_id,
            position,
        }
    }
    pub const fn marker_id(self) -> SyndicDraftMarkerId {
        self.marker_id
    }
    pub const fn position(self) -> DraftCompositePositionV1 {
        self.position
    }
}

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use beryl_home_store::{CommandCancellation, CommandError, CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{ImageLabelOrdinal, SyndicDraftMarkerId};
use gpui_text_input::{
    BindingId, InlineObjectGap, InlineObjectId, MutationFragment, MutationFragmentPayload,
    MutationKind, MutationPositions, MutationProposal, ObjectChange, SourcePosition,
    SourceRevision,
};
use syndic_storage::{
    DRAFT_PIECE_PAGE_MAX_BYTES, DRAFT_PIECE_PAGE_MAX_RECORDS, DraftCompositeGapWitnessV1,
    DraftCompositePositionV1, DraftEditorCandidateActivationBindingV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1,
    DraftPieceMarkerAtV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerEdgeProofRequestV1, DraftPieceMarkerEdgeProofV1, DraftPieceMarkerMoveV1,
    DraftPieceMarkerScopeV1, DraftPieceMarkerV1, DraftPieceOperationStatusV1,
    DraftPiecePrepareErrorV1, DraftPieceReplacementV1, DraftPieceRootReferenceV1,
    DraftPieceSettlementClosureV1, DraftPieceSettlementProofV1, DraftPieceTransactionOutcomeV1,
    DraftPieceV1, PreparedDraftPieceEditV1, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1,
};

use super::request::{candidate_head, validate_store};
use super::{ComposerHostBinding, ComposerHostError, SyndicComposerHost};

pub(super) const COMPOSER_HOST_MAX_MUTATION_TRANSITIONS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostImageMarkerMetadata {
    object_id: InlineObjectId,
    label: ImageLabelOrdinal,
}

impl ComposerHostImageMarkerMetadata {
    pub const fn new(object_id: InlineObjectId, label: ImageLabelOrdinal) -> Self {
        Self { object_id, label }
    }

    pub const fn object_id(self) -> InlineObjectId {
        self.object_id
    }

    pub const fn label(self) -> ImageLabelOrdinal {
        self.label
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposerHostMutationRequest {
    binding: ComposerHostBinding,
    proposal: MutationProposal,
    operation_id: syndic_storage::DraftPieceOperationIdV1,
    fragments: Box<[MutationFragment]>,
    marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ComposerHostMutationIdentity {
    request: ComposerHostMutationRequest,
}

impl ComposerHostMutationIdentity {
    fn new(request: &ComposerHostMutationRequest) -> Self {
        Self {
            request: request.clone(),
        }
    }

    fn operation(&self) -> u64 {
        self.request.proposal.key().operation().get()
    }
}

impl ComposerHostMutationRequest {
    pub fn new(
        binding: ComposerHostBinding,
        proposal: MutationProposal,
        operation_id: syndic_storage::DraftPieceOperationIdV1,
        fragments: Box<[MutationFragment]>,
        marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
    ) -> Self {
        Self {
            binding,
            proposal,
            operation_id,
            fragments,
            marker_metadata,
        }
    }

    pub const fn binding(&self) -> ComposerHostBinding {
        self.binding
    }

    pub const fn proposal(&self) -> MutationProposal {
        self.proposal
    }

    pub const fn operation_id(&self) -> syndic_storage::DraftPieceOperationIdV1 {
        self.operation_id
    }

    pub fn fragments(&self) -> &[MutationFragment] {
        &self.fragments
    }

    pub fn marker_metadata(&self) -> &[ComposerHostImageMarkerMetadata] {
        &self.marker_metadata
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComposerHostMutationOutcome {
    Committed {
        binding: ComposerHostBinding,
        positions: MutationPositions,
    },
    Rejected,
    Conflict,
    Cancelled,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostMutationStatus {
    Staged,
    Admitted,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct ComposerHostRetainedMutationIntent {
    binding: ComposerHostBinding,
    operation_id: syndic_storage::DraftPieceOperationIdV1,
    proposal: MutationProposal,
    replacements: Box<[DraftPieceReplacementV1]>,
    positions: MutationPositions,
    targets: Box<[DraftPieceMarkerAtV1]>,
}

impl ComposerHostRetainedMutationIntent {
    pub const fn binding(&self) -> ComposerHostBinding {
        self.binding
    }

    pub const fn operation_id(&self) -> syndic_storage::DraftPieceOperationIdV1 {
        self.operation_id
    }

    pub const fn proposal(&self) -> MutationProposal {
        self.proposal
    }

    pub fn replacements(&self) -> &[DraftPieceReplacementV1] {
        &self.replacements
    }

    pub const fn positions(&self) -> MutationPositions {
        self.positions
    }

    pub fn targets(&self) -> &[DraftPieceMarkerAtV1] {
        &self.targets
    }
}

pub(super) struct ComposerHostMutationTransaction {
    binding: ComposerHostBinding,
    prepared: PreparedDraftPieceEditV1,
    fragments: Box<[DraftPieceBuildFragmentV1]>,
    positions: MutationPositions,
    successors: Box<[DraftPieceMarkerAtV1]>,
    identity: ComposerHostMutationIdentity,
    intent: ComposerHostRetainedMutationIntent,
}

pub(super) enum ComposerHostPendingMutation {
    Staged(ComposerHostMutationTransaction),
    Admitted(ComposerHostMutationTransaction),
    Unavailable(ComposerHostRetainedMutationIntent),
}

enum MutationCommandResult {
    Pending,
    Terminal(DraftPieceTransactionOutcomeV1),
    CancelledBeforeAdmission,
}

mod execution;
mod settlement;
mod terminal_gap;
mod terminal_transform;

mod translation;
mod validation;

use translation::{canonical_position, translate_request};
use validation::{validate_committed_successor, validate_request_key};

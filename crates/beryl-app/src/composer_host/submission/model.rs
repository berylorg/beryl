use beryl_home_store::{
    CommandCancellation, FreeSpaceOutcome, ReconciliationHandle, TurnStartAdmissionRequirement,
};
use beryl_model::{SyndicDraftId, SyndicItemId};
use syndic_storage::{
    DraftComposerBuildKeyV1, DraftComposerMaterializationOperationIdV1, DraftPieceOperationIdV1,
    FirstAcceptance, FirstAcceptanceKind, SyndicTimestamp,
};

use super::super::{ComposerHostBinding, ComposerHostError, ComposerHostFlushTicket};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostSubmissionRequest {
    next_draft_id: SyndicDraftId,
    idle_user_item_id: SyndicItemId,
    materialization_operation_id: DraftComposerMaterializationOperationIdV1,
    session_disposal_operation_id: DraftPieceOperationIdV1,
    admitted_at: SyndicTimestamp,
    turn_start_admission_requirement: TurnStartAdmissionRequirement,
}

impl ComposerHostSubmissionRequest {
    pub const fn new(
        next_draft_id: SyndicDraftId,
        idle_user_item_id: SyndicItemId,
        materialization_operation_id: DraftComposerMaterializationOperationIdV1,
        session_disposal_operation_id: DraftPieceOperationIdV1,
        admitted_at: SyndicTimestamp,
        turn_start_admission_requirement: TurnStartAdmissionRequirement,
    ) -> Self {
        Self {
            next_draft_id,
            idle_user_item_id,
            materialization_operation_id,
            session_disposal_operation_id,
            admitted_at,
            turn_start_admission_requirement,
        }
    }

    pub const fn next_draft_id(self) -> SyndicDraftId {
        self.next_draft_id
    }

    pub const fn idle_user_item_id(self) -> SyndicItemId {
        self.idle_user_item_id
    }

    pub const fn materialization_operation_id(self) -> DraftComposerMaterializationOperationIdV1 {
        self.materialization_operation_id
    }

    pub const fn session_disposal_operation_id(self) -> DraftPieceOperationIdV1 {
        self.session_disposal_operation_id
    }

    pub const fn admitted_at(self) -> SyndicTimestamp {
        self.admitted_at
    }

    pub const fn turn_start_admission_requirement(self) -> TurnStartAdmissionRequirement {
        self.turn_start_admission_requirement
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostSubmissionTicket {
    pub(super) binding: ComposerHostBinding,
    pub(super) generation: u64,
}

impl ComposerHostSubmissionTicket {
    pub const fn binding(self) -> ComposerHostBinding {
        self.binding
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostSubmissionStage {
    Flushing,
    Capturing,
    Materializing,
    Accepting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostSubmissionAdvance {
    Progress(ComposerHostSubmissionStage),
    ReconciliationPending,
    DirectAdmissionDenied(FreeSpaceOutcome),
    NotCommitted,
    ExactSuccess(FirstAcceptanceKind),
    Collision,
    Cancelled,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComposerHostSubmissionDiagnostics {
    pub(super) pending: bool,
    pub(super) stage: Option<ComposerHostSubmissionStage>,
    pub(super) retained_roots: usize,
    pub(super) retained_materializations: usize,
    pub(super) command_attempted: bool,
}

impl ComposerHostSubmissionDiagnostics {
    pub const fn pending(self) -> bool {
        self.pending
    }

    pub const fn stage(self) -> Option<ComposerHostSubmissionStage> {
        self.stage
    }

    pub const fn retained_roots(self) -> usize {
        self.retained_roots
    }

    pub const fn retained_materializations(self) -> usize {
        self.retained_materializations
    }

    pub const fn command_attempted(self) -> bool {
        self.command_attempted
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ComposerHostSubmissionError {
    #[error(transparent)]
    Host(#[from] ComposerHostError),
    #[error(transparent)]
    Materialization(#[from] syndic_storage::DraftComposerMaterializationErrorV1),
    #[error(transparent)]
    Admission(#[from] crate::input_admission::InputAdmissionBuildError),
    #[error(transparent)]
    HomeRead(#[from] beryl_home_store::ReadError),
    #[error(transparent)]
    CommandBuild(#[from] beryl_home_store::CommandBuildError),
    #[error(transparent)]
    AssetRead(#[from] beryl_state::AssetReadError),
    #[error(transparent)]
    SyndicRead(#[from] syndic_storage::SyndicReadError),
    #[error("the submission root is empty")]
    Empty,
    #[error("the exact published draft asset owner is inconsistent")]
    AssetOwnerConflict,
    #[error("the submission materializer reached a terminal non-sealed state")]
    MaterializationTerminal,
    #[cfg(feature = "test-faults")]
    #[error("injected submission transition fault at {0:?}")]
    InjectedFault(ComposerHostSubmissionFaultPoint),
}

pub(in crate::composer_host) struct ComposerHostSubmissionCoordinator {
    pub(in crate::composer_host) pending: Option<Box<PendingSubmission>>,
    pub(super) generation: u64,
}

impl ComposerHostSubmissionCoordinator {
    pub(in crate::composer_host) const fn new() -> Self {
        Self {
            pending: None,
            generation: 0,
        }
    }
}

pub(in crate::composer_host) struct PendingSubmission {
    pub(super) ticket: ComposerHostSubmissionTicket,
    pub(super) request: ComposerHostSubmissionRequest,
    pub(super) cancellation: Option<CommandCancellation>,
    pub(super) stage: PendingSubmissionStage,
}

#[derive(Clone)]
pub(super) enum PendingSubmissionStage {
    Flushing(ComposerHostFlushTicket),
    Capturing,
    Materializing {
        captured: CapturedSubmission,
        reconciliation: Option<ReconciliationHandle>,
    },
    Accepting {
        acceptance: FirstAcceptance,
        reconciliation: Option<ReconciliationHandle>,
    },
    Transitioning,
}

#[derive(Clone)]
pub(super) struct CapturedSubmission {
    pub(super) thread_id: beryl_model::SyndicThreadId,
    pub(super) candidate: syndic_storage::DraftEditorCandidateActivationBindingV1,
    pub(super) thread_revision: beryl_model::ThreadRevision,
    pub(super) draft_revision: beryl_model::DraftRevision,
    pub(super) gate_revision: beryl_model::InputGateRevision,
    pub(super) gate_state: syndic_storage::InputGateState,
    pub(super) asset_reference_set: Option<beryl_model::SealedAssetReferenceSetProof>,
    pub(super) build: DraftComposerBuildKeyV1,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComposerHostSubmissionFaultPoint {
    Flush,
    Materializer,
    AcceptanceBeforeAttempt,
    CancellationAfterFreeSpace,
    CancellationBeforeFinalCommand,
    AcceptanceAfterAttempt,
}

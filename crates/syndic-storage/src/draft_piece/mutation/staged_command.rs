use std::sync::{Arc, Mutex};

use beryl_home_store::{
    CommandError, CommitReceipt, CommittedLocalFinalizationError, HomeGeneration,
    ReconciliationFailure, ReconciliationHandle,
};

use super::*;
use crate::draft_piece::staging::{StageDurableWindowMutation, TransferMutation};

mod capture;
mod cleanup;
mod preparation;
mod submission;
mod verification;

pub const STAGED_DRAFT_PIECE_OUTCOME_MAX_READS: usize = 128;
pub const STAGED_DRAFT_PIECE_OUTCOME_MAX_ENCODED_VALUE_BYTES: usize = 8_388_608;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedDraftPieceCommandKindV1 {
    Transfer,
    Window,
    Advance,
    Terminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedDraftPieceTerminalElectionV1 {
    Settle,
    Cancel,
    Reject(DraftPieceRejectedReasonV1),
    Error(DraftPieceErrorReasonV1),
}

#[derive(Debug, thiserror::Error)]
pub enum StagedDraftPiecePreparationErrorV1 {
    #[error("staged build preparation failed: {0}")]
    Build(#[from] DraftPiecePrepareErrorV1),
    #[error("durable staging preparation failed: {0}")]
    Staging(#[from] DraftMutationStagingErrorV1),
    #[error("staged build authority read failed: {0}")]
    Read(#[from] crate::SyndicReadError),
    #[error("staged build generation or attachment is unavailable")]
    Unavailable,
    #[error("staged build endpoint is stale or belongs to another operation")]
    StaleEndpoint,
}

#[derive(Debug, thiserror::Error)]
pub enum StagedDraftPieceOutcomeErrorV1 {
    #[error("staged build authority read failed: {0}")]
    Read(#[from] crate::SyndicReadError),
    #[error("staged build reconciliation failed: {0}")]
    Reconciliation(#[from] ReconciliationFailure),
    #[error("staged build local finalization failed: {0}")]
    LocalFinalization(CommittedLocalFinalizationError),
    #[error("staged build generation or attachment is unavailable")]
    Unavailable,
    #[error("staged build endpoint changed")]
    StaleEndpoint,
    #[error("staged build anchors changed during verification")]
    ConcurrentChange,
    #[error("staged build referenced closure disagrees: {0}")]
    Invariant(&'static str),
    #[error("staged build local writer custody is unavailable")]
    LocalCustody,
    #[error("staged build referenced closure exceeds its verification budget")]
    VerificationLimit,
    #[error("staged build serialized result is unavailable")]
    MissingCapture,
    #[error("staged build reconciliation selected a later successor")]
    ExactSuccessor,
    #[error("staged build reconciliation found a collision")]
    Collision,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StagedDraftPieceVerificationWorkV1 {
    pub attempted_reads: usize,
    pub charged_encoded_value_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedDraftPieceDurableClassificationV1 {
    Unresolved,
    NotCommitted,
    Committed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedDraftPieceOutcomeStateV1 {
    NotCommitted,
    Reconciling,
    Verifying,
    Finalizing,
    CleanupPending,
    CleanupReconciling,
    CleanupVerifying,
    CleanupFinalizing,
    Complete,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StagedDraftPieceLocalFinalizationV1 {
    NotRequired,
    Consumed,
    Failed,
}

#[must_use]
pub struct PreparedStagedDraftPieceCommandV1 {
    storage: SyndicStorage,
    source: Box<CapturedState>,
    command: CommandKind,
}

impl PreparedStagedDraftPieceCommandV1 {
    pub fn kind(&self) -> StagedDraftPieceCommandKindV1 {
        self.command.kind()
    }

    pub fn identity(&self) -> DraftMutationStagingIdentityV1 {
        self.source.staging.identity()
    }

    pub fn source_endpoint(&self) -> Option<DraftPieceBuildProgressReceiptReferenceV1> {
        self.source
            .build
            .as_ref()
            .map(DraftPieceBuildRecordV1::progress_receipt)
    }
}

impl std::fmt::Debug for PreparedStagedDraftPieceCommandV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedStagedDraftPieceCommandV1")
            .field("kind", &self.kind())
            .field("identity", &self.identity())
            .field("source_endpoint", &self.source_endpoint())
            .finish_non_exhaustive()
    }
}

#[must_use]
pub struct StagedDraftPieceOutcomeFlightV1 {
    command: Box<PreparedStagedDraftPieceCommandV1>,
    capture: Option<Box<CapturedCommand>>,
    phase: FlightPhase,
    classification: StagedDraftPieceDurableClassificationV1,
    receipt: Option<CommitReceipt>,
    original_failure: Option<CommandError>,
    later_failure: Option<CommandError>,
    cleanup_failure: Option<CommandError>,
    cleanup_receipt: Option<CommitReceipt>,
    cleanup_local_finalization: StagedDraftPieceLocalFinalizationV1,
    failure: Option<StagedDraftPieceOutcomeErrorV1>,
    local_finalization: StagedDraftPieceLocalFinalizationV1,
    verification: StagedDraftPieceVerificationWorkV1,
    inert_cleanup: Option<StagedDraftPieceInertCleanupV1>,
    result: Option<Box<DraftPieceReconciledCommandV1>>,
}

impl StagedDraftPieceOutcomeFlightV1 {
    pub fn state(&self) -> StagedDraftPieceOutcomeStateV1 {
        match self.phase {
            FlightPhase::NotCommitted => StagedDraftPieceOutcomeStateV1::NotCommitted,
            FlightPhase::Reconciling { .. } => StagedDraftPieceOutcomeStateV1::Reconciling,
            FlightPhase::Verifying(_) => StagedDraftPieceOutcomeStateV1::Verifying,
            FlightPhase::Finalizing(_) => StagedDraftPieceOutcomeStateV1::Finalizing,
            FlightPhase::CleanupPending => StagedDraftPieceOutcomeStateV1::CleanupPending,
            FlightPhase::CleanupReconciling { .. } => {
                StagedDraftPieceOutcomeStateV1::CleanupReconciling
            }
            FlightPhase::CleanupVerifying => StagedDraftPieceOutcomeStateV1::CleanupVerifying,
            FlightPhase::CleanupFinalizing => StagedDraftPieceOutcomeStateV1::CleanupFinalizing,
            FlightPhase::Complete => StagedDraftPieceOutcomeStateV1::Complete,
            FlightPhase::Unavailable(_) => StagedDraftPieceOutcomeStateV1::Unavailable,
        }
    }

    pub fn classification(&self) -> StagedDraftPieceDurableClassificationV1 {
        self.classification
    }
    pub fn result(&self) -> Option<&DraftPieceReconciledCommandV1> {
        self.result.as_deref()
    }
    pub fn receipt(&self) -> Option<&CommitReceipt> {
        self.receipt.as_ref()
    }
    pub fn original_failure(&self) -> Option<&CommandError> {
        self.original_failure.as_ref()
    }
    pub fn later_failure(&self) -> Option<&CommandError> {
        self.later_failure.as_ref()
    }
    pub fn cleanup_failure(&self) -> Option<&CommandError> {
        self.cleanup_failure.as_ref()
    }
    pub fn cleanup_receipt(&self) -> Option<&CommitReceipt> {
        self.cleanup_receipt.as_ref()
    }
    pub fn cleanup_local_finalization(&self) -> StagedDraftPieceLocalFinalizationV1 {
        self.cleanup_local_finalization
    }
    pub fn failure(&self) -> Option<&StagedDraftPieceOutcomeErrorV1> {
        self.failure.as_ref()
    }
    pub fn local_finalization(&self) -> StagedDraftPieceLocalFinalizationV1 {
        self.local_finalization
    }
    pub fn verification_work(&self) -> StagedDraftPieceVerificationWorkV1 {
        self.verification
    }
    pub fn has_reconciliation_custody(&self) -> bool {
        phase_has_reconciliation(&self.phase)
    }

    pub fn into_completion(self) -> Result<StagedDraftPieceCommandCompletionV1, Self> {
        if !matches!(self.phase, FlightPhase::Complete)
            || self.result.is_none()
            || self.receipt.is_none()
        {
            return Err(self);
        }
        Ok(StagedDraftPieceCommandCompletionV1 {
            result: *self.result.expect("complete command has a captured result"),
            receipt: self
                .receipt
                .expect("complete command has its exact receipt"),
            original_failure: self.original_failure,
            later_failure: self.later_failure,
            cleanup_failure: self.cleanup_failure,
            cleanup_receipt: self.cleanup_receipt,
            local_finalization: self.local_finalization,
            cleanup_local_finalization: self.cleanup_local_finalization,
            verification: self.verification,
            inert_cleanup: self.inert_cleanup,
        })
    }

    pub fn into_noncommit(self) -> Result<StagedDraftPieceCommandNoncommitV1, Self> {
        if !matches!(self.phase, FlightPhase::NotCommitted) || self.original_failure.is_none() {
            return Err(self);
        }
        Ok(StagedDraftPieceCommandNoncommitV1 {
            command: *self.command,
            failure: self
                .original_failure
                .expect("noncommit retains the original failure"),
            local_failure: self.failure,
            verification: self.verification,
        })
    }
}

impl std::fmt::Debug for StagedDraftPieceOutcomeFlightV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StagedDraftPieceOutcomeFlightV1")
            .field("state", &self.state())
            .field("classification", &self.classification)
            .field("failure", &self.failure)
            .field("original_failure", &self.original_failure)
            .field("verification", &self.verification)
            .finish_non_exhaustive()
    }
}

pub struct StagedDraftPieceCommandCompletionV1 {
    pub result: DraftPieceReconciledCommandV1,
    pub receipt: CommitReceipt,
    pub original_failure: Option<CommandError>,
    pub later_failure: Option<CommandError>,
    pub cleanup_failure: Option<CommandError>,
    pub cleanup_receipt: Option<CommitReceipt>,
    pub local_finalization: StagedDraftPieceLocalFinalizationV1,
    pub cleanup_local_finalization: StagedDraftPieceLocalFinalizationV1,
    pub verification: StagedDraftPieceVerificationWorkV1,
    pub inert_cleanup: Option<StagedDraftPieceInertCleanupV1>,
}

pub struct StagedDraftPieceCommandNoncommitV1 {
    pub command: PreparedStagedDraftPieceCommandV1,
    pub failure: CommandError,
    pub local_failure: Option<StagedDraftPieceOutcomeErrorV1>,
    pub verification: StagedDraftPieceVerificationWorkV1,
}

#[must_use]
pub struct StagedDraftPieceInertCleanupV1 {
    admission: DraftMarkerWriterAdmissionV1,
    terminal_digest: DraftPieceDigestV1,
}

impl StagedDraftPieceInertCleanupV1 {
    pub fn owner(&self) -> DraftMarkerAdmissionOwnerV1 {
        self.admission.binding().owner()
    }
    pub fn terminal_digest(&self) -> DraftPieceDigestV1 {
        self.terminal_digest
    }
}

#[derive(Clone)]
enum CommandKind {
    Transfer(Box<PreparedDraftMutationTransferV1>),
    Window(Box<PreparedDraftPieceStagingWindowV1>),
    Advance(Box<PreparedDraftPieceAdvanceV1>),
    Terminal(
        Box<PreparedDraftPieceEditV1>,
        StagedDraftPieceTerminalElectionV1,
    ),
}

impl CommandKind {
    fn kind(&self) -> StagedDraftPieceCommandKindV1 {
        match self {
            Self::Transfer(_) => StagedDraftPieceCommandKindV1::Transfer,
            Self::Window(_) => StagedDraftPieceCommandKindV1::Window,
            Self::Advance(_) => StagedDraftPieceCommandKindV1::Advance,
            Self::Terminal(..) => StagedDraftPieceCommandKindV1::Terminal,
        }
    }
}

#[derive(Clone)]
struct CapturedState {
    staging: DraftMutationStagingHeadV1,
    build: Option<DraftPieceBuildRecordV1>,
    session: DraftEditorCandidateSessionV1,
    settlement: Option<DraftPieceSettlementV1>,
    terminal_admission: Option<(
        DraftMarkerAdmissionHeadV1,
        DraftMarkerAdmissionReplayReceiptV1,
    )>,
}

impl CapturedState {
    fn admission(&self) -> Option<DraftMarkerWriterAdmissionV1> {
        self.build
            .as_ref()
            .and_then(DraftPieceBuildRecordV1::writer_admission)
            .or_else(|| self.staging.begin().writer_admission())
    }
}

struct CapturedCommand {
    source: Box<CapturedState>,
    target: Box<CapturedState>,
    replayed: bool,
}

#[derive(Clone, Copy)]
enum SelectedSide {
    Source,
    Target,
}

enum FlightPhase {
    NotCommitted,
    Reconciling {
        handle: ReconciliationHandle,
        retry_failed: bool,
    },
    Verifying(SelectedSide),
    Finalizing(SelectedSide),
    CleanupPending,
    CleanupReconciling {
        handle: ReconciliationHandle,
        retry_failed: bool,
    },
    CleanupVerifying,
    CleanupFinalizing,
    Complete,
    Unavailable(Box<FlightPhase>),
}

type CaptureSlot = Arc<Mutex<Option<Box<CapturedCommand>>>>;

fn phase_has_reconciliation(phase: &FlightPhase) -> bool {
    match phase {
        FlightPhase::Reconciling { .. } | FlightPhase::CleanupReconciling { .. } => true,
        FlightPhase::Unavailable(retained) => phase_has_reconciliation(retained),
        _ => false,
    }
}

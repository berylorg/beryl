use beryl_model::{SyndicThreadId, WindowId};
use beryl_state::WindowClaimSelection;
use syndic_storage::{DraftEditorCandidateSessionIdV1, DraftPieceOperationIdV1};

use crate::composer_host::{
    ComposerHostActivationOutcome, ComposerHostBinding, ComposerHostError, ComposerHostFlushState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerSelectionIdentity {
    pub(super) window_id: WindowId,
    pub(super) claim: WindowClaimSelection,
    pub(super) binding: ComposerHostBinding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerWidgetRelease {
    selection: MainWindowComposerSelectionIdentity,
}

impl MainWindowComposerWidgetRelease {
    pub const fn selection(self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub(in crate::main_window) const fn new(
        selection: MainWindowComposerSelectionIdentity,
    ) -> Self {
        Self { selection }
    }

    #[cfg(feature = "test-faults")]
    pub const fn for_test(selection: MainWindowComposerSelectionIdentity) -> Self {
        Self::new(selection)
    }
}

impl MainWindowComposerSelectionIdentity {
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }

    pub const fn claim(self) -> WindowClaimSelection {
        self.claim
    }

    pub const fn binding(self) -> ComposerHostBinding {
        self.binding
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MainWindowComposerActivationReceipt {
    pub(super) window_id: WindowId,
    pub(super) generation: u64,
    pub(super) target_thread: SyndicThreadId,
    pub(super) session_id: DraftEditorCandidateSessionIdV1,
    pub(super) open_operation_id: DraftPieceOperationIdV1,
    pub(super) presentation_generation: std::num::NonZeroU64,
    pub(super) expected_prior: MainWindowComposerSelectionIdentity,
}

impl MainWindowComposerActivationReceipt {
    pub const fn generation(self) -> u64 {
        self.generation
    }

    pub const fn target_thread(self) -> SyndicThreadId {
        self.target_thread
    }

    pub const fn session_id(self) -> DraftEditorCandidateSessionIdV1 {
        self.session_id
    }

    pub const fn open_operation_id(self) -> DraftPieceOperationIdV1 {
        self.open_operation_id
    }

    pub const fn presentation_generation(self) -> std::num::NonZeroU64 {
        self.presentation_generation
    }

    pub const fn expected_prior(self) -> MainWindowComposerSelectionIdentity {
        self.expected_prior
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerPendingStatus {
    Ready,
    Publishing(ComposerHostFlushState),
    WidgetReleaseRequired,
    RetirementPending,
    ReconciliationPending,
    DepartedFreshBoundary,
}

#[derive(Debug)]
pub enum MainWindowComposerActivationAdvance {
    Ready(MainWindowComposerActivationReceipt),
    Cancelled,
    Rejected(ComposerHostActivationOutcome),
    FailureRetired(ComposerHostError),
    RetirementPending(MainWindowComposerActivationReceipt),
    FailureRetirementPending {
        receipt: MainWindowComposerActivationReceipt,
        error: ComposerHostError,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerRetirementAdvance {
    Retired,
    Pending,
    DepartedFreshBoundary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerPublishAdvance {
    Progress(ComposerHostFlushState),
    ReconciliationPending,
    WidgetReleaseRequired(MainWindowComposerSelectionIdentity),
    Published(MainWindowComposerSelectionIdentity),
    PriorFlushFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowComposerDisposalAdvance {
    Progress(ComposerHostFlushState),
    ReconciliationPending,
    WidgetReleaseRequired(MainWindowComposerSelectionIdentity),
    Disposed,
    Failed,
}

#[derive(Debug, thiserror::Error)]
pub enum MainWindowComposerSlotError {
    #[error("composer slot identity does not match its host")]
    IdentityMismatch,
    #[error("a target activation is already pending")]
    ActivationPending,
    #[error("the activation receipt is stale or belongs to another window")]
    StaleActivationReceipt,
    #[error("the activation generation is exhausted")]
    GenerationExhausted,
    #[error("the pending target is not ready for publication")]
    TargetNotReady,
    #[error("the pending target cannot be freshly abandoned")]
    TargetNotFresh,
    #[error("the recovered Syndic handle does not belong to this recovered home generation")]
    RecoveryHandleMismatch,
    #[error("the slot is disposed")]
    Disposed,
    #[error("widget release contained work that was not locally releasable")]
    WidgetReleaseIncomplete,
    #[error("composer host failed: {0}")]
    Host(#[from] ComposerHostError),
}

use beryl_home_store::{
    CommandCancellation, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    CursorReadLimits, HomeCommand, HomeStore, ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::{ExecutionBinding, SyndicDraftId, SyndicThreadId, WindowId};
use beryl_state::{
    BerylState, CatalogClaimKind, CatalogClaimReplacementAudit, CatalogClaimReplacementRow,
    CatalogClaimSummary, CatalogPointReadLimit, CatalogSourceRevisions,
    PreparedWindowClaimReplacement, PublishCatalogClaimReplacement, RememberedTarget,
    SessionWindowRecord, ThreadClaimRecord, WindowClaimReplacementPreparation,
    WindowClaimReplacementState, WindowClaimSelection,
};
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1, SyndicStorage, SyndicTimestamp};

use crate::{
    catalog_projection::{CatalogProjectionBuildError, project_facts, validate_execution_binding},
    main_window::running_threads::activation::{RunningThreadActivationError, prepare_catalog_row},
};

#[derive(Debug)]
pub struct SameWindowThreadRequest {
    window: WindowId,
    selected: Option<WindowClaimSelection>,
    target: RememberedTarget,
    creation: CreateThread,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SameWindowThreadDisposition {
    Current,
    Reused,
    Created,
}

#[derive(Debug, thiserror::Error)]
pub enum SameWindowThreadError {
    #[error(transparent)]
    Activation(#[from] RunningThreadActivationError),
    #[error("thread acquisition was cancelled before admission")]
    Cancelled,
    #[error("thread acquisition source changed")]
    SourceChanged,
    #[error("thread acquisition cannot establish the exact joined result")]
    Collision,
    #[error("thread acquisition reconciliation proved noncommit")]
    ReconciledOld,
    #[error("thread acquisition eligibility failed: {0}")]
    Eligibility(String),
}

pub enum SameWindowThreadPreparation {
    Current {
        window: SessionWindowRecord,
        claim: ThreadClaimRecord,
        draft: SyndicDraftId,
    },
    Prepared(SameWindowThreadAcquisition),
}

pub struct SameWindowThreadAcquisition {
    home: beryl_model::BerylHomeId,
    generation: beryl_home_store::HomeGenerationIdentity,
    replacement: PreparedWindowClaimReplacement,
    draft: SyndicDraftId,
    disposition: SameWindowThreadDisposition,
    command: HomeCommand,
    rows: CatalogClaimReplacementAudit,
}

#[derive(Debug)]
pub struct SameWindowThreadCommit {
    pub window: SessionWindowRecord,
    pub selection: WindowClaimSelection,
    pub claim: ThreadClaimRecord,
    pub draft: SyndicDraftId,
    pub disposition: SameWindowThreadDisposition,
    pub receipt: CommitReceipt,
    pub later_failure: Option<beryl_home_store::CommandError>,
    pub local_finalization: Option<CommittedLocalFinalization>,
}

pub enum SameWindowThreadOutcome {
    Settled(SameWindowThreadCommit),
    NotCommitted(SameWindowThreadError),
    Pending(SameWindowThreadPending),
    Unavailable(SameWindowThreadPending),
}

pub struct SameWindowThreadPending {
    replacement: PreparedWindowClaimReplacement,
    draft: SyndicDraftId,
    disposition: SameWindowThreadDisposition,
    rows: CatalogClaimReplacementAudit,
    handle: Option<ReconciliationHandle>,
    receipt: Option<CommitReceipt>,
    later_failure: Option<beryl_home_store::CommandError>,
    local_finalization: Option<CommittedLocalFinalization>,
    problem: SameWindowThreadError,
    unavailable: bool,
}

mod preparation;
mod settlement;

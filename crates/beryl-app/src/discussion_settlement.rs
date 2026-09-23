use crate::process_admission::ProcessExecutionPermit;
use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    HomeCandidateRecoveryAccess, HomeCommand, HomeServiceReference, HomeStore,
    ReconciliationHandle,
};
use beryl_model::{BerylHomeId, JobId};
use beryl_state::{BerylState, ResolvingTransitionWitness};
use std::sync::{Arc, Mutex};
use syndic_storage::{DiscussionHandoffIntent, SyndicStorage};

mod access;
mod flight;
mod prepare;
mod recovery;
pub use flight::DiscussionSettlementOperations;
use flight::Flight;
pub use recovery::DiscussionSettlementAuditOutcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionSettlementResult {
    ReadyForParent,
    ChildInputPending,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscussionSettlementError {
    #[error("discussion settlement was cancelled")]
    Cancelled,
    #[error("discussion settlement capacity is full")]
    Capacity,
    #[error("another operation retains this handoff job")]
    DuplicateIdentity,
    #[error("discussion settlement custody is unavailable")]
    CustodyUnavailable,
    #[error("discussion settlement names another home")]
    ForeignHome,
    #[error("home changed while settlement facts were read")]
    ConcurrentChange,
    #[error("job and pending discussion gate do not identify the same resolving attempt")]
    IdentityMismatch,
    #[error(transparent)]
    Process(#[from] crate::process_admission::ProcessAdmissionError),
    #[error(transparent)]
    Read(#[from] beryl_home_store::ReadError),
    #[error(transparent)]
    Syndic(#[from] syndic_storage::SyndicReadError),
    #[error(transparent)]
    SyndicMutation(#[from] syndic_storage::SyndicMutationError),
    #[error(transparent)]
    State(#[from] beryl_state::DurableJobMutationError),
    #[error(transparent)]
    Build(#[from] beryl_home_store::CommandBuildError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Reconciliation(#[from] beryl_home_store::ReconciliationFailure),
}

#[derive(Clone)]
pub struct DiscussionSettlementAudit(Arc<Attempt>);
struct Attempt {
    home_id: BerylHomeId,
    job: ResolvingTransitionWitness,
    gate: Option<DiscussionHandoffIntent>,
    result: DiscussionSettlementResult,
    disposition: Mutex<Disposition>,
    _flight: Flight,
}
enum Disposition {
    Prepared,
    NotCommitted,
    Committed,
    Indeterminate(ReconciliationHandle),
    Collision,
}

pub enum DiscussionSettlementOutcome {
    NotCommitted {
        evidence: DiscussionSettlementError,
    },
    Committed {
        result: DiscussionSettlementResult,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate {
        failure: CommandError,
        audit: DiscussionSettlementAudit,
    },
}

#[derive(Clone)]
pub struct DiscussionSettlementService {
    operations: DiscussionSettlementOperations,
    store: HomeServiceReference,
    state: BerylState,
    syndic: SyndicStorage,
}
impl DiscussionSettlementService {
    pub fn new(
        operations: DiscussionSettlementOperations,
        store: HomeServiceReference,
        state: BerylState,
        syndic: SyndicStorage,
    ) -> Self {
        Self {
            operations,
            store,
            state,
            syndic,
        }
    }
}

enum Execution<'a> {
    Ordinary(HomeServiceReference),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}
pub struct PreparedDiscussionSettlement<'a> {
    command: Option<HomeCommand>,
    execution: Execution<'a>,
    permit: Option<ProcessExecutionPermit>,
    audit: DiscussionSettlementAudit,
}

impl PreparedDiscussionSettlement<'_> {
    pub fn audit(&self) -> DiscussionSettlementAudit {
        self.audit.clone()
    }
    pub fn execute(mut self) -> DiscussionSettlementOutcome {
        let Ok(mut disposition) = self.audit.0.disposition.lock() else {
            return DiscussionSettlementOutcome::NotCommitted {
                evidence: DiscussionSettlementError::CustodyUnavailable,
            };
        };
        let command = self
            .command
            .take()
            .expect("prepared settlement consumed once");
        let commit = || {
            let outcome = match &self.execution {
                Execution::Ordinary(store) => store.execute(command),
                Execution::Candidate(access) => access.execute(command),
            };
            match outcome {
                CommandOutcome::NotCommitted { evidence } => {
                    *disposition = Disposition::NotCommitted;
                    DiscussionSettlementOutcome::NotCommitted {
                        evidence: evidence.into(),
                    }
                }
                CommandOutcome::Committed {
                    receipt,
                    later_failure,
                    local_finalization,
                } => {
                    *disposition = Disposition::Committed;
                    DiscussionSettlementOutcome::Committed {
                        result: self.audit.0.result,
                        receipt,
                        later_failure,
                        local_finalization,
                    }
                }
                CommandOutcome::Indeterminate {
                    failure,
                    reconciliation,
                } => {
                    *disposition = Disposition::Indeterminate(reconciliation.install_and_handle());
                    DiscussionSettlementOutcome::Indeterminate {
                        failure,
                        audit: self.audit.clone(),
                    }
                }
            }
        };
        let outcome = match &self.permit {
            Some(permit) => permit.commit(commit),
            None => Ok(commit()),
        };
        match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                *disposition = Disposition::NotCommitted;
                DiscussionSettlementOutcome::NotCommitted {
                    evidence: error.into(),
                }
            }
        }
    }
}
impl Drop for PreparedDiscussionSettlement<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.audit.0.disposition.lock() {
            if matches!(*state, Disposition::Prepared) {
                *state = Disposition::NotCommitted;
            }
        }
    }
}

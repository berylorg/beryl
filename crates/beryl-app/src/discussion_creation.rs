use std::sync::{Arc, Mutex};

use beryl_home_store::{
    CommandCancellation, CommandError, CommandOutcome, CommitReceipt, CommittedLocalFinalization,
    HomeCommand, HomeServiceReference, HomeStore, ReconciliationHandle,
};
use beryl_model::{BerylHomeId, SyndicDraftId, SyndicThreadId};
use beryl_state::{
    BerylState, CatalogClaimSummary, CatalogInitialPublication, CatalogSourceRevisions,
};
use syndic_storage::{
    CreateDiscussion, DiscussionCreationIntent, PreparedDiscussionSource, SyndicStorage,
};

use crate::{
    catalog_projection::{project_facts, validate_execution_binding},
    process_admission::ProcessExecutionPermit,
};

mod flight;
mod recovery;

pub use flight::DiscussionCreationOperations;
use flight::Flight;
pub use recovery::DiscussionCreationAuditOutcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreatedDiscussion {
    pub thread_id: SyndicThreadId,
    pub draft_id: SyndicDraftId,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscussionCreationError {
    #[error("discussion creation was cancelled before writer admission")]
    Cancelled,
    #[error("discussion creation capacity is full")]
    Capacity,
    #[error("another operation retains this discussion identity")]
    DuplicateIdentity,
    #[error("discussion creation custody is unavailable")]
    CustodyUnavailable,
    #[error("discussion creation names another home")]
    ForeignHome,
    #[error("the home changed while discussion creation facts were read")]
    ConcurrentChange,
    #[error("the new discussion identity already has a window claim")]
    AlreadyClaimed,
    #[error(transparent)]
    Process(#[from] crate::process_admission::ProcessAdmissionError),
    #[error(transparent)]
    Read(#[from] beryl_home_store::ReadError),
    #[error(transparent)]
    Syndic(#[from] syndic_storage::SyndicReadError),
    #[error(transparent)]
    CatalogRead(#[from] beryl_state::CatalogReadError),
    #[error(transparent)]
    CatalogMutation(#[from] beryl_state::CatalogMutationError),
    #[error(transparent)]
    Projection(#[from] crate::catalog_projection::CatalogProjectionBuildError),
    #[error(transparent)]
    Runtime(#[from] beryl_state::RuntimeRootCatalogSourceError),
    #[error(transparent)]
    Claim(#[from] beryl_state::ThreadClaimCatalogSourceError),
    #[error(transparent)]
    Build(#[from] beryl_home_store::CommandBuildError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Reconciliation(#[from] beryl_home_store::ReconciliationFailure),
}

#[derive(Clone)]
pub struct DiscussionCreationAudit(Arc<Attempt>);

struct Attempt {
    home_id: BerylHomeId,
    syndic: DiscussionCreationIntent,
    catalog: CatalogInitialPublication,
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

impl DiscussionCreationAudit {
    pub fn identity(&self) -> CreatedDiscussion {
        CreatedDiscussion {
            thread_id: self.0.syndic.thread_id(),
            draft_id: self.0.syndic.draft_id(),
        }
    }
}

pub enum DiscussionCreationOutcome {
    NotCommitted {
        evidence: DiscussionCreationError,
    },
    Committed {
        created: CreatedDiscussion,
        receipt: CommitReceipt,
        later_failure: Option<CommandError>,
        local_finalization: Option<CommittedLocalFinalization>,
    },
    Indeterminate {
        failure: CommandError,
        audit: DiscussionCreationAudit,
    },
}

#[derive(Clone)]
pub struct DiscussionCreationService {
    operations: DiscussionCreationOperations,
    store: HomeServiceReference,
    state: BerylState,
    syndic: SyndicStorage,
}

impl DiscussionCreationService {
    pub fn new(
        operations: DiscussionCreationOperations,
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

    pub fn prepare(
        &self,
        source: PreparedDiscussionSource,
        request: CreateDiscussion,
        cancellation: CommandCancellation,
    ) -> Result<PreparedDiscussionCreationOperation, DiscussionCreationError> {
        if cancellation.is_cancelled() {
            return Err(DiscussionCreationError::Cancelled);
        }
        let permit = self.operations.permit();
        let flight = permit.commit(|| self.operations.acquire(request.thread_id()))??;
        let before = self.store.home_revision()?;
        let creation = self
            .syndic
            .prepare_discussion_creation(&self.store, source, request)?;
        let intent = creation.intent().clone();
        let summary = intent.initial_catalog_summary();
        let runtime_revision = self.state.runtime_roots().revision(&self.store)?;
        let runtime_source = self.state.runtime_roots().catalog_source(
            &self.store,
            summary.execution().runtime_id(),
            summary.execution().root_id(),
        )?;
        validate_execution_binding(summary, &runtime_source)?;
        let session_revision = self.state.session().revision(&self.store)?;
        let claim = self
            .state
            .session()
            .thread_claim_catalog_source(&self.store, request.thread_id())?;
        if claim.claim().is_some() {
            return Err(DiscussionCreationError::AlreadyClaimed);
        }
        let facts = project_facts(summary, &runtime_source, CatalogClaimSummary::Unclaimed)?;
        let sources = CatalogSourceRevisions::new(
            summary.revision(),
            runtime_source.runtime().revision(),
            runtime_source.root().revision(),
            None,
        );
        let catalog = self.state.catalog().prepare_initial_publication(
            &self.store,
            request.thread_id(),
            sources,
            facts,
        )?;
        let publication = catalog.publication().clone();
        let mut command = HomeCommand::new(before).with_cancellation(cancellation);
        command.add(creation.contribution())?;
        command.add(catalog.contribution())?;
        command.add_validation(
            self.state
                .runtime_roots()
                .validate_catalog_source(runtime_revision, runtime_source),
        )?;
        command.add_validation(
            self.state
                .session()
                .validate_thread_claim_catalog_source(session_revision, claim),
        )?;
        if self.store.home_revision()? != before {
            return Err(DiscussionCreationError::ConcurrentChange);
        }
        Ok(PreparedDiscussionCreationOperation {
            command: Some(command),
            store: self.store.clone(),
            permit,
            audit: DiscussionCreationAudit(Arc::new(Attempt {
                home_id: self.store.home_id(),
                syndic: intent,
                catalog: publication,
                disposition: Mutex::new(Disposition::Prepared),
                _flight: flight,
            })),
        })
    }
}

pub struct PreparedDiscussionCreationOperation {
    command: Option<HomeCommand>,
    store: HomeServiceReference,
    permit: ProcessExecutionPermit,
    audit: DiscussionCreationAudit,
}

impl PreparedDiscussionCreationOperation {
    pub fn audit(&self) -> DiscussionCreationAudit {
        self.audit.clone()
    }

    pub fn execute(mut self) -> DiscussionCreationOutcome {
        let Ok(mut disposition) = self.audit.0.disposition.lock() else {
            return DiscussionCreationOutcome::NotCommitted {
                evidence: DiscussionCreationError::CustodyUnavailable,
            };
        };
        let command = self
            .command
            .take()
            .expect("prepared command is consumed once");
        match self.permit.commit(|| match self.store.execute(command) {
            CommandOutcome::NotCommitted { evidence } => {
                *disposition = Disposition::NotCommitted;
                DiscussionCreationOutcome::NotCommitted {
                    evidence: evidence.into(),
                }
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => {
                *disposition = Disposition::Committed;
                DiscussionCreationOutcome::Committed {
                    created: self.audit.identity(),
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
                DiscussionCreationOutcome::Indeterminate {
                    failure,
                    audit: self.audit.clone(),
                }
            }
        }) {
            Ok(outcome) => outcome,
            Err(error) => {
                *disposition = Disposition::NotCommitted;
                DiscussionCreationOutcome::NotCommitted {
                    evidence: error.into(),
                }
            }
        }
    }
}

impl Drop for PreparedDiscussionCreationOperation {
    fn drop(&mut self) {
        if let Ok(mut state) = self.audit.0.disposition.lock() {
            if matches!(*state, Disposition::Prepared) {
                *state = Disposition::NotCommitted;
            }
        }
    }
}

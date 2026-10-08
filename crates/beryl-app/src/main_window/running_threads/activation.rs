use beryl_home_store::{
    CommandBuildError, CommandCancellation, CommandError, CommandOutcome, CommitReceipt,
    CommittedLocalFinalization, HomeCommand, HomeStore, ReadError, ReconciliationFailure,
    ReconciliationHandle, ReconciliationResolution,
};
use beryl_model::{SyndicThreadId, WindowId};
use beryl_state::{
    BerylState, CatalogClaimKind, CatalogClaimReplacementAudit, CatalogClaimReplacementRow,
    CatalogClaimSummary, CatalogCurrentRowError, CatalogPointReadLimit, CatalogReadError,
    CatalogSourceRevisions, CatalogWindowClaim, PreparedWindowClaimReplacement,
    PublishCatalogClaimReplacement, RememberedTarget, SessionMutationError, SessionWindowRecord,
    ThreadClaimRecord, WindowClaimReplacementPreparation, WindowClaimReplacementState,
    WindowClaimSelection,
};
use syndic_storage::{
    SyndicMutationError, SyndicReadError, SyndicStorage, ThreadCatalogSummaryPreparation,
};

use crate::catalog_projection::{
    CatalogProjectionBuildError, project_facts, validate_execution_binding,
};

#[derive(Debug, thiserror::Error)]
pub enum RunningThreadActivationError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Session(#[from] SessionMutationError),
    #[error(transparent)]
    Projection(#[from] CatalogProjectionBuildError),
    #[error(transparent)]
    CatalogRead(#[from] CatalogReadError),
    #[error(transparent)]
    CatalogAudit(#[from] CatalogCurrentRowError),
    #[error(transparent)]
    SyndicRead(#[from] SyndicReadError),
    #[error(transparent)]
    SyndicMutation(#[from] SyndicMutationError),
    #[error(transparent)]
    Build(#[from] CommandBuildError),
    #[error(transparent)]
    Command(#[from] CommandError),
    #[error(transparent)]
    Reconciliation(#[from] ReconciliationFailure),
    #[error("activation thread no longer exists")]
    ThreadMissing,
    #[error("activation sources changed during preparation")]
    SourceChanged,
    #[error("activation execution binding differs from the requested target")]
    TargetMismatch,
    #[error("activation audit does not equal the exact joined target")]
    Collision,
    #[error("activation reconciliation proved the exact original state")]
    ReconciledOld,
}

#[derive(Debug)]
pub enum RunningThreadActivationPreparation {
    Current {
        window: SessionWindowRecord,
        claim: ThreadClaimRecord,
    },
    ClaimedElsewhere {
        claim: ThreadClaimRecord,
    },
    Prepared(RunningThreadActivation),
}

#[derive(Debug)]
pub struct RunningThreadActivation {
    prepared: PreparedWindowClaimReplacement,
}

#[derive(Debug)]
pub struct RunningThreadActivationCommit {
    pub window: SessionWindowRecord,
    pub selection: WindowClaimSelection,
    pub claim: ThreadClaimRecord,
    pub receipt: CommitReceipt,
    pub later_failure: Option<CommandError>,
    pub local_finalization: Option<CommittedLocalFinalization>,
}

#[derive(Debug)]
pub enum RunningThreadActivationOutcome {
    Settled(RunningThreadActivationCommit),
    NotCommitted(RunningThreadActivationError),
    Pending(RunningThreadActivationPending),
}

#[derive(Debug)]
pub struct RunningThreadActivationPending {
    activation: RunningThreadActivation,
    rows: CatalogClaimReplacementAudit,
    handle: Option<ReconciliationHandle>,
    receipt: Option<CommitReceipt>,
    later_failure: Option<CommandError>,
    local_finalization: Option<CommittedLocalFinalization>,
    problem: RunningThreadActivationError,
}

impl RunningThreadActivation {
    pub fn prepare(
        store: &HomeStore,
        state: &BerylState,
        syndic: &SyndicStorage,
        window_id: WindowId,
        expected_selected: Option<WindowClaimSelection>,
        target: RememberedTarget,
        thread_id: SyndicThreadId,
    ) -> Result<RunningThreadActivationPreparation, RunningThreadActivationError> {
        let revision = store.home_revision()?;
        let source = syndic
            .prepare_thread_catalog_summary(store, thread_id)?
            .ok_or(RunningThreadActivationError::ThreadMissing)?;
        let summary = summary(&source);
        if summary.execution().runtime_id() != target.runtime_id()
            || summary.execution().root_id() != target.root_id()
        {
            return Err(RunningThreadActivationError::TargetMismatch);
        }
        let runtime = state
            .runtime_roots()
            .catalog_source(store, target.runtime_id(), target.root_id())
            .map_err(CatalogProjectionBuildError::from)?;
        validate_execution_binding(summary, &runtime)?;
        let prepared = state.session().prepare_window_claim_replacement(
            store,
            window_id,
            expected_selected,
            target,
            thread_id,
        )?;
        if store.home_revision()? != revision {
            return Err(RunningThreadActivationError::SourceChanged);
        }
        Ok(match prepared {
            WindowClaimReplacementPreparation::Current { window, claim } => {
                RunningThreadActivationPreparation::Current { window, claim }
            }
            WindowClaimReplacementPreparation::ClaimedElsewhere { claim } => {
                RunningThreadActivationPreparation::ClaimedElsewhere { claim }
            }
            WindowClaimReplacementPreparation::Prepared(prepared) => {
                RunningThreadActivationPreparation::Prepared(Self { prepared })
            }
        })
    }

    pub fn future_selection(&self) -> WindowClaimSelection {
        self.prepared.future_selection()
    }
    pub fn future_window(&self) -> &SessionWindowRecord {
        self.prepared.future_window()
    }

    pub fn commit(
        self,
        store: &HomeStore,
        state: &BerylState,
        syndic: &SyndicStorage,
        cancellation: CommandCancellation,
    ) -> RunningThreadActivationOutcome {
        let (command, rows) = match self.command(store, state, syndic, cancellation) {
            Ok(prepared) => prepared,
            Err(error) => return RunningThreadActivationOutcome::NotCommitted(error),
        };
        match store.execute(command) {
            CommandOutcome::NotCommitted { evidence } => {
                RunningThreadActivationOutcome::NotCommitted(evidence.into())
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => RunningThreadActivationPending {
                activation: self,
                rows,
                handle: None,
                receipt: Some(receipt),
                later_failure,
                local_finalization,
                problem: RunningThreadActivationError::Collision,
            }
            .audit(store, state),
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => RunningThreadActivationOutcome::Pending(RunningThreadActivationPending {
                activation: self,
                rows,
                handle: Some(reconciliation.install_and_handle()),
                receipt: None,
                later_failure: None,
                local_finalization: None,
                problem: failure.into(),
            }),
        }
    }

    fn command(
        &self,
        store: &HomeStore,
        state: &BerylState,
        syndic: &SyndicStorage,
        cancellation: CommandCancellation,
    ) -> Result<(HomeCommand, CatalogClaimReplacementAudit), RunningThreadActivationError> {
        let revision = store.home_revision()?;
        let contribution = self.prepared.contribution(&state.session(), store)?;
        let catalog_revision = state.catalog().revision(store)?;
        let runtime_revision = state.runtime_roots().revision(store)?;
        let mut command = HomeCommand::new(revision).with_cancellation(cancellation);
        command.add(contribution)?;
        let (target_row, target_source, target_runtime) = prepare_catalog_row(
            store,
            state,
            syndic,
            self.prepared.future_claim().thread_id(),
            Some(self.prepared.future_claim()),
        )?;
        if target_runtime.runtime().runtime_id() != self.prepared.target().runtime_id()
            || target_runtime.root().root_id() != self.prepared.target().root_id()
        {
            return Err(RunningThreadActivationError::TargetMismatch);
        }
        let (old_row, old_source) = if let Some(old_claim) = self.prepared.prior_claim() {
            let (row, source, _) =
                prepare_catalog_row(store, state, syndic, old_claim.thread_id(), None)?;
            (Some(row), Some(source))
        } else {
            (None, None)
        };
        let (publication, audit) =
            PublishCatalogClaimReplacement::from_planned_rows(target_row, old_row);
        command.add(
            state
                .catalog()
                .replace_claim_projection(catalog_revision, publication),
        )?;
        match (target_source, old_source) {
            (ThreadCatalogSummaryPreparation::ExactCurrent(first), None) => {
                command
                    .add_validation(syndic.validate_thread_catalog_summary_pair(first, None)?)?;
            }
            (
                ThreadCatalogSummaryPreparation::ExactCurrent(first),
                Some(ThreadCatalogSummaryPreparation::ExactCurrent(second)),
            ) => {
                command.add_validation(
                    syndic.validate_thread_catalog_summary_pair(first, Some(second))?,
                )?;
            }
            (first, second) => {
                command.add(syndic.publish_thread_catalog_summary_pair(first, second)?)?;
            }
        }
        command.add_validation(
            state
                .runtime_roots()
                .validate_catalog_source(runtime_revision, target_runtime),
        )?;
        if store.home_revision()? != revision {
            return Err(RunningThreadActivationError::SourceChanged);
        }
        Ok((command, audit))
    }
}

impl RunningThreadActivationPending {
    pub fn problem(&self) -> &RunningThreadActivationError {
        &self.problem
    }

    pub fn reconcile(
        mut self,
        store: &HomeStore,
        state: &BerylState,
    ) -> RunningThreadActivationOutcome {
        if let Some(handle) = &self.handle {
            match store.reconcile(handle) {
                Ok(ReconciliationResolution::ExactOld) => {
                    return match state
                        .session()
                        .classify_window_claim_replacement(store, &self.activation.prepared)
                    {
                        Ok(WindowClaimReplacementState::Original) => {
                            RunningThreadActivationOutcome::NotCommitted(
                                RunningThreadActivationError::ReconciledOld,
                            )
                        }
                        Ok(_) => {
                            self.problem = RunningThreadActivationError::Collision;
                            RunningThreadActivationOutcome::Pending(self)
                        }
                        Err(error) => {
                            self.problem = error.into();
                            RunningThreadActivationOutcome::Pending(self)
                        }
                    };
                }
                Ok(ReconciliationResolution::ExactNew { receipt }) => self.receipt = Some(receipt),
                Ok(
                    ReconciliationResolution::ExactSuccessor { .. }
                    | ReconciliationResolution::Collision,
                ) => {
                    self.problem = RunningThreadActivationError::Collision;
                    return RunningThreadActivationOutcome::Pending(self);
                }
                Err(error) => {
                    self.problem = error.into();
                    return RunningThreadActivationOutcome::Pending(self);
                }
            }
        }
        self.audit(store, state)
    }

    fn audit(mut self, store: &HomeStore, state: &BerylState) -> RunningThreadActivationOutcome {
        let audit = (|| -> Result<(), RunningThreadActivationError> {
            let revision = store.home_revision()?;
            if self.receipt.is_none()
                || state
                    .session()
                    .classify_window_claim_replacement(store, &self.activation.prepared)?
                    != WindowClaimReplacementState::Committed
            {
                return Err(RunningThreadActivationError::Collision);
            }
            for thread_id in self.rows.thread_ids() {
                let current = state
                    .catalog()
                    .current_row_source(store, thread_id, CatalogPointReadLimit::schema_maximum())?
                    .ok_or(RunningThreadActivationError::Collision)?;
                let row = current.row();
                if !self.rows.matches_row(row) {
                    return Err(RunningThreadActivationError::Collision);
                }
            }
            if store.home_revision()? != revision {
                return Err(RunningThreadActivationError::SourceChanged);
            }
            Ok(())
        })();
        if let Err(error) = audit {
            self.problem = error;
            return RunningThreadActivationOutcome::Pending(self);
        }
        RunningThreadActivationOutcome::Settled(RunningThreadActivationCommit {
            window: self.activation.prepared.future_window().clone(),
            selection: self.activation.prepared.future_selection(),
            claim: self.activation.prepared.future_claim(),
            receipt: self.receipt.take().expect("audited commit receipt"),
            later_failure: self.later_failure,
            local_finalization: self.local_finalization,
        })
    }
}

pub(crate) fn summary(
    source: &ThreadCatalogSummaryPreparation,
) -> &syndic_storage::ThreadCatalogSummaryRecord {
    match source {
        ThreadCatalogSummaryPreparation::ExactCurrent(exact) => exact.summary(),
        ThreadCatalogSummaryPreparation::PreparedReplacement(prepared) => prepared.replacement(),
    }
}

pub(crate) fn prepare_catalog_row(
    store: &HomeStore,
    state: &BerylState,
    syndic: &SyndicStorage,
    thread_id: SyndicThreadId,
    claim: Option<ThreadClaimRecord>,
) -> Result<
    (
        CatalogClaimReplacementRow,
        ThreadCatalogSummaryPreparation,
        beryl_state::RuntimeRootCatalogSource,
    ),
    RunningThreadActivationError,
> {
    prepare_catalog_row_with_claim(
        store,
        state,
        syndic,
        thread_id,
        claim.map_or(CatalogClaimSummary::Unclaimed, |claim| {
            CatalogClaimSummary::claimed(claim.window_id(), CatalogClaimKind::Active)
        }),
        claim.map(|claim| claim.revision()),
    )
}

pub(crate) fn prepare_catalog_row_for_claim(
    store: &HomeStore,
    state: &BerylState,
    syndic: &SyndicStorage,
    thread_id: SyndicThreadId,
    claim: Option<CatalogWindowClaim>,
) -> Result<
    (
        CatalogClaimReplacementRow,
        ThreadCatalogSummaryPreparation,
        beryl_state::RuntimeRootCatalogSource,
    ),
    RunningThreadActivationError,
> {
    if claim.is_some_and(|claim| claim.thread_id() != thread_id) {
        return Err(CatalogProjectionBuildError::ThreadClaimMismatch.into());
    }
    prepare_catalog_row_with_claim(
        store,
        state,
        syndic,
        thread_id,
        claim.map_or(CatalogClaimSummary::Unclaimed, |claim| {
            CatalogClaimSummary::claimed(claim.window_id(), CatalogClaimKind::Active)
        }),
        claim.map(|claim| claim.revision()),
    )
}

fn prepare_catalog_row_with_claim(
    store: &HomeStore,
    state: &BerylState,
    syndic: &SyndicStorage,
    thread_id: SyndicThreadId,
    projected_claim: CatalogClaimSummary,
    claim_revision: Option<beryl_model::ClaimRevision>,
) -> Result<
    (
        CatalogClaimReplacementRow,
        ThreadCatalogSummaryPreparation,
        beryl_state::RuntimeRootCatalogSource,
    ),
    RunningThreadActivationError,
> {
    let source = syndic
        .prepare_thread_catalog_summary(store, thread_id)?
        .ok_or(RunningThreadActivationError::ThreadMissing)?;
    let summary = summary(&source);
    let runtime = state
        .runtime_roots()
        .catalog_source(
            store,
            summary.execution().runtime_id(),
            summary.execution().root_id(),
        )
        .map_err(CatalogProjectionBuildError::from)?;
    validate_execution_binding(summary, &runtime)?;
    let facts = project_facts(summary, &runtime, projected_claim)?;
    let sources = CatalogSourceRevisions::new(
        summary.revision(),
        runtime.runtime().revision(),
        runtime.root().revision(),
        claim_revision,
    );
    let current = state
        .catalog()
        .row(store, thread_id, CatalogPointReadLimit::schema_maximum())?;
    let publication = CatalogClaimReplacementRow::new(thread_id, current, sources, facts)
        .map_err(CatalogProjectionBuildError::from)?;
    Ok((publication, source, runtime))
}

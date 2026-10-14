use beryl_home_store::{
    CommitReceipt, CommittedLocalFinalization, HomeCommand, HomeGenerationIdentity, HomeStore,
    ReconciliationHandle,
};

use super::{status::StagingOutcomeExpectation, *};

#[derive(Debug)]
pub enum DraftMutationStagingPageOutcomeErrorV1 {
    Staging(DraftMutationStagingErrorV1),
    ForeignHome,
    LocalFinalizationFailed,
}

impl fmt::Display for DraftMutationStagingPageOutcomeErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Staging(error) => error.fmt(formatter),
            Self::ForeignHome => formatter.write_str("staging outcome belongs to another Home"),
            Self::LocalFinalizationFailed => {
                formatter.write_str("staging local finalization failed")
            }
        }
    }
}

impl Error for DraftMutationStagingPageOutcomeErrorV1 {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Staging(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DraftMutationStagingErrorV1> for DraftMutationStagingPageOutcomeErrorV1 {
    fn from(error: DraftMutationStagingErrorV1) -> Self {
        Self::Staging(error)
    }
}

pub struct DraftMutationStagingPageSubmissionErrorV1 {
    prepared: Box<PreparedDraftMutationStagingBatchV1>,
    error: DraftMutationStagingPageOutcomeErrorV1,
}

impl DraftMutationStagingPageSubmissionErrorV1 {
    pub fn into_parts(
        self,
    ) -> (
        PreparedDraftMutationStagingBatchV1,
        DraftMutationStagingPageOutcomeErrorV1,
    ) {
        (*self.prepared, self.error)
    }

    pub fn error(&self) -> &DraftMutationStagingPageOutcomeErrorV1 {
        &self.error
    }
}

impl fmt::Debug for DraftMutationStagingPageSubmissionErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("StagingPageSubmissionError")
            .field(&self.error)
            .finish()
    }
}

pub struct DraftMutationStagingPageOutcomeFlightV1 {
    state: Box<PageOutcomeState>,
}

struct PageOutcomeState {
    storage: SyndicStorage,
    identity: HomeGenerationIdentity,
    prepared: Box<PreparedDraftMutationStagingBatchV1>,
    not_committed: bool,
    original_failure: Option<Box<CommandError>>,
    later_failure: Option<Box<CommandError>>,
    receipt: Option<CommitReceipt>,
    local_finalization: Option<CommittedLocalFinalization>,
    finalization_consumed: bool,
    finalization_failure: Option<Box<DraftMutationStagingErrorV1>>,
    reconciliation: Option<ReconciliationHandle>,
    retry_failed: bool,
    rejected_resolution: Option<ReconciliationResolution>,
    expectation: Option<StagingOutcomeExpectation>,
    selected: Option<DraftMutationStagingReconcileV1>,
    #[cfg(feature = "test-faults")]
    fail_exact_read: bool,
}

impl SyndicStorage {
    pub fn submit_draft_mutation_staging_page_batch(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        prepared: PreparedDraftMutationStagingBatchV1,
    ) -> Result<DraftMutationStagingPageOutcomeFlightV1, DraftMutationStagingPageSubmissionErrorV1>
    {
        self.submit_staging_page_batch(store, expected_revision, prepared, || {})
    }

    #[cfg(feature = "test-faults")]
    pub fn submit_draft_mutation_staging_page_batch_with_before_execute_faults(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        prepared: PreparedDraftMutationStagingBatchV1,
        before_execute: impl FnOnce(),
    ) -> Result<DraftMutationStagingPageOutcomeFlightV1, DraftMutationStagingPageSubmissionErrorV1>
    {
        self.submit_staging_page_batch(store, expected_revision, prepared, before_execute)
    }

    fn submit_staging_page_batch(
        &self,
        store: &HomeStore,
        expected_revision: DomainRevision,
        prepared: PreparedDraftMutationStagingBatchV1,
        before_execute: impl FnOnce(),
    ) -> Result<DraftMutationStagingPageOutcomeFlightV1, DraftMutationStagingPageSubmissionErrorV1>
    {
        let admission = (|| {
            store.domain_revision(&self.handle)?;
            let identity = store.generation_identity()?;
            let revision = store.home_revision()?;
            Ok::<_, beryl_home_store::ReadError>((identity, revision))
        })();
        let (identity, home_revision) = match admission {
            Ok(admitted) => admitted,
            Err(error) => {
                return Err(DraftMutationStagingPageSubmissionErrorV1 {
                    prepared: Box::new(prepared),
                    error: DraftMutationStagingErrorV1::Read(error.into()).into(),
                });
            }
        };
        let mut command = HomeCommand::new(home_revision);
        command
            .add(self.draft_mutation_staging_page_batch(expected_revision, prepared.clone()))
            .expect("one Syndic participant fits an empty command");
        before_execute();
        let outcome = store.execute(command);
        let mut state = Box::new(PageOutcomeState {
            storage: self.clone(),
            identity,
            prepared: Box::new(prepared),
            not_committed: false,
            original_failure: None,
            later_failure: None,
            receipt: None,
            local_finalization: None,
            finalization_consumed: false,
            finalization_failure: None,
            reconciliation: None,
            retry_failed: false,
            rejected_resolution: None,
            expectation: None,
            selected: None,
            #[cfg(feature = "test-faults")]
            fail_exact_read: false,
        });
        match outcome {
            CommandOutcome::NotCommitted { evidence } => {
                state.not_committed = true;
                state.expectation = Some(match &evidence {
                    CommandError::EmptyContribution { domain } if *domain == SyndicDomain::NAME => {
                        StagingOutcomeExpectation::ExactTargetReplay
                    }
                    _ => StagingOutcomeExpectation::Source,
                });
                state.original_failure = Some(Box::new(evidence));
            }
            CommandOutcome::Committed {
                receipt,
                later_failure,
                local_finalization,
            } => {
                state.receipt = Some(receipt);
                state.later_failure = later_failure.map(Box::new);
                state.local_finalization = local_finalization;
                state.expectation = Some(StagingOutcomeExpectation::Target);
            }
            CommandOutcome::Indeterminate {
                failure,
                reconciliation,
            } => {
                state.original_failure = Some(Box::new(failure));
                state.reconciliation = Some(reconciliation.install_and_handle());
            }
        }
        Ok(DraftMutationStagingPageOutcomeFlightV1 { state })
    }
}

impl DraftMutationStagingPageOutcomeFlightV1 {
    pub fn prepared(&self) -> &PreparedDraftMutationStagingBatchV1 {
        &self.state.prepared
    }
    pub fn original_failure(&self) -> Option<&CommandError> {
        self.state.original_failure.as_deref()
    }
    pub fn later_failure(&self) -> Option<&CommandError> {
        self.state.later_failure.as_deref()
    }
    pub fn committed_receipt(&self) -> Option<&CommitReceipt> {
        self.state.receipt.as_ref()
    }
    pub fn is_not_committed(&self) -> bool {
        self.state.not_committed
    }
    pub fn reconciliation_handle_present(&self) -> bool {
        self.state.reconciliation.is_some()
    }
    pub fn local_finalization_consumed(&self) -> bool {
        self.state.finalization_consumed
    }
    pub fn local_finalization_failure(&self) -> Option<&DraftMutationStagingErrorV1> {
        self.state.finalization_failure.as_deref()
    }

    pub fn into_prepared(self) -> Result<PreparedDraftMutationStagingBatchV1, Self> {
        if self.state.selected != Some(DraftMutationStagingReconcileV1::SourceSelected) {
            return Err(self);
        }
        Ok(*self.state.prepared)
    }

    #[cfg(feature = "test-faults")]
    pub fn fail_next_exact_read_for_test(&mut self) {
        self.state.fail_exact_read = true;
    }

    #[cfg(feature = "test-faults")]
    pub fn shares_payload_for_test(&self, prepared: &PreparedDraftMutationStagingBatchV1) -> bool {
        Arc::ptr_eq(&self.state.prepared.targets, &prepared.targets)
    }

    #[cfg(feature = "test-faults")]
    pub fn has_pending_local_finalization_for_test(&self) -> bool {
        self.state.local_finalization.is_some()
    }

    pub fn resolve(
        &mut self,
        store: &HomeStore,
    ) -> Result<DraftMutationStagingReconcileV1, DraftMutationStagingPageOutcomeErrorV1> {
        let identity = store
            .generation_identity()
            .map_err(|error| DraftMutationStagingErrorV1::Read(error.into()))?;
        if identity != self.state.identity {
            return Err(DraftMutationStagingPageOutcomeErrorV1::ForeignHome);
        }
        if let Some(selected) = self.state.selected {
            return Ok(selected);
        }
        if self.state.rejected_resolution.is_some() {
            return Err(DraftMutationStagingErrorV1::Invariant.into());
        }
        if self.state.expectation.is_none() {
            let handle = self
                .state
                .reconciliation
                .as_ref()
                .expect("indeterminate page flight retains original custody");
            let resolution = if self.state.retry_failed {
                store.retry_reconciliation(handle)
            } else {
                store.reconcile(handle)
            };
            match resolution {
                Err(error) => {
                    self.state.retry_failed = true;
                    return Err(DraftMutationStagingErrorV1::Reconciliation(error).into());
                }
                Ok(ReconciliationResolution::ExactOld) => {
                    self.state.expectation = Some(StagingOutcomeExpectation::Source);
                }
                Ok(ReconciliationResolution::ExactNew { receipt }) => {
                    self.state.receipt = Some(receipt);
                    self.state.expectation = Some(StagingOutcomeExpectation::Target);
                }
                Ok(rejected) => {
                    self.state.rejected_resolution = Some(rejected);
                    return Err(DraftMutationStagingErrorV1::Invariant.into());
                }
            }
        }
        if let Some(local) = self.state.local_finalization.take() {
            self.state.finalization_consumed = true;
            let receipt = self
                .state
                .receipt
                .as_ref()
                .expect("committed flight retains receipt");
            let storage = &self.state.storage;
            let result = store.with_committed_local_finalization(
                local,
                receipt,
                &storage.handle,
                |attachment| {
                    let Some(admission) =
                        self.state.prepared.target_head.begin().writer_admission()
                    else {
                        return Ok(());
                    };
                    if admission.binding().home_generation().get() != storage.home_generation.get()
                    {
                        return Err(());
                    }
                    attachment.finalize_writer_committed_unknown(admission.binding().owner())
                },
            );
            let failure = match result {
                Ok(Ok(())) => None,
                Ok(Err(())) => Some(DraftMutationStagingErrorV1::LocalCustody),
                Err(error) => Some(DraftMutationStagingErrorV1::LocalFinalization(error)),
            };
            if let Some(failure) = failure {
                self.state.finalization_failure = Some(Box::new(failure));
                return Err(DraftMutationStagingPageOutcomeErrorV1::LocalFinalizationFailed);
            }
        }
        #[cfg(feature = "test-faults")]
        if std::mem::take(&mut self.state.fail_exact_read) {
            return Err(
                DraftMutationStagingErrorV1::Read(SyndicReadError::ConcurrentChange {
                    operation: "staging page outcome",
                })
                .into(),
            );
        }
        let storage = &self.state.storage;
        let selected = storage
            .reconcile_draft_mutation_staging_page_batch_exact(store, &self.state.prepared)?;
        if !super::status::staging_outcome_matches(
            self.state
                .expectation
                .expect("page outcome resolves expectation first"),
            &selected,
        ) {
            return Err(DraftMutationStagingErrorV1::Invariant.into());
        }
        if selected == DraftMutationStagingReconcileV1::TargetSelected {
            if let Some(admission) = self.state.prepared.target_head.begin().writer_admission() {
                store
                    .with_domain_attachment(&storage.handle.attachment_capability(), |attachment| {
                        attachment.resolve_writer_progress(admission.binding().owner())
                    })
                    .map_err(|_| DraftMutationStagingErrorV1::LocalCustody)?
                    .map_err(|()| DraftMutationStagingErrorV1::LocalCustody)?;
            }
        }
        self.state.selected = Some(selected);
        Ok(selected)
    }
}

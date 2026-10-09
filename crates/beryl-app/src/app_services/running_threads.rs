use super::*;

mod claim_recovery;
mod ordinary_selection;
mod ordinary_selection_recovery;
mod thread_creation_recovery;
use crate::cas_projection::{
    ProcessWorkError, ProcessWorkPageLimits, ProcessWorkQueryPage, ProcessWorkQueryRevision,
    ProcessWorkReader, ProjectionCancellationToken,
};
use crate::lifecycle_attention::{LifecycleAttentionToken, ProcessLifecycleAttentionPool};
pub(crate) use claim_recovery::*;
pub(crate) use ordinary_selection_recovery::*;
use std::sync::Weak;
pub(crate) use thread_creation_recovery::*;

pub(crate) struct RunningThreadsObservation {
    observation: beryl_home_store::HomeMutationObservation,
    generation: beryl_home_store::HomeGeneration,
}

#[derive(Clone)]
pub(crate) struct PublishedRunningThreadsReader {
    source: ProcessWorkReader,
    state: BerylState,
    lifetime: Weak<()>,
    attention: Weak<ProcessLifecycleAttentionPool>,
    home: Arc<beryl_home_store::HomeServiceReference>,
    syndic: SyndicStorage,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
    observer: beryl_home_store::HomeMutationObserver,
}

impl ProcessServiceOwner {
    pub(crate) fn thread_creation_reader(&self) -> Option<PublishedRunningThreadsReader> {
        self.running_threads_reader()
    }

    pub(crate) fn admit_thread_creation(
        &self,
        members: &[beryl_model::WindowId],
        invoking: beryl_model::WindowId,
    ) -> Result<crate::window_acquisition::WindowSelectionLease, String> {
        self.admit_running_selection(members, invoking)
    }
    pub(crate) fn running_selection_pending(&self) -> bool {
        self.windows.selection_pending()
    }

    #[cfg(test)]
    pub(crate) fn test_recovery_process_is_fenced(&self) -> bool {
        matches!(
            self.process.execution_permit().reserve(),
            Err(crate::process_admission::ProcessAdmissionError::Fenced)
        )
    }
    pub(crate) fn admit_running_selection(
        &self,
        members: &[beryl_model::WindowId],
        invoking: beryl_model::WindowId,
    ) -> Result<crate::window_acquisition::WindowSelectionLease, String> {
        if self.graph().is_none_or(|graph| graph.shutdown.is_some()) {
            return Err("Running threads is unavailable during service retirement.".into());
        }
        self.windows
            .admit_selection(members, invoking)
            .map_err(|error| error.to_string())
    }
    pub(crate) fn running_threads_reader(&self) -> Option<PublishedRunningThreadsReader> {
        let graph = self.graph()?;
        if graph.shutdown.is_some() {
            return None;
        }
        Some(PublishedRunningThreadsReader {
            source: graph
                .cas()
                .process_work_reader(graph.sessions(), graph.attention()),
            state: graph.state().clone(),
            lifetime: Arc::downgrade(graph.restore_lifetime.as_ref()?),
            attention: Arc::downgrade(graph.attention()),
            home: Arc::new(graph.home().service_reference()),
            syndic: graph.syndic().clone(),
            service_generation: graph.cas().service_generation(),
            observer: graph.cas().home_mutation_observer(),
        })
    }
}

impl PublishedRunningThreadsReader {
    pub(crate) fn failed_retirement_home(&self) -> &beryl_home_store::HomeStore {
        &self.home
    }

    pub(crate) fn prepare_thread_creation(
        &self,
        request: crate::same_window_thread_acquisition::SameWindowThreadRequest,
        lease: Arc<crate::window_acquisition::WindowSelectionLease>,
        saved: crate::main_window::MainWindowThreadPredecessorSave,
        cancellation: beryl_home_store::CommandCancellation,
    ) -> Result<PublishedSameWindowThreadPreparation, PublishedSameWindowThreadPreparationFailure>
    {
        let result = (|| -> Result<_, String> {
            if !self.current()
                || lease.invoking() != request.window_id()
                || saved.selected().window_id() != request.window_id()
                || Some(saved.selected().claim()) != request.selected()
            {
                return Err("Thread creation source retired or belongs to another window".into());
            }
            lease
                .validate_publication()
                .map_err(|error| error.to_string())?;
            saved.validate()?;
            let preparation = request
                .prepare(&self.home, &self.state, &self.syndic, cancellation)
                .map_err(|error| error.to_string())?;
            if !self.current() {
                return Err("Thread creation source retired".into());
            }
            Ok(preparation)
        })();
        let preparation = match result {
            Ok(preparation) => preparation,
            Err(error) => {
                return Err(PublishedSameWindowThreadPreparationFailure {
                    reader: self.clone(),
                    lease,
                    saved: Some(saved),
                    error,
                });
            }
        };
        Ok(match preparation {
            crate::same_window_thread_acquisition::SameWindowThreadPreparation::Current {
                window,
                claim,
                draft,
            } => PublishedSameWindowThreadPreparation::Current(PublishedSameWindowThreadCurrent {
                window,
                claim,
                draft,
                custody: PublishedSameWindowThreadPreparationFailure {
                    reader: self.clone(),
                    lease,
                    saved: Some(saved),
                    error: String::new(),
                },
            }),
            crate::same_window_thread_acquisition::SameWindowThreadPreparation::Prepared(
                prepared,
            ) => {
                PublishedSameWindowThreadPreparation::Prepared(PublishedSameWindowThreadOperation {
                    reader: self.clone(),
                    lease,
                    saved: Some(saved),
                    prepared: Some(prepared),
                    outcome: None,
                    adopted_save: None,
                })
            }
        })
    }
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn for_test(
        service: &crate::cas_projection::ProjectionConnectionService,
        sessions: &crate::cas_projection::ScheduledExecutionSessions,
        state: BerylState,
        home: Arc<beryl_home_store::HomeServiceReference>,
        syndic: SyndicStorage,
        attention: &Arc<ProcessLifecycleAttentionPool>,
        lifetime: &Arc<()>,
    ) -> Self {
        Self {
            source: service.process_work_reader(sessions, attention),
            state,
            lifetime: Arc::downgrade(lifetime),
            attention: Arc::downgrade(attention),
            home,
            syndic,
            service_generation: service.service_generation(),
            observer: service.home_mutation_observer(),
        }
    }

    pub(crate) fn observe(&self) -> Result<RunningThreadsObservation, String> {
        if !self.current() {
            return Err("Running threads source retired".into());
        }
        let generation = self
            .home
            .health()
            .generation()
            .ok_or("Running threads home unavailable")?;
        let observation = self
            .observer
            .observe()
            .map_err(|_| "Running threads source changed".to_owned())?;
        Ok(RunningThreadsObservation {
            observation,
            generation,
        })
    }

    pub(crate) fn elect<T>(
        &self,
        observation: &RunningThreadsObservation,
        apply: impl FnOnce() -> T,
    ) -> Result<T, String> {
        if !self.current() {
            return Err("Running threads source retired".into());
        }
        self.home
            .try_elect_observed_coherent(&observation.observation, observation.generation, apply)
            .map_err(|_| "Running threads source changed".into())
    }
    pub(crate) fn transcript_provider(
        &self,
    ) -> Result<
        crate::transcript_provider::TranscriptProviderReader,
        crate::transcript_provider::TranscriptAttachmentError,
    > {
        if !self.current() {
            return Err(crate::transcript_provider::TranscriptAttachmentError::Retired);
        }
        crate::transcript_provider::TranscriptProviderReader::new(
            self.home.clone(),
            self.syndic.clone(),
            self.lifetime.clone(),
            self.service_generation,
            self.observer.clone(),
        )
    }

    pub(crate) fn activation_sources(
        &self,
    ) -> Option<(
        Arc<beryl_home_store::HomeServiceReference>,
        BerylState,
        SyndicStorage,
    )> {
        self.current()
            .then(|| (self.home.clone(), self.state.clone(), self.syndic.clone()))
    }
    pub(crate) fn current(&self) -> bool {
        self.lifetime.upgrade().is_some()
    }

    pub(crate) fn same_publication(&self, other: &Self) -> bool {
        self.lifetime.ptr_eq(&other.lifetime)
    }

    pub(crate) fn query(
        &self,
        query: &beryl_state::CatalogNormalizedQuery,
        start: u64,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ProcessWorkQueryPage, ProcessWorkError> {
        if !self.current() {
            return Err(ProcessWorkError::Closed);
        }
        let revision = self.source.query_revision(&self.state)?;
        let page = self.source.query_page(
            &self.state,
            &revision,
            query,
            start,
            ProcessWorkPageLimits::new(32, 65_536)?,
            cancellation,
        )?;
        if !self.current() {
            return Err(ProcessWorkError::Closed);
        }
        Ok(page)
    }

    pub(crate) fn page(
        &self,
        revision: &ProcessWorkQueryRevision,
        query: &beryl_state::CatalogNormalizedQuery,
        start: u64,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ProcessWorkQueryPage, ProcessWorkError> {
        if !self.current() {
            return Err(ProcessWorkError::Closed);
        }
        let page = self.source.query_page(
            &self.state,
            revision,
            query,
            start,
            ProcessWorkPageLimits::new(32, 65_536)?,
            cancellation,
        )?;
        if !self.current() {
            return Err(ProcessWorkError::Closed);
        }
        Ok(page)
    }

    pub(crate) fn acknowledge(&self, token: &LifecycleAttentionToken) -> bool {
        if !self.current() {
            return false;
        }
        self.attention
            .upgrade()
            .is_some_and(|pool| pool.acknowledge(token))
    }

    pub(crate) fn attention_snapshot(
        &self,
    ) -> Option<Vec<crate::lifecycle_attention::LifecycleAttentionRecord>> {
        if !self.current() {
            return None;
        }
        let records = self.attention.upgrade()?.try_snapshot()?;
        self.current().then_some(records)
    }

    pub(crate) fn position(
        &self,
        revision: &ProcessWorkQueryRevision,
        query: &beryl_state::CatalogNormalizedQuery,
        thread: beryl_model::SyndicThreadId,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Option<u64>, ProcessWorkError> {
        if !self.current() {
            return Err(ProcessWorkError::Closed);
        }
        let position =
            self.source
                .query_position(&self.state, revision, query, thread, cancellation)?;
        if !self.current() {
            return Err(ProcessWorkError::Closed);
        }
        Ok(position)
    }
}

pub(crate) enum PublishedSameWindowThreadPreparation {
    Current(PublishedSameWindowThreadCurrent),
    Prepared(PublishedSameWindowThreadOperation),
}

pub(crate) struct PublishedSameWindowThreadPreparationFailure {
    reader: PublishedRunningThreadsReader,
    lease: Arc<crate::window_acquisition::WindowSelectionLease>,
    saved: Option<crate::main_window::MainWindowThreadPredecessorSave>,
    error: String,
}

impl PublishedSameWindowThreadPreparationFailure {
    pub(crate) fn error(&self) -> &str {
        &self.error
    }
    pub(crate) fn release(mut self) -> Result<(), Self> {
        let Some(saved) = self.saved.take() else {
            return Err(self);
        };
        match saved.release(&self.reader.state) {
            Ok(()) => Ok(()),
            Err((saved, error)) => {
                self.saved = Some(saved);
                self.error = error;
                Err(self)
            }
        }
    }
}

pub(crate) struct PublishedSameWindowThreadCurrent {
    pub(crate) window: beryl_state::SessionWindowRecord,
    pub(crate) claim: beryl_state::ThreadClaimRecord,
    pub(crate) draft: beryl_model::SyndicDraftId,
    custody: PublishedSameWindowThreadPreparationFailure,
}

impl PublishedSameWindowThreadCurrent {
    pub(crate) fn release(self) -> Result<(), PublishedSameWindowThreadPreparationFailure> {
        self.custody.release()
    }
}

pub(crate) struct PublishedSameWindowThreadOperation {
    reader: PublishedRunningThreadsReader,
    lease: Arc<crate::window_acquisition::WindowSelectionLease>,
    saved: Option<crate::main_window::MainWindowThreadPredecessorSave>,
    prepared: Option<crate::same_window_thread_acquisition::SameWindowThreadAcquisition>,
    outcome: Option<crate::same_window_thread_acquisition::SameWindowThreadOutcome>,
    adopted_save: Option<crate::main_window::MainWindowRetiredClaimPredecessorSave>,
}

impl PublishedSameWindowThreadOperation {
    pub(crate) fn future_selection(&self) -> beryl_state::WindowClaimSelection {
        if let Some(prepared) = &self.prepared {
            return prepared.future_selection();
        }
        match self
            .outcome
            .as_ref()
            .expect("original thread creation result")
        {
            crate::same_window_thread_acquisition::SameWindowThreadOutcome::Settled(commit) => {
                commit.selection
            }
            crate::same_window_thread_acquisition::SameWindowThreadOutcome::Pending(pending)
            | crate::same_window_thread_acquisition::SameWindowThreadOutcome::Unavailable(
                pending,
            ) => pending.future_selection(),
            crate::same_window_thread_acquisition::SameWindowThreadOutcome::NotCommitted(_) => {
                unreachable!("noncommit grants no target selection")
            }
        }
    }
    pub(crate) fn commit(&mut self) -> Result<(), String> {
        if self.outcome.is_some() {
            return Err("Thread creation already owns its original result".into());
        }
        if !self.reader.current() {
            return Err("Thread creation source retired".into());
        }
        self.saved
            .as_ref()
            .ok_or("Thread creation predecessor custody is missing")?
            .validate()?;
        let lease = self.lease.clone();
        lease
            .admit_commit(|| {
                let prepared = self
                    .prepared
                    .take()
                    .expect("original thread creation capability");
                self.outcome = Some(prepared.commit(&self.reader.home, &self.reader.state));
            })
            .map_err(|error| error.to_string())
    }
    pub(crate) fn outcome(
        &self,
    ) -> Option<&crate::same_window_thread_acquisition::SameWindowThreadOutcome> {
        self.outcome.as_ref()
    }
    pub(crate) fn reconcile(&mut self) -> Result<(), String> {
        if !self.reader.current() {
            return Err("Thread creation source retired; original custody remains retained".into());
        }
        if !matches!(
            self.outcome,
            Some(crate::same_window_thread_acquisition::SameWindowThreadOutcome::Pending(_))
        ) {
            return Err("Thread creation has no pending reconciliation".into());
        }
        let Some(crate::same_window_thread_acquisition::SameWindowThreadOutcome::Pending(pending)) =
            self.outcome.take()
        else {
            unreachable!()
        };
        self.outcome = Some(pending.reconcile(&self.reader.home, &self.reader.state));
        Ok(())
    }
    pub(crate) fn validate_publication(&self) -> Result<(), String> {
        if !self.reader.current() {
            return Err(
                "Thread creation publication retired; original custody remains retained".into(),
            );
        }
        if !matches!(
            self.outcome,
            Some(crate::same_window_thread_acquisition::SameWindowThreadOutcome::Settled(_))
        ) {
            return Err("Thread creation has no exact committed publication".into());
        }
        self.lease
            .validate_publication()
            .map_err(|error| error.to_string())
    }
    pub(crate) fn reader(&self) -> &PublishedRunningThreadsReader {
        &self.reader
    }
    pub(crate) fn selection_lease(&self) -> &Arc<crate::window_acquisition::WindowSelectionLease> {
        &self.lease
    }
    pub(crate) fn adopt_predecessor_save(
        &mut self,
        receipt: crate::main_window::MainWindowComposerActivationReceipt,
    ) -> Result<(), String> {
        self.validate_publication()?;
        let Some(crate::same_window_thread_acquisition::SameWindowThreadOutcome::Settled(commit)) =
            &self.outcome
        else {
            return Err("Thread creation has no committed target".into());
        };
        let target = commit.selection;
        if receipt.target_thread() != target.thread_id()
            || receipt.expected_prior().window_id() != commit.window.window_id()
        {
            return Err("Thread creation receipt differs from its original target".into());
        }
        let current = self
            .reader
            .state
            .session()
            .capture_window_removal(&self.reader.home, commit.window.window_id())
            .map_err(|error| error.to_string())?;
        if current.window().selected_thread() != Some(target) {
            return Err("Thread creation durable target changed".into());
        }
        let saved = self
            .saved
            .take()
            .ok_or("Thread creation predecessor custody is missing")?;
        match saved.adopt(receipt, target) {
            Ok(saved) => {
                self.adopted_save = Some(saved);
                Ok(())
            }
            Err((saved, error)) => {
                self.saved = Some(saved);
                Err(error)
            }
        }
    }
    pub(crate) fn release_noncommit(mut self) -> Result<(), (Self, String)> {
        if self.prepared.is_none()
            && !matches!(
                self.outcome,
                Some(
                    crate::same_window_thread_acquisition::SameWindowThreadOutcome::NotCommitted(_)
                )
            )
        {
            return Err((self, "Thread creation has no proven noncommit".into()));
        }
        let Some(saved) = self.saved.take() else {
            return Err((
                self,
                "Thread creation predecessor custody is missing".into(),
            ));
        };
        match saved.release(&self.reader.state) {
            Ok(()) => Ok(()),
            Err((saved, error)) => {
                self.saved = Some(saved);
                Err((self, error))
            }
        }
    }
}

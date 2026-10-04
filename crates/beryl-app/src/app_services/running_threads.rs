use super::*;
use crate::cas_projection::{
    ProcessWorkError, ProcessWorkPageLimits, ProcessWorkQueryPage, ProcessWorkQueryRevision,
    ProcessWorkReader, ProjectionCancellationToken,
};
use crate::lifecycle_attention::{LifecycleAttentionToken, ProcessLifecycleAttentionPool};
use std::sync::Weak;

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
    pub(crate) fn running_selection_pending(&self) -> bool {
        self.windows.selection_pending()
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

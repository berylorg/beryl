use std::sync::Weak;

use super::*;
use crate::cas_projection::context_compaction::ContextCompactionCoordinator;

#[derive(Clone)]
pub(in crate::cas_projection) struct ProcessWorkSources {
    home: Weak<HomeStore>,
    home_id: BerylHomeId,
    home_generation: HomeGeneration,
    storage: SyndicStorage,
    session_capacity: usize,
    service_generation: ProjectionServiceGeneration,
    command_authorizer: LiveCommandAuthorizer,
    connections: Weak<ProjectionServiceConnectionRegistry>,
    stop_coordinator: Weak<StopCoordinator>,
    context_compaction: Weak<ContextCompactionCoordinator>,
}

pub(super) struct ProcessWorkRead {
    pub(super) home: Option<Arc<HomeStore>>,
    pub(super) home_id: BerylHomeId,
    pub(super) home_generation: HomeGeneration,
    pub(super) storage: SyndicStorage,
    pub(super) session_capacity: usize,
    pub(super) service_generation: ProjectionServiceGeneration,
    pub(super) command_authorizer: LiveCommandAuthorizer,
    pub(super) connections: Arc<ProjectionServiceConnectionRegistry>,
    pub(super) stop_coordinator: Arc<StopCoordinator>,
    pub(super) context_compaction: Option<Arc<ContextCompactionCoordinator>>,
}

impl ProjectionConnectionService {
    #[cfg(feature = "test-faults")]
    pub fn required_session_work_for_test(
        &self,
        sessions: &super::super::ScheduledExecutionSessions,
        cancellation: &super::super::ProjectionCancellationToken,
    ) -> Result<
        Vec<(
            SyndicThreadId,
            super::super::ScheduledSessionFact,
            ProcessWorkFacts,
        )>,
        ProcessWorkError,
    > {
        self.work_sources()
            .required_session_work(sessions, cancellation)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| (row.thread_id, row.session, row.facts))
                    .collect()
            })
    }

    pub(in crate::cas_projection) fn work_sources(&self) -> ProcessWorkSources {
        ProcessWorkSources {
            home: self.home.as_ref().map_or_else(Weak::new, Arc::downgrade),
            home_id: self.home_id,
            home_generation: self.home_generation,
            storage: self.storage.clone(),
            session_capacity: self.config.worker_capacity().get()
                / super::super::service_config::CONNECTION_WORKER_PERMITS,
            service_generation: self.service_generation,
            command_authorizer: self.command_authorizer.clone(),
            connections: Arc::downgrade(&self.connections),
            stop_coordinator: Arc::downgrade(&self.stop_coordinator),
            context_compaction: self
                .context_compaction
                .as_ref()
                .map_or_else(Weak::new, Arc::downgrade),
        }
    }

    pub(super) fn work_read(&self) -> ProcessWorkRead {
        ProcessWorkRead {
            home: self.home.clone(),
            home_id: self.home_id,
            home_generation: self.home_generation,
            storage: self.storage.clone(),
            session_capacity: self.config.worker_capacity().get()
                / super::super::service_config::CONNECTION_WORKER_PERMITS,
            service_generation: self.service_generation,
            command_authorizer: self.command_authorizer.clone(),
            connections: Arc::clone(&self.connections),
            stop_coordinator: Arc::clone(&self.stop_coordinator),
            context_compaction: self.context_compaction.clone(),
        }
    }
}

impl ProcessWorkSources {
    pub(super) fn read(&self) -> Result<ProcessWorkRead, ProcessWorkError> {
        if !self.command_authorizer.is_open() {
            return Err(ProcessWorkError::Closed);
        }
        let read = ProcessWorkRead {
            home: Some(self.home.upgrade().ok_or(ProcessWorkError::Closed)?),
            home_id: self.home_id,
            home_generation: self.home_generation,
            storage: self.storage.clone(),
            session_capacity: self.session_capacity,
            service_generation: self.service_generation,
            command_authorizer: self.command_authorizer.clone(),
            connections: self.connections.upgrade().ok_or(ProcessWorkError::Closed)?,
            stop_coordinator: self
                .stop_coordinator
                .upgrade()
                .ok_or(ProcessWorkError::Closed)?,
            context_compaction: Some(
                self.context_compaction
                    .upgrade()
                    .ok_or(ProcessWorkError::Closed)?,
            ),
        };
        if !read.command_authorizer.is_open() {
            return Err(ProcessWorkError::Closed);
        }
        Ok(read)
    }
}

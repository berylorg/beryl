use super::super::flight_registry::FlightRegistry;
use super::super::work_sources::{ProcessWorkRead, ProcessWorkSources};
use super::*;
use beryl_home_store::CursorReadLimits;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/shutdown_work_observation.rs"]
mod tests;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ShutdownWorkError {
    #[error(transparent)]
    Work(#[from] ProcessWorkError),
    #[error(transparent)]
    Runtime(#[from] crate::cas_projection::runtime_work::RuntimeWorkError),
    #[error(transparent)]
    Home(#[from] beryl_home_store::HomeObservedCoherenceError),
    #[error(transparent)]
    Admission(#[from] crate::process_admission::ProcessAdmissionError),
}

#[derive(Clone, Debug)]
pub(crate) struct ShutdownWorkObservation {
    pub(super) revision: ShutdownWorkRevision,
    has_work: bool,
    pub(super) home_interval: beryl_home_store::HomeMutationObservation,
    pub(super) connection_interval:
        crate::cas_projection::connection_work::ConnectionWorkObservation,
}

impl ShutdownWorkObservation {
    pub(crate) fn revision(&self) -> &ShutdownWorkRevision {
        &self.revision
    }

    pub(crate) fn has_work(&self) -> bool {
        self.has_work
    }
}

pub(crate) struct ShutdownWorkReadJob {
    sources: ProcessWorkSources,
    sessions: ScheduledExecutionSessions,
}

impl ShutdownWorkReadJob {
    pub(crate) fn collect(
        self,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownWorkObservation, ShutdownWorkError> {
        self.sources
            .collect_shutdown_observation(&self.sessions, cancellation, || {})
    }
}

impl ProjectionConnectionService {
    pub(crate) fn prepare_shutdown_observation(
        &self,
        sessions: &ScheduledExecutionSessions,
    ) -> ShutdownWorkReadJob {
        ShutdownWorkReadJob {
            sources: self.work_sources(),
            sessions: sessions.clone(),
        }
    }

    pub(crate) fn observe_shutdown_work(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownWorkObservation, ShutdownWorkError> {
        self.collect_shutdown_observation(sessions, cancellation, || {})
    }

    fn collect_shutdown_observation(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
        before_validation: impl FnOnce(),
    ) -> Result<ShutdownWorkObservation, ShutdownWorkError> {
        self.work_sources()
            .collect_shutdown_observation(sessions, cancellation, before_validation)
    }
}

impl ProcessWorkSources {
    fn collect_shutdown_observation(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
        before_validation: impl FnOnce(),
    ) -> Result<ShutdownWorkObservation, ShutdownWorkError> {
        check_cancelled(cancellation)?;
        let home_interval = self.mutation_observation()?;
        let read = self.read()?;
        let boundary = read.connections.work_boundary();
        let connection_interval = boundary.try_observe()?;
        let (revision, has_work) =
            read.collect_shutdown_work_facts(sessions, cancellation, before_validation)?;
        let home = read.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        boundary.try_elect(&connection_interval, || {
            home.try_elect_observed_coherent(&home_interval, read.home_generation, || ())
        })??;
        Ok(ShutdownWorkObservation {
            revision,
            has_work,
            home_interval,
            connection_interval,
        })
    }
}

impl ProcessWorkRead {
    fn collect_shutdown_work_facts(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
        before_validation: impl FnOnce(),
    ) -> Result<(ShutdownWorkRevision, bool), ProcessWorkError> {
        check_cancelled(cancellation)?;
        let revision = self.shutdown_work_revision(sessions)?;
        let read = self;
        let home = read.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        let mut has_work = revision.requires_connection_cleanup();
        read.visit_live_facts(sessions, &revision.required, cancellation, |_, facts| {
            has_work |= facts != ProcessWorkFacts::default();
        })?;
        has_work |= !FlightRegistry::work_prefix(
            read.home_id,
            read.home_generation,
            revision.flights,
            None,
            1,
        )?
        .is_empty();

        // These unfiltered source indexes need only one row to establish presence.
        let limits = CursorReadLimits::new(1, 65_536).expect("fixed nonzero limits");
        check_cancelled(cancellation)?;
        has_work |= !read
            .storage
            .non_idle_gate_source_page(home, revision.required.durable, None, limits)?
            .records()
            .is_empty();
        check_cancelled(cancellation)?;
        has_work |= !read
            .storage
            .accepted_next_source_page(home, revision.required.durable, None, limits)?
            .records()
            .is_empty();
        check_cancelled(cancellation)?;
        has_work |= !read
            .storage
            .accepted_ready_source_page(home, revision.required.durable, None, limits)?
            .records()
            .is_empty();
        before_validation();
        self.validate_shutdown_work_revision(sessions, &revision)?;
        check_cancelled(cancellation)?;
        Ok((revision, has_work))
    }
}

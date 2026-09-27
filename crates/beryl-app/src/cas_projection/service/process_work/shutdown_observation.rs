use super::super::work_sources::{ProcessWorkRead, ProcessWorkSources};
use super::*;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/shutdown_work_observation.rs"]
mod tests;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ShutdownWorkError {
    #[error("shutdown window custody is unavailable: {0:?}")]
    Window(#[from] crate::window_acquisition::WindowCloseAdmissionError),
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
    running_threads: u64,
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

    pub(crate) fn running_threads(&self) -> u64 {
        self.running_threads
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
        let (revision, has_work, running_threads) =
            read.collect_shutdown_work_facts(sessions, cancellation, before_validation)?;
        let home = read.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        boundary.try_elect(&connection_interval, || {
            home.try_elect_observed_coherent(&home_interval, read.home_generation, || ())
        })??;
        Ok(ShutdownWorkObservation {
            revision,
            has_work,
            running_threads,
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
    ) -> Result<(ShutdownWorkRevision, bool, u64), ProcessWorkError> {
        check_cancelled(cancellation)?;
        let revision = self.shutdown_work_revision(sessions)?;
        let live = self.live_facts(sessions, &revision.required, cancellation)?;
        let mut running_threads = 0_u64;
        self.scan_work_threads(
            &revision.required,
            live,
            Some(revision.flights),
            cancellation,
            |_, _| {
                running_threads = running_threads
                    .checked_add(1)
                    .ok_or(ProcessWorkError::CountOverflow)?;
                Ok(())
            },
        )?;
        let has_work = revision.requires_connection_cleanup() || running_threads != 0;
        before_validation();
        self.validate_shutdown_work_revision(sessions, &revision)?;
        check_cancelled(cancellation)?;
        Ok((revision, has_work, running_threads))
    }
}

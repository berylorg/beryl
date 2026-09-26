use super::super::flight_registry::FlightRegistry;
use super::*;
use beryl_home_store::CursorReadLimits;

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/shutdown_work_observation.rs"]
mod tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownWorkObservation {
    revision: ShutdownWorkRevision,
    has_work: bool,
}

impl ShutdownWorkObservation {
    pub(crate) fn revision(&self) -> &ShutdownWorkRevision {
        &self.revision
    }

    pub(crate) fn has_work(&self) -> bool {
        self.has_work
    }
}

impl ProjectionConnectionService {
    pub(crate) fn observe_shutdown_work(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownWorkObservation, ProcessWorkError> {
        self.collect_shutdown_observation(sessions, cancellation, || {})
    }

    fn collect_shutdown_observation(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
        before_validation: impl FnOnce(),
    ) -> Result<ShutdownWorkObservation, ProcessWorkError> {
        check_cancelled(cancellation)?;
        let revision = self.shutdown_work_revision(sessions)?;
        let read = self.work_read();
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
        Ok(ShutdownWorkObservation { revision, has_work })
    }
}

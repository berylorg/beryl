use std::sync::Arc;

use super::*;
use crate::cas_projection::{connection_work::ConnectionWorkStamp, runtime_work::RuntimeWorkError};

impl ProjectionConnectionService {
    pub(crate) fn try_validate_shutdown_runtime(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &ShutdownWorkRevision,
    ) -> Result<(), RuntimeWorkError> {
        self.work_read()
            .try_validate_shutdown_runtime(sessions, revision)
    }
}

impl ProcessWorkRead {
    pub(in crate::cas_projection::service) fn validate_shutdown_source_identity(
        &self,
        revision: &ShutdownWorkRevision,
    ) -> Result<(), RuntimeWorkError> {
        let required = &revision.required;
        if self.home.is_none() {
            return Err(RuntimeWorkError::Closed);
        }
        if required.sessions.home_id() != self.home_id
            || required.sessions.home_generation() != self.home_generation
            || required.sessions.service_generation() != self.service_generation
        {
            return Err(RuntimeWorkError::Foreign);
        }
        let connections = &required.connections;
        let stop = &required.controls.stop;
        let compaction = &required.controls.compaction;
        for (owner, home, generation, service) in [
            (
                &connections.owner,
                connections.home_id,
                connections.home_generation,
                connections.service_generation,
            ),
            (
                &stop.owner,
                stop.home_id,
                stop.home_generation,
                stop.service_generation,
            ),
            (
                &compaction.owner,
                compaction.home_id,
                compaction.home_generation,
                compaction.service_generation,
            ),
        ] {
            if !Arc::ptr_eq(owner, self.connections.work_owner())
                || home != self.home_id
                || generation != self.home_generation
                || service != self.service_generation
            {
                return Err(RuntimeWorkError::Foreign);
            }
        }
        Ok(())
    }

    fn try_validate_shutdown_runtime(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &ShutdownWorkRevision,
    ) -> Result<(), RuntimeWorkError> {
        self.validate_shutdown_source_identity(revision)?;
        let required = &revision.required;
        let connections = &required.connections;
        let stop = &required.controls.stop;
        let compaction = &required.controls.compaction;
        // Two complete reads detect intervening changes; they retain no publication authority.
        for _ in 0..2 {
            self.command_authorizer.try_check_work_open()?;
            if sessions.try_work_revision()? != required.sessions
                || self.stop_coordinator.try_work_revision()? != stop.stamp
                || self
                    .context_compaction
                    .as_ref()
                    .ok_or(RuntimeWorkError::Closed)?
                    .try_work_revision()?
                    != compaction.stamp
                || FlightRegistry::try_work_revision()? != revision.flights
                || registry::try_work_revision()? != revision.loaded
            {
                return Err(RuntimeWorkError::Stale);
            }
            let retained = self.connections.try_work_lock()?;
            let mut work = ConnectionWorkStamp {
                membership: retained.revision().ok_or(RuntimeWorkError::Unavailable)?,
                ..ConnectionWorkStamp::default()
            };
            let mut custody = ConnectionCustodyWorkStamp::default();
            for connection in retained.iter() {
                work.add(connection.try_work_stamp()?)
                    .map_err(|_| RuntimeWorkError::Unavailable)?;
                custody
                    .add(connection.try_custody_work_fact()?)
                    .map_err(|_| RuntimeWorkError::Unavailable)?;
            }
            if work != connections.stamp || custody != revision.connections {
                return Err(RuntimeWorkError::Stale);
            }
            self.command_authorizer.try_check_work_open()?;
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../../tests/unit/shutdown_runtime_validation.rs"]
mod tests;

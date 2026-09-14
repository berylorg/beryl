use super::super::{flight_registry::FlightRegistry, work_sources::ProcessWorkRead};
use super::required::RequiredWorkRevision;
use super::*;
use crate::cas_projection::{
    CasProjectionCoordinator, ScheduledSessionWorkPageLimits,
    connection::{ConnectionCustodyWorkStamp, registry},
};
use syndic_storage::{SelectedPathProof, SyndicPointReadLimit, SyndicReadError};

mod prefix;
#[cfg(all(test, feature = "test-faults"))]
#[path = "../../../../tests/unit/shutdown_work_capture.rs"]
mod tests;
mod types;
use types::Custody;
pub(crate) use types::{
    ShutdownWorkCursor, ShutdownWorkPage, ShutdownWorkRecord, ShutdownWorkRevision,
};

impl ProjectionConnectionService {
    #[cfg(feature = "test-faults")]
    pub fn shutdown_work_first_page_for_test(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Vec<(SyndicThreadId, ProcessWorkFacts)>, ProcessWorkError> {
        let revision = self.shutdown_work_revision(sessions)?;
        let page = self.shutdown_work_page(
            sessions,
            &revision,
            None,
            ProcessWorkPageLimits::new(256, 65_536)?,
            cancellation,
        )?;
        Ok(page
            .records
            .into_iter()
            .map(|row| (row.thread_id, row.work))
            .collect())
    }

    pub(crate) fn shutdown_work_revision(
        &self,
        sessions: &ScheduledExecutionSessions,
    ) -> Result<ShutdownWorkRevision, ProcessWorkError> {
        self.work_read().shutdown_work_revision(sessions)
    }

    pub(crate) fn validate_shutdown_work_revision(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &ShutdownWorkRevision,
    ) -> Result<(), ProcessWorkError> {
        self.work_read()
            .validate_shutdown_work_revision(sessions, revision)
    }

    pub(crate) fn shutdown_work_page(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &ShutdownWorkRevision,
        cursor: Option<&ShutdownWorkCursor>,
        limits: ProcessWorkPageLimits,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownWorkPage, ProcessWorkError> {
        self.work_read().read_shutdown_work_page(
            sessions,
            revision,
            cursor,
            limits,
            cancellation,
            || {},
        )
    }
}

impl ProcessWorkRead {
    fn validate_shutdown_home(&self) -> Result<(), ProcessWorkError> {
        let home = self.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        let coordinator = CasProjectionCoordinator::for_healthy_home(home)?;
        if coordinator.home_id() != self.home_id
            || coordinator.home_generation() != self.home_generation
        {
            return Err(ProcessWorkError::ForeignSources);
        }
        Ok(())
    }

    fn connection_custody_stamp(&self) -> Result<ConnectionCustodyWorkStamp, ProcessWorkError> {
        let connections = self
            .connections
            .lock()
            .map_err(|_| crate::cas_projection::ConnectionWorkError::Poisoned)?
            .clone();
        let mut stamp = ConnectionCustodyWorkStamp::default();
        for connection in &connections {
            stamp.add(connection.custody_work_fact()?)?;
        }
        Ok(stamp)
    }

    fn shutdown_work_revision(
        &self,
        sessions: &ScheduledExecutionSessions,
    ) -> Result<ShutdownWorkRevision, ProcessWorkError> {
        self.validate_shutdown_home()?;
        let revision = ShutdownWorkRevision {
            required: self.required_work_revision(sessions)?,
            flights: FlightRegistry::work_revision()?,
            loaded: registry::work_revision()?,
            connections: self.connection_custody_stamp()?,
        };
        self.validate_shutdown_work_revision(sessions, &revision)?;
        Ok(revision)
    }

    fn validate_shutdown_work_revision(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &ShutdownWorkRevision,
    ) -> Result<(), ProcessWorkError> {
        self.validate_shutdown_home()?;
        self.validate_required_work_revision(sessions, &revision.required)?;
        if FlightRegistry::work_revision()? != revision.flights
            || registry::work_revision()? != revision.loaded
            || self.connection_custody_stamp()? != revision.connections
        {
            return Err(ProcessWorkError::StaleRevision);
        }
        self.validate_required_work_revision(sessions, &revision.required)?;
        self.validate_shutdown_home()
    }

    fn read_shutdown_work_page(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &ShutdownWorkRevision,
        cursor: Option<&ShutdownWorkCursor>,
        limits: ProcessWorkPageLimits,
        cancellation: &ProjectionCancellationToken,
        before_confirmation: impl FnOnce(),
    ) -> Result<ShutdownWorkPage, ProcessWorkError> {
        check_cancelled(cancellation)?;
        self.validate_shutdown_work_revision(sessions, revision)?;
        if cursor.is_some_and(|cursor| cursor.revision != *revision) {
            return Err(ProcessWorkError::ForeignCursor);
        }
        let count = limits
            .max_records
            .min(limits.max_bytes / std::mem::size_of::<ShutdownWorkRecord>());
        if count == 0 {
            return Err(ProcessWorkError::ByteLimit);
        }
        let result = self.collect_shutdown_prefix(
            sessions,
            revision,
            cursor.map(|cursor| cursor.after),
            count,
            cancellation,
        );
        before_confirmation();
        self.validate_shutdown_work_revision(sessions, revision)?;
        check_cancelled(cancellation)?;
        result
    }

    fn collect_shutdown_prefix(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &ShutdownWorkRevision,
        after: Option<SyndicThreadId>,
        count: usize,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<ShutdownWorkPage, ProcessWorkError> {
        let mut selected = prefix::Prefix::new(after, count + 1);
        for (thread, facts) in self.live_facts(sessions, &revision.required, cancellation)? {
            selected.insert(
                thread,
                Custody {
                    work: facts.work,
                    ..Default::default()
                },
            );
        }
        let mut cursor = None;
        loop {
            check_cancelled(cancellation)?;
            let page = sessions.work_page(
                &revision.required.sessions,
                cursor.as_ref(),
                ScheduledSessionWorkPageLimits::new(256, 65_536)?,
            )?;
            for row in page.records() {
                selected.insert(
                    row.thread_id(),
                    Custody {
                        session_registered: row.session().is_some(),
                        preparation_retained: row.preparation().is_some(),
                        ..Default::default()
                    },
                );
            }
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }
        for thread in FlightRegistry::work_prefix(
            self.home_id,
            self.home_generation,
            revision.flights,
            after,
            count + 1,
        )? {
            selected.insert(
                thread,
                Custody {
                    projection_flight: true,
                    ..Default::default()
                },
            );
        }
        let connections = self
            .connections
            .lock()
            .map_err(|_| crate::cas_projection::ConnectionWorkError::Poisoned)?
            .clone();
        for connection in &connections {
            check_cancelled(cancellation)?;
            let generation = connection.custody_work_fact()?.generation();
            for thread in
                registry::loaded_owner_prefix(generation, revision.loaded, after, count + 1)?
            {
                selected.insert(
                    thread,
                    Custody {
                        loaded_projection: true,
                        ..Default::default()
                    },
                );
            }
        }
        let (selected, more) = selected.finish(count);
        let home = self.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        let limit = SyndicPointReadLimit::new(65_536).expect("fixed shutdown capture point limit");
        let mut records = Vec::with_capacity(selected.len());
        for (thread_id, custody) in selected {
            check_cancelled(cancellation)?;
            let thread =
                self.storage
                    .thread(home, thread_id, limit)?
                    .ok_or(SyndicReadError::Invariant(
                        "live shutdown source has no thread",
                    ))?;
            let gate = self.storage.input_gate(home, thread_id, limit)?.ok_or(
                SyndicReadError::Invariant("live shutdown source has no input gate"),
            )?;
            let current_turn_id = gate.state().blocking_turn_id();
            if let Some(turn_id) = current_turn_id {
                let turn =
                    self.storage
                        .turn(home, turn_id, limit)?
                        .ok_or(SyndicReadError::Invariant(
                            "live shutdown source has no current turn",
                        ))?;
                if turn.origin_thread_id() != thread_id {
                    return Err(SyndicReadError::Invariant(
                        "live shutdown source has a foreign current turn",
                    )
                    .into());
                }
            }
            records.push(ShutdownWorkRecord {
                thread_id,
                selected_path: SelectedPathProof::new(
                    thread.committed_tail(),
                    thread.revision(),
                    thread.selected_path_digest(),
                ),
                current_turn_id,
                gate_revision: gate.revision(),
                gate_state: gate.state().clone(),
                work: custody.work,
                session_registered: custody.session_registered,
                preparation_retained: custody.preparation_retained,
                projection_flight: custody.projection_flight,
                loaded_projection: custody.loaded_projection,
            });
        }
        Ok(ShutdownWorkPage {
            next_cursor: more.then(|| ShutdownWorkCursor {
                revision: revision.clone(),
                after: records
                    .last()
                    .expect("nonfinal capture page contains a row")
                    .thread_id,
            }),
            bytes: records.len() * std::mem::size_of::<ShutdownWorkRecord>(),
            records,
            revision: revision.clone(),
        })
    }
}

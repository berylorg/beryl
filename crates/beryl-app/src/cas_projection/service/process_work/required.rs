use beryl_model::DomainRevision;
use syndic_storage::{InputGateState, SyndicPointReadLimit};

use super::super::work_sources::{ProcessWorkRead, ProcessWorkSources};
use super::*;
use crate::cas_projection::{
    ConnectionWorkRevision, ControlWorkRevision, ScheduledSessionFact,
    ScheduledSessionWorkPageLimits, ScheduledSessionWorkRevision,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RequiredWorkRevision {
    pub(super) durable: DomainRevision,
    pub(super) sessions: ScheduledSessionWorkRevision,
    pub(super) connections: ConnectionWorkRevision,
    pub(super) controls: ControlWorkRevision,
}

pub(in crate::cas_projection) struct RequiredSessionWork {
    pub(in crate::cas_projection) thread_id: SyndicThreadId,
    pub(in crate::cas_projection) session: ScheduledSessionFact,
    pub(in crate::cas_projection) facts: ProcessWorkFacts,
}

impl ProcessWorkSources {
    pub(in crate::cas_projection) fn required_session_work(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Vec<RequiredSessionWork>, ProcessWorkError> {
        self.read()?.required_session_work(sessions, cancellation)
    }
}

impl ProcessWorkRead {
    pub(super) fn required_work_revision(
        &self,
        sessions: &ScheduledExecutionSessions,
    ) -> Result<RequiredWorkRevision, ProcessWorkError> {
        let home = self.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        let revision = RequiredWorkRevision {
            durable: self
                .storage
                .revision(home)
                .map_err(syndic_storage::SyndicReadError::from)?,
            sessions: sessions.work_revision()?,
            connections: self.connection_work_revision()?,
            controls: self.control_work_revision()?,
        };
        self.validate_required_work_revision(sessions, &revision)?;
        Ok(revision)
    }

    pub(super) fn validate_required_work_revision(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &RequiredWorkRevision,
    ) -> Result<(), ProcessWorkError> {
        if revision.sessions.home_id() != self.home_id
            || revision.sessions.home_generation() != self.home_generation
            || revision.sessions.service_generation() != self.service_generation
        {
            return Err(ProcessWorkError::ForeignSources);
        }
        let home = self.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        if self
            .storage
            .revision(home)
            .map_err(syndic_storage::SyndicReadError::from)?
            != revision.durable
            || sessions.work_revision()? != revision.sessions
        {
            return Err(ProcessWorkError::StaleRevision);
        }
        self.validate_connection_work_revision(&revision.connections)?;
        self.validate_control_work_revision(&revision.controls)?;
        Ok(())
    }

    fn required_session_work(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<Vec<RequiredSessionWork>, ProcessWorkError> {
        self.collect_required_session_work(
            sessions,
            cancellation,
            #[cfg(test)]
            || {},
        )
    }

    pub(super) fn collect_required_session_work(
        &self,
        sessions: &ScheduledExecutionSessions,
        cancellation: &ProjectionCancellationToken,
        #[cfg(test)] after_read: impl FnOnce(),
    ) -> Result<Vec<RequiredSessionWork>, ProcessWorkError> {
        check_cancelled(cancellation)?;
        let revision = self.required_work_revision(sessions)?;
        let mut live = self.live_facts(sessions, &revision, cancellation)?;
        let home = self.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        let mut cursor = None;
        let mut records = Vec::new();
        loop {
            check_cancelled(cancellation)?;
            let page = sessions.work_page(
                &revision.sessions,
                cursor.as_ref(),
                ScheduledSessionWorkPageLimits::new(256, 65_536)?,
            )?;
            for row in page.records() {
                let Some(session) = row.session() else {
                    continue;
                };
                if records.len() == self.session_capacity {
                    return Err(ProcessWorkError::SourceBoundExceeded);
                }
                check_cancelled(cancellation)?;
                let gate = self
                    .storage
                    .input_gate(
                        home,
                        row.thread_id(),
                        SyndicPointReadLimit::new(65_536).expect("fixed nonzero limit"),
                    )?
                    .ok_or(syndic_storage::SyndicReadError::Invariant(
                        "admitted session has no input gate",
                    ))?;
                let mut facts = live.remove(&row.thread_id()).unwrap_or_default().work;
                facts.merge(gate_work_facts(gate.state()));
                records.push(RequiredSessionWork {
                    thread_id: row.thread_id(),
                    session: session.clone(),
                    facts,
                });
            }
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }
        #[cfg(test)]
        after_read();
        self.validate_required_work_revision(sessions, &revision)?;
        check_cancelled(cancellation)?;
        Ok(records)
    }
}

pub(super) fn gate_work_facts(state: &InputGateState) -> ProcessWorkFacts {
    let mut facts = ProcessWorkFacts::default();
    match state {
        InputGateState::Idle => {}
        InputGateState::PendingTurn(_) => facts.pending = true,
        InputGateState::AwaitingSteering(_) | InputGateState::Steerable(_) => {
            facts.executing = true
        }
        InputGateState::AwaitingTerminal(_) | InputGateState::FinalizingHistory(_) => {
            facts.terminal_settlement = true
        }
        InputGateState::Compacting { .. } => facts.compacting = true,
        InputGateState::Stopping { .. } => facts.stopping = true,
    }
    facts
}

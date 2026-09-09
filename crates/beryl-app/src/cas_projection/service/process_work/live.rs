use super::*;
use crate::cas_projection::{
    CompactionCommandWorkStage, ConnectionTargetWorkState, ConnectionWorkPageLimits,
    ConnectionWorkRecord, ControlWorkPageLimits, ScheduledSessionWorkPageLimits,
    ScheduledSessionWorkState, StopDispatchWorkState, StopWorkRecord,
};

impl ProcessWorkInventory<'_> {
    pub(super) fn live_facts(
        &self,
        revision: &ProcessWorkRevision,
        cancellation: &ProjectionCancellationToken,
    ) -> Result<LiveMap, ProcessWorkError> {
        let mut facts = LiveMap::new();
        let mut cursor = None;
        loop {
            check_cancelled(cancellation)?;
            let page = self.sessions.work_page(
                &revision.sessions,
                cursor.as_ref(),
                ScheduledSessionWorkPageLimits::new(256, 65_536)?,
            )?;
            for row in page.records() {
                let mut work = ProcessWorkFacts::default();
                if let Some(session) = row.session() {
                    match session.state() {
                        ScheduledSessionWorkState::Available => {}
                        ScheduledSessionWorkState::CheckedOut => work.executing = true,
                        ScheduledSessionWorkState::Retiring { .. } => work.cleanup = true,
                    }
                }
                work.preparing = row
                    .preparation()
                    .is_some_and(|preparing| !preparing.is_complete());
                add(&mut facts, row.thread_id(), work);
            }
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }
        let mut cursor = None;
        loop {
            check_cancelled(cancellation)?;
            let page = self.service.connection_work_page(
                &revision.connections,
                cursor.as_ref(),
                ConnectionWorkPageLimits::new(256, 65_536)?,
            )?;
            for row in page.records() {
                let mut work = ProcessWorkFacts::default();
                match row {
                    ConnectionWorkRecord::Target(target) => {
                        match target.state() {
                            ConnectionTargetWorkState::AwaitingStart => work.pending = true,
                            ConnectionTargetWorkState::AwaitingCompactionTurn => {
                                work.compacting = true
                            }
                            ConnectionTargetWorkState::Executing => work.executing = true,
                            ConnectionTargetWorkState::Terminal => work.terminal_settlement = true,
                        }
                        work.cleanup = target.closing().is_some()
                            || target.loss_requested()
                            || target.connection_retired();
                    }
                    ConnectionWorkRecord::Request(_) => work.request_handling = true,
                }
                add(&mut facts, row.identity().thread_id(), work);
            }
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }
        let mut cursor = None;
        loop {
            check_cancelled(cancellation)?;
            let page = self.service.control_work_page(
                &revision.controls,
                cursor.as_ref(),
                ControlWorkPageLimits::new(256, 65_536)?,
            )?;
            for row in page.stop_records() {
                let stopping = match row {
                    StopWorkRecord::Permission(_) => true,
                    StopWorkRecord::Stop(stop) => {
                        stop.primary_custody
                            || stop.driver_custody
                            || stop.local_dispatch.is_some_and(|state| {
                                state != StopDispatchWorkState::DurablyAbandoned
                            })
                    }
                };
                add(
                    &mut facts,
                    row.thread_id(),
                    ProcessWorkFacts {
                        stopping,
                        ..Default::default()
                    },
                );
            }
            for row in page.compaction_records() {
                let mut work = ProcessWorkFacts {
                    continuation: row.continuation.is_some(),
                    ..Default::default()
                };
                if let Some(compaction) = &row.compaction {
                    work.compacting = compaction.command.is_some()
                        || (compaction.local_registered && compaction.result.is_none());
                    work.cleanup = compaction.command == Some(CompactionCommandWorkStage::Cleanup);
                }
                add(&mut facts, row.thread_id(), work);
            }
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }
        check_cancelled(cancellation)?;
        for record in self.attention.work_snapshot(&revision.attention)?.records() {
            if record.home_id() == self.service.home_id {
                facts
                    .entry(record.thread_id())
                    .or_default()
                    .attention
                    .push(record.clone());
            }
        }
        Ok(facts)
    }
}

fn add(facts: &mut LiveMap, thread_id: SyndicThreadId, work: ProcessWorkFacts) {
    if work != ProcessWorkFacts::default() {
        facts.entry(thread_id).or_default().work.merge(work);
    }
}

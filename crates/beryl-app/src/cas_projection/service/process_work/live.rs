use super::super::work_sources::ProcessWorkRead;
use super::required::RequiredWorkRevision;
use super::*;
use crate::cas_projection::{
    CompactionCommandWorkStage, ConnectionTargetWorkState, ConnectionWorkPageLimits,
    ConnectionWorkRecord, ControlWorkPageLimits, ScheduledSessionWorkPageLimits,
    ScheduledSessionWorkState, StopDispatchWorkState, StopWorkRecord,
};
use std::collections::BTreeMap;

impl ProcessWorkInventory<'_> {
    pub(super) fn live_facts<'a>(
        &'a self,
        revision: &'a ProcessWorkRevision,
        cancellation: &'a ProjectionCancellationToken,
    ) -> Result<LiveSource<'a>, ProcessWorkError> {
        Ok(LiveSource::new(move |after| {
            let mut prefix = LivePrefix::new(after);
            self.service.visit_live_facts(
                self.sessions,
                &revision.work,
                cancellation,
                |thread, work| {
                    prefix.add(thread, work, None);
                },
            )?;
            for record in self.attention.work_snapshot(&revision.attention)?.records() {
                check_cancelled(cancellation)?;
                if record.home_id() == self.service.home_id {
                    prefix.add(
                        record.thread_id(),
                        ProcessWorkFacts::default(),
                        Some(record.clone()),
                    );
                }
            }
            Ok(prefix.finish())
        }))
    }
}

impl ProcessWorkRead {
    pub(super) fn live_facts<'a>(
        &'a self,
        sessions: &'a ScheduledExecutionSessions,
        revision: &'a RequiredWorkRevision,
        cancellation: &'a ProjectionCancellationToken,
    ) -> Result<LiveSource<'a>, ProcessWorkError> {
        Ok(LiveSource::new(move |after| {
            let mut prefix = LivePrefix::new(after);
            self.visit_live_facts(sessions, revision, cancellation, |thread, work| {
                prefix.add(thread, work, None);
            })?;
            Ok(prefix.finish())
        }))
    }

    pub(super) fn visit_live_facts(
        &self,
        sessions: &ScheduledExecutionSessions,
        revision: &RequiredWorkRevision,
        cancellation: &ProjectionCancellationToken,
        mut visit: impl FnMut(SyndicThreadId, ProcessWorkFacts),
    ) -> Result<(), ProcessWorkError> {
        let mut cursor = None;
        loop {
            check_cancelled(cancellation)?;
            let page = sessions.work_page(
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
                visit(row.thread_id(), work);
            }
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }
        let mut cursor = None;
        loop {
            check_cancelled(cancellation)?;
            let page = self.connection_work_page(
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
                    ConnectionWorkRecord::Request(request) => {
                        work.request_handling = !request.response().response_written()
                            && request.response().retained_capabilities() != 0;
                    }
                }
                visit(row.identity().thread_id(), work);
            }
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }
        let mut cursor = None;
        loop {
            check_cancelled(cancellation)?;
            let page = self.control_work_page(
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
                visit(
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
                visit(row.thread_id(), work);
            }
            cursor = page.next_cursor().cloned();
            if cursor.is_none() {
                break;
            }
        }
        check_cancelled(cancellation)?;
        Ok(())
    }
}

type LiveEntry = (SyndicThreadId, LiveFacts);
type LivePage = (Vec<LiveEntry>, Option<SyndicThreadId>);

pub(super) struct LiveSource<'a> {
    read: Box<dyn FnMut(Option<SyndicThreadId>) -> Result<LivePage, ProcessWorkError> + 'a>,
    page: std::vec::IntoIter<LiveEntry>,
    cursor: Option<SyndicThreadId>,
    next: Option<LiveEntry>,
    finished: bool,
}

impl<'a> LiveSource<'a> {
    fn new(
        read: impl FnMut(Option<SyndicThreadId>) -> Result<LivePage, ProcessWorkError> + 'a,
    ) -> Self {
        Self {
            read: Box::new(read),
            page: Vec::new().into_iter(),
            cursor: None,
            next: None,
            finished: false,
        }
    }

    pub(super) fn peek(&mut self) -> Result<Option<SyndicThreadId>, ProcessWorkError> {
        while self.next.is_none() {
            self.next = self.page.next();
            if self.next.is_some() || self.finished {
                break;
            }
            let (records, cursor) = (self.read)(self.cursor.take())?;
            self.page = records.into_iter();
            self.finished = cursor.is_none();
            self.cursor = cursor;
        }
        Ok(self.next.as_ref().map(|(id, _)| *id))
    }

    pub(super) fn take(&mut self) -> Option<LiveEntry> {
        self.next.take()
    }
}

struct LivePrefix {
    after: Option<SyndicThreadId>,
    records: BTreeMap<SyndicThreadId, LiveFacts>,
    omitted: bool,
}

impl LivePrefix {
    fn new(after: Option<SyndicThreadId>) -> Self {
        Self {
            after,
            records: BTreeMap::new(),
            omitted: false,
        }
    }

    fn add(
        &mut self,
        thread: SyndicThreadId,
        work: ProcessWorkFacts,
        attention: Option<LifecycleAttentionRecord>,
    ) {
        if self.after.is_some_and(|after| thread <= after)
            || (work == ProcessWorkFacts::default() && attention.is_none())
        {
            return;
        }
        let row = self.records.entry(thread).or_default();
        row.work.merge(work);
        if let Some(attention) = attention {
            row.attention.push(attention);
        }
        if self.records.len() > 256 {
            self.records.pop_last();
            self.omitted = true;
        }
    }

    fn finish(self) -> LivePage {
        let cursor = self.omitted.then(|| {
            *self
                .records
                .last_key_value()
                .expect("a nonfinal prefix has rows")
                .0
        });
        (self.records.into_iter().collect(), cursor)
    }
}

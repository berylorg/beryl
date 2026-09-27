use beryl_home_store::CursorReadLimits;
use syndic_storage::{InputGateState, SyndicPointReadLimit};

use super::super::{flight_registry::FlightRegistry, work_sources::ProcessWorkRead};
use super::required::RequiredWorkRevision;
use super::*;

type Entry = (SyndicThreadId, ProcessWorkFacts);

#[cfg(all(test, feature = "test-faults"))]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/process_work_source_merge.rs"
    ));
}

struct Source<C, F> {
    cursor: Option<C>,
    page: std::vec::IntoIter<Entry>,
    next: Option<Entry>,
    finished: bool,
    read: F,
}

impl<C, F> Source<C, F>
where
    F: FnMut(Option<C>) -> Result<(Vec<Entry>, Option<C>), ProcessWorkError>,
{
    fn new(read: F) -> Self {
        Self {
            cursor: None,
            page: Vec::new().into_iter(),
            next: None,
            finished: false,
            read,
        }
    }

    fn peek(&mut self) -> Result<Option<SyndicThreadId>, ProcessWorkError> {
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

    fn consume(
        &mut self,
        thread_id: SyndicThreadId,
        work: &mut ProcessWorkFacts,
    ) -> Result<(), ProcessWorkError> {
        while self.peek()? == Some(thread_id) {
            work.merge(self.next.take().expect("peek selected this entry").1);
        }
        Ok(())
    }
}

impl ProcessWorkInventory<'_> {
    pub(super) fn scan_threads(
        &self,
        revision: &ProcessWorkRevision,
        live: LiveMap,
        cancellation: &ProjectionCancellationToken,
        visit: impl FnMut(SyndicThreadId, LiveFacts) -> Result<(), ProcessWorkError>,
    ) -> Result<(), ProcessWorkError> {
        self.service
            .scan_work_threads(&revision.work, live, None, cancellation, visit)
    }
}

impl ProcessWorkRead {
    pub(super) fn scan_work_threads(
        &self,
        revision: &RequiredWorkRevision,
        live: LiveMap,
        flight_revision: Option<u64>,
        cancellation: &ProjectionCancellationToken,
        mut visit: impl FnMut(SyndicThreadId, LiveFacts) -> Result<(), ProcessWorkError>,
    ) -> Result<(), ProcessWorkError> {
        let storage = &self.storage;
        let home = self.home.as_deref().ok_or(ProcessWorkError::Closed)?;
        let limits = CursorReadLimits::new(256, 65_536).expect("fixed nonzero limits");
        let point_limit = SyndicPointReadLimit::new(65_536).expect("fixed nonzero limit");
        let mut gates = Source::new(|cursor| {
            check_cancelled(cancellation)?;
            let page = storage.non_idle_gate_source_page(home, revision.durable, cursor, limits)?;
            let mut rows = Vec::new();
            for source in page.records() {
                check_cancelled(cancellation)?;
                let gate = storage.resolve_non_idle_gate_source(
                    home,
                    revision.durable,
                    *source,
                    point_limit,
                )?;
                if matches!(gate.state(), InputGateState::Idle) {
                    return Err(syndic_storage::SyndicReadError::Invariant(
                        "non-idle source resolved an idle gate",
                    )
                    .into());
                }
                rows.push((source.thread_id(), required::gate_work_facts(gate.state())));
            }
            Ok((rows, page.next_cursor()))
        });
        let mut next = Source::new(|cursor| {
            check_cancelled(cancellation)?;
            let page = storage.accepted_next_source_page(home, revision.durable, cursor, limits)?;
            Ok((
                page.records()
                    .iter()
                    .map(|row| {
                        (
                            row.thread_id(),
                            ProcessWorkFacts {
                                queued: true,
                                ..Default::default()
                            },
                        )
                    })
                    .collect(),
                page.next_cursor(),
            ))
        });
        let mut ready = Source::new(|cursor| {
            check_cancelled(cancellation)?;
            let page =
                storage.accepted_ready_source_page(home, revision.durable, cursor, limits)?;
            Ok((
                page.records()
                    .iter()
                    .map(|row| {
                        (
                            row.thread_id(),
                            ProcessWorkFacts {
                                queued: true,
                                ..Default::default()
                            },
                        )
                    })
                    .collect(),
                page.next_cursor(),
            ))
        });
        let mut flights = Source::new(|cursor| {
            check_cancelled(cancellation)?;
            let Some(revision) = flight_revision else {
                return Ok((Vec::new(), None));
            };
            let threads = FlightRegistry::work_prefix(
                self.home_id,
                self.home_generation,
                revision,
                cursor,
                256,
            )?;
            let next = (threads.len() == 256).then(|| *threads.last().unwrap());
            Ok((
                threads
                    .into_iter()
                    .map(|thread| {
                        (
                            thread,
                            ProcessWorkFacts {
                                terminal_settlement: true,
                                ..Default::default()
                            },
                        )
                    })
                    .collect(),
                next,
            ))
        });
        let mut live = live.into_iter().peekable();
        loop {
            check_cancelled(cancellation)?;
            let thread_id = [
                gates.peek()?,
                next.peek()?,
                ready.peek()?,
                flights.peek()?,
                live.peek().map(|(id, _)| *id),
            ]
            .into_iter()
            .flatten()
            .min();
            let Some(thread_id) = thread_id else {
                break;
            };
            let mut facts = if live.peek().is_some_and(|(id, _)| *id == thread_id) {
                live.next().expect("peek selected this live entry").1
            } else {
                LiveFacts::default()
            };
            gates.consume(thread_id, &mut facts.work)?;
            next.consume(thread_id, &mut facts.work)?;
            ready.consume(thread_id, &mut facts.work)?;
            flights.consume(thread_id, &mut facts.work)?;
            visit(thread_id, facts)?;
        }
        Ok(())
    }
}

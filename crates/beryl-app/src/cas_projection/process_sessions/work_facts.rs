use std::{mem::size_of, ops::Bound};

use super::*;

mod types;
pub use types::*;

impl ScheduledExecutionSessions {
    pub fn work_revision(&self) -> Result<ScheduledSessionWorkRevision, ScheduledSessionWorkError> {
        let state = self
            .state
            .lock()
            .map_err(|_| ScheduledSessionWorkError::Poisoned)?;
        self.current_work_revision(&state)
    }

    pub fn work_page(
        &self,
        expected: &ScheduledSessionWorkRevision,
        cursor: Option<&ScheduledSessionWorkCursor>,
        limits: ScheduledSessionWorkPageLimits,
    ) -> Result<ScheduledSessionWorkPage, ScheduledSessionWorkError> {
        if !Arc::ptr_eq(&self.work_identity, &expected.owner)
            || cursor.is_some_and(|cursor| &cursor.revision != expected)
        {
            return Err(ScheduledSessionWorkError::ForeignCursor);
        }
        let state = self
            .state
            .lock()
            .map_err(|_| ScheduledSessionWorkError::Poisoned)?;
        if self.current_work_revision(&state)? != *expected {
            return Err(ScheduledSessionWorkError::StaleRevision);
        }
        let start = cursor.map_or(Bound::Unbounded, |cursor| Bound::Excluded(cursor.after));
        let mut slots = state.slots.range((start, Bound::Unbounded)).peekable();
        let mut preparing = state.preparing.range((start, Bound::Unbounded)).peekable();
        let mut records = Vec::new();
        let mut bytes = 0_usize;
        let mut has_more = false;
        loop {
            let thread_id = match (slots.peek(), preparing.peek()) {
                (Some((left, _)), Some((right, _))) => **left.min(right),
                (Some((thread, _)), None) | (None, Some((thread, _))) => **thread,
                (None, None) => break,
            };
            if records.len() == limits.max_records {
                has_more = true;
                break;
            }
            let slot = slots
                .peek()
                .filter(|(id, _)| **id == thread_id)
                .map(|(_, slot)| *slot);
            let preparation = preparing
                .peek()
                .filter(|(id, _)| **id == thread_id)
                .map(|(_, worker)| *worker);
            let record_bytes = size_of::<ScheduledSessionWorkRecord>()
                .checked_add(slot.map_or(0, |slot| slot.binding.root_path().as_str().len()))
                .and_then(|bytes| {
                    bytes.checked_add(
                        preparation.map_or(0, |worker| worker.binding.root_path().as_str().len()),
                    )
                })
                .ok_or(ScheduledSessionWorkError::ByteLimit)?;
            let next_bytes = bytes
                .checked_add(record_bytes)
                .ok_or(ScheduledSessionWorkError::ByteLimit)?;
            if next_bytes > limits.max_bytes {
                if records.is_empty() {
                    return Err(ScheduledSessionWorkError::ByteLimit);
                }
                has_more = true;
                break;
            }
            records.push(ScheduledSessionWorkRecord {
                thread_id,
                session: slot.map(|slot| ScheduledSessionFact {
                    registration_serial: slot.registration.serial,
                    binding: slot.binding.clone(),
                    state: if slot.retiring {
                        ScheduledSessionWorkState::Retiring {
                            checked_out: slot.checked_out,
                        }
                    } else if slot.checked_out {
                        ScheduledSessionWorkState::CheckedOut
                    } else {
                        ScheduledSessionWorkState::Available
                    },
                }),
                preparation: preparation.map(|worker| ScheduledSessionPreparationFact {
                    binding: worker.binding.clone(),
                    complete: worker.complete,
                }),
            });
            bytes = next_bytes;
            if slot.is_some() {
                slots.next();
            }
            if preparation.is_some() {
                preparing.next();
            }
        }
        if self.current_work_revision(&state)? != *expected {
            return Err(ScheduledSessionWorkError::StaleRevision);
        }
        let next_cursor = has_more.then(|| ScheduledSessionWorkCursor {
            revision: expected.clone(),
            after: records
                .last()
                .expect("a nonfinal work page contains a record")
                .thread_id,
        });
        Ok(ScheduledSessionWorkPage {
            revision: expected.clone(),
            records,
            bytes,
            next_cursor,
        })
    }

    fn current_work_revision(
        &self,
        state: &SessionState,
    ) -> Result<ScheduledSessionWorkRevision, ScheduledSessionWorkError> {
        let context = state
            .context
            .as_ref()
            .ok_or(ScheduledSessionWorkError::Unattached)?;
        if state.closed || !context.commands.is_open() {
            return Err(ScheduledSessionWorkError::Closed);
        }
        Ok(ScheduledSessionWorkRevision {
            owner: Arc::clone(&self.work_identity),
            home_id: context.home_id,
            home_generation: context.home_generation,
            service_generation: context.service_generation,
            revision: state
                .work_revision
                .ok_or(ScheduledSessionWorkError::RevisionExhausted)?,
        })
    }
}

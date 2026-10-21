use crate::generated_output::{GeneratedOutputError, GeneratedReaderAllowance, lock};

#[cfg(test)]
#[path = "../../tests/unit/generated_readers.rs"]
mod tests;
use beryl_wsl_supervisor::{Frame, Nonce, ReaderFailure, ReaderId, encode_frame};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{SyncSender, TrySendError},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct ReaderRegistry {
    state: Mutex<RegistryState>,
}
#[derive(Default)]
struct RegistryState {
    limit: Option<u32>,
    next_id: u128,
    namespace_closed: bool,
    joined: bool,
    failed: bool,
    readers: BTreeMap<ReaderId, Arc<ReaderSlot>>,
}
struct ReaderSlot {
    id: ReaderId,
    state: Mutex<SlotState>,
    event: Condvar,
}
#[derive(Clone, Copy, Debug)]
enum Pending {
    Open,
    Read {
        sequence: u64,
        offset: u64,
        allowance: u32,
    },
    Close {
        sequence: u64,
    },
}
struct SlotState {
    pending: Pending,
    opening: bool,
    request_in_flight: bool,
    previous_read: Option<(u64, u64, u32)>,
    reply: Option<Frame>,
    error: Option<GeneratedOutputError>,
    automatic_close: Option<u64>,
    automatic_closed: bool,
    close_sent: bool,
    length: Option<u64>,
    next_sequence: u64,
    closed: bool,
    failed: bool,
}

impl std::fmt::Debug for ReaderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = lock(&self.state);
        f.debug_struct("ReaderRegistry")
            .field("limit", &state.limit)
            .field("readers", &state.readers.len())
            .field("namespace_closed", &state.namespace_closed)
            .field("joined", &state.joined)
            .field("failed", &state.failed)
            .finish()
    }
}
impl std::fmt::Debug for ReaderSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = lock(&self.state);
        f.debug_struct("ReaderSlot")
            .field("id", &self.id)
            .field("pending", &state.pending)
            .field("reply_pending", &state.reply.is_some())
            .field("error", &state.error)
            .field("closed", &state.closed)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct WslGeneratedReader {
    registry: Arc<ReaderRegistry>,
    slot: Arc<ReaderSlot>,
    sender: Arc<Mutex<Option<SyncSender<Vec<u8>>>>>,
    nonce: Nonce,
    operation: Mutex<()>,
}

fn failure(kind: ReaderFailure) -> GeneratedOutputError {
    match kind {
        ReaderFailure::Refused => GeneratedOutputError::Busy,
        ReaderFailure::Missing => GeneratedOutputError::Missing,
        ReaderFailure::Unreadable => GeneratedOutputError::Io,
        ReaderFailure::Changed => GeneratedOutputError::Changed,
        ReaderFailure::Protocol => GeneratedOutputError::Protocol,
        ReaderFailure::Cancelled => GeneratedOutputError::Cancelled,
    }
}

impl ReaderRegistry {
    pub(crate) fn configure(
        &self,
        allowance: GeneratedReaderAllowance,
    ) -> Result<bool, GeneratedOutputError> {
        let mut state = lock(&self.state);
        if state.namespace_closed || state.failed {
            return Err(GeneratedOutputError::LostSourceAuthority);
        }
        match state.limit {
            Some(limit) if limit == allowance.0 => Ok(false),
            Some(_) => Err(GeneratedOutputError::Busy),
            None => {
                state.limit = Some(allowance.0);
                Ok(true)
            }
        }
    }
    pub(crate) fn unconfigure(&self) {
        let mut state = lock(&self.state);
        if state.readers.is_empty() {
            state.limit = None;
        }
    }
    pub(crate) fn reserve(
        self: &Arc<Self>,
        sender: Arc<Mutex<Option<SyncSender<Vec<u8>>>>>,
        nonce: Nonce,
    ) -> Result<WslGeneratedReader, GeneratedOutputError> {
        let mut state = lock(&self.state);
        if state.namespace_closed || state.failed {
            return Err(GeneratedOutputError::LostSourceAuthority);
        }
        if state.readers.len() >= state.limit.unwrap_or(0) as usize {
            return Err(GeneratedOutputError::Busy);
        }
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or(GeneratedOutputError::Protocol)?;
        let id = state.next_id.to_be_bytes();
        let slot = Arc::new(ReaderSlot {
            id,
            state: Mutex::new(SlotState {
                pending: Pending::Open,
                opening: true,
                request_in_flight: false,
                previous_read: None,
                reply: None,
                error: None,
                automatic_close: None,
                automatic_closed: false,
                close_sent: false,
                length: None,
                next_sequence: 1,
                closed: false,
                failed: false,
            }),
            event: Condvar::new(),
        });
        state.readers.insert(id, slot.clone());
        Ok(WslGeneratedReader {
            registry: self.clone(),
            slot,
            sender,
            nonce,
            operation: Mutex::new(()),
        })
    }
    pub(crate) fn route(&self, frame: Frame) -> Result<Option<Frame>, GeneratedOutputError> {
        let id = match &frame {
            Frame::ReaderOpened { id, .. }
            | Frame::ReaderPage { id, .. }
            | Frame::ReaderFailed { id, .. }
            | Frame::ReaderClosed { id, .. } => *id,
            Frame::OwnedNamespaceClosed { .. } => {
                self.namespace_closed();
                return Ok(Some(frame));
            }
            _ => return Ok(Some(frame)),
        };
        let slot = lock(&self.state)
            .readers
            .get(&id)
            .cloned()
            .ok_or(GeneratedOutputError::Protocol)?;
        let mut state = lock(&slot.state);
        let valid = match (&frame, state.pending) {
            (Frame::ReaderOpened { length, .. }, Pending::Open | Pending::Close { .. }) => {
                if *length == 0 || state.reply.is_some() || !state.opening {
                    false
                } else {
                    state.length = Some(*length);
                    state.opening = false;
                    true
                }
            }
            (
                Frame::ReaderPage {
                    sequence,
                    offset,
                    bytes,
                    eof,
                    ..
                },
                Pending::Read {
                    sequence: expected,
                    offset: position,
                    allowance,
                },
            ) => {
                state.reply.is_none()
                    && *sequence == expected
                    && *offset == position
                    && bytes.len() <= allowance as usize
                    && offset.checked_add(bytes.len() as u64).is_some_and(|end| {
                        state.length.is_some_and(|length| {
                            end <= length
                                && if *eof {
                                    bytes.is_empty() && *offset == length
                                } else {
                                    !bytes.is_empty()
                                }
                        })
                    })
            }
            (
                Frame::ReaderPage {
                    sequence,
                    offset,
                    bytes,
                    eof,
                    ..
                },
                Pending::Close { .. },
            ) => state
                .previous_read
                .is_some_and(|(expected, position, allowance)| {
                    *sequence == expected
                        && *offset == position
                        && bytes.len() <= allowance as usize
                        && offset.checked_add(bytes.len() as u64).is_some_and(|end| {
                            state.length.is_some_and(|length| {
                                end <= length
                                    && if *eof {
                                        bytes.is_empty() && *offset == length
                                    } else {
                                        !bytes.is_empty()
                                    }
                            })
                        })
                }),
            (Frame::ReaderFailed { sequence, .. }, Pending::Open) => {
                *sequence == 0 && state.reply.is_none()
            }
            (
                Frame::ReaderFailed { sequence, .. },
                Pending::Read {
                    sequence: expected, ..
                },
            ) => *sequence == expected && state.reply.is_none(),
            (Frame::ReaderFailed { sequence, .. }, Pending::Close { sequence: expected }) => {
                *sequence == expected
                    || (state.opening && *sequence == 0)
                    || state
                        .previous_read
                        .is_some_and(|(previous, _, _)| *sequence == previous)
            }
            (Frame::ReaderClosed { sequence, .. }, Pending::Close { sequence: expected }) => {
                *sequence == expected || state.automatic_close == Some(*sequence)
            }
            (Frame::ReaderClosed { sequence, .. }, _) => state.automatic_close == Some(*sequence),
            _ => false,
        };
        if !valid || state.closed {
            state.failed = true;
            slot.event.notify_all();
            return Err(GeneratedOutputError::Protocol);
        }
        if matches!(frame, Frame::ReaderPage { .. }) {
            state.request_in_flight = false;
        }
        if let Frame::ReaderClosed { sequence, .. } = &frame {
            state.automatic_closed = state.automatic_close == Some(*sequence);
            state.closed = matches!(state.pending,Pending::Close{sequence:expected} if state.close_sent && *sequence==expected);
            state.reply = None;
        } else if let Frame::ReaderFailed { sequence, kind, .. } = frame {
            state.error = Some(failure(kind));
            state.automatic_close = Some(sequence);
        } else if !matches!(state.pending, Pending::Close { .. }) {
            state.reply = Some(frame);
        }
        drop(state);
        slot.event.notify_all();
        Ok(None)
    }
    pub(crate) fn fail(&self) {
        let mut state = lock(&self.state);
        state.failed = true;
        for slot in state.readers.values() {
            lock(&slot.state).failed = true;
            slot.event.notify_all();
        }
    }
    fn namespace_closed(&self) {
        let mut state = lock(&self.state);
        state.namespace_closed = true;
        for slot in state.readers.values() {
            slot.event.notify_all();
        }
    }
    pub(crate) fn joined(&self) {
        let mut state = lock(&self.state);
        state.joined = true;
        for slot in state.readers.values() {
            slot.event.notify_all();
        }
    }
}

mod requests;

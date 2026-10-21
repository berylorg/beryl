use super::{reader_transport as transport, sys};
use crate::{Frame, ReaderFailure, ReaderId};
use std::collections::BTreeMap;
use std::io;
use std::os::fd::RawFd;

pub(super) struct ReaderControl {
    limit: Option<u32>,
    highest: ReaderId,
    readers: BTreeMap<ReaderId, Operation>,
}

enum Operation {
    Waiting,
    Ready,
    Closing {
        sequence: u64,
    },
    Failed,
    Closed {
        sequence: u64,
        pending: Option<u64>,
        reusable: bool,
    },
}

impl ReaderControl {
    pub(super) fn new() -> Self {
        Self {
            limit: None,
            highest: [0; 16],
            readers: BTreeMap::new(),
        }
    }

    pub(super) fn request(
        &mut self,
        fd: RawFd,
        output: &mut sys::Output,
        frame: Frame,
    ) -> io::Result<()> {
        match &frame {
            Frame::ConfigureReaders { limit } if *limit != 0 && self.limit.is_none() => {
                self.limit = Some(*limit);
                output.configure_readers(*limit);
            }
            Frame::ReaderOpen { id, .. } if *id > self.highest && self.limit.is_some() => {
                self.highest = *id;
                if self
                    .readers
                    .values()
                    .filter(|operation| {
                        !matches!(operation, Operation::Closed { reusable: true, .. })
                    })
                    .count() as u64
                    >= u64::from(self.limit.unwrap())
                {
                    output.push(Frame::ReaderFailed {
                        id: *id,
                        sequence: 0,
                        kind: ReaderFailure::Refused,
                    });
                    output.push(Frame::ReaderClosed {
                        id: *id,
                        sequence: 0,
                        cancelled: true,
                    });
                    return Ok(());
                }
                if self.readers.len() as u64 >= u64::from(self.limit.unwrap()) {
                    let terminal = self
                        .readers
                        .iter()
                        .find_map(|(id, operation)| {
                            matches!(operation, Operation::Closed { reusable: true, .. })
                                .then_some(*id)
                        })
                        .ok_or_else(|| io::Error::other("missing terminal reader slot"))?;
                    self.readers.remove(&terminal);
                }
                self.readers.insert(*id, Operation::Waiting);
            }
            Frame::ReaderRead { id, .. }
                if matches!(self.readers.get(id), Some(Operation::Ready))
                    && !output.reader_pending(id) =>
            {
                self.readers.insert(*id, Operation::Waiting);
            }
            Frame::ReaderClose { id, sequence } | Frame::ReaderCancel { id, sequence }
                if self.readers.contains_key(id) =>
            {
                if let Some(Operation::Closed {
                    sequence: previous,
                    pending,
                    ..
                }) = self.readers.get(id)
                {
                    if previous.checked_add(1) != Some(*sequence) || *sequence == 0 {
                        return Err(io::Error::other("replayed terminal reader close"));
                    }
                    if pending.is_some() {
                        return Err(io::Error::other("duplicate pending terminal close"));
                    }
                }
                self.readers.insert(
                    *id,
                    Operation::Closing {
                        sequence: *sequence,
                    },
                );
            }
            _ => return Err(io::Error::other("illegal original reader request")),
        }
        if !transport::send(fd, &frame)? {
            return Err(io::Error::other("reader command channel full"));
        }
        Ok(())
    }

    pub(super) fn pump(&mut self, fd: RawFd, output: &mut sys::Output) -> io::Result<bool> {
        for _ in 0..8 {
            let frame = match transport::receive(fd)? {
                transport::Received::Pending => break,
                transport::Received::Closed => return Ok(true),
                transport::Received::Frame(frame) => frame,
            };
            let id = match &frame {
                Frame::ReaderOpened { id, .. }
                | Frame::ReaderPage { id, .. }
                | Frame::ReaderFailed { id, .. }
                | Frame::ReaderClosed { id, .. } => *id,
                _ => return Err(io::Error::other("illegal original reader response")),
            };
            if !self.readers.contains_key(&id) {
                return Err(io::Error::other("unknown original reader response"));
            }
            if let Frame::ReaderClosed { sequence, .. } = frame {
                let pending = match self.readers.get(&id) {
                    Some(Operation::Closing { sequence }) => Some(*sequence),
                    Some(Operation::Closed { pending, .. }) => *pending,
                    _ => None,
                };
                let reusable = pending == Some(sequence);
                self.readers.insert(
                    id,
                    Operation::Closed {
                        sequence,
                        pending: pending.filter(|_| !reusable),
                        reusable,
                    },
                );
            } else if matches!(frame, Frame::ReaderFailed { .. }) {
                if !matches!(self.readers.get(&id), Some(Operation::Closing { .. })) {
                    self.readers.insert(id, Operation::Failed);
                }
            } else if !matches!(
                self.readers.get(&id),
                Some(Operation::Closing { .. } | Operation::Failed)
            ) {
                self.readers.insert(id, Operation::Ready);
            }
            output.reader(id, frame)?;
        }
        Ok(false)
    }

    pub(super) fn drain_closed(&mut self, fd: RawFd, output: &mut sys::Output) -> io::Result<bool> {
        output.abandon_readers();
        for _ in 0..8 {
            match transport::receive(fd)? {
                transport::Received::Closed => {
                    self.readers.clear();
                    return Ok(true);
                }
                transport::Received::Pending => return Ok(false),
                transport::Received::Frame(_) => {}
            }
        }
        Ok(false)
    }
}

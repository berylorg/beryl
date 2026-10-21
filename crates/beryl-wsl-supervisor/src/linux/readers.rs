use super::{context::Context, reader_transport as transport, reader_worker, sys};
use crate::{Frame, MAX_PENDING_FRAMES, ReaderFailure, ReaderId};
use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};

enum Operation {
    Opening,
    Idle,
    Reading {
        sequence: u64,
        offset: u64,
        allowance: u32,
    },
    Closing,
}

struct Reader {
    id: ReaderId,
    pid: i32,
    pidfd: OwnedFd,
    channel: OwnedFd,
    operation: Operation,
    sequence: u64,
    length: u64,
    pending: Option<Frame>,
    failed: Option<ReaderFailure>,
    cancelled: bool,
    reaped: bool,
    drained: bool,
    close_requested: bool,
}

struct Terminal {
    id: ReaderId,
    sequence: u64,
    reusable: bool,
}

impl Reader {
    fn terminate(&mut self, failure: Option<ReaderFailure>, cancelled: bool) -> io::Result<()> {
        self.operation = Operation::Closing;
        self.pending = None;
        self.failed = failure;
        self.cancelled = cancelled;
        match sys::signal(self.pidfd.as_raw_fd(), libc::SIGKILL) {
            Ok(()) => Ok(()),
            Err(error) if error.raw_os_error() == Some(libc::ESRCH) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn receive(&mut self) -> io::Result<()> {
        if self.drained || (self.pending.is_some() && !matches!(self.operation, Operation::Closing))
        {
            return Ok(());
        }
        let received = match transport::receive(self.channel.as_raw_fd()) {
            Err(error)
                if error.kind() == io::ErrorKind::ConnectionReset
                    && matches!(self.operation, Operation::Closing) =>
            {
                return Ok(());
            }
            result => result?,
        };
        match received {
            transport::Received::Pending => {}
            transport::Received::Closed => {
                self.drained = true;
                if !matches!(self.operation, Operation::Closing) {
                    self.terminate(Some(ReaderFailure::Protocol), true)?;
                }
            }
            transport::Received::Frame(frame) => {
                if matches!(self.operation, Operation::Closing) {
                    return Ok(());
                }
                let valid = match (&self.operation, &frame) {
                    (Operation::Opening, Frame::ReaderOpened { id, length })
                        if *id == self.id && *length != 0 =>
                    {
                        self.length = *length;
                        true
                    }
                    (
                        Operation::Reading {
                            sequence,
                            offset,
                            allowance,
                        },
                        Frame::ReaderPage {
                            id,
                            sequence: actual_sequence,
                            offset: actual_offset,
                            bytes,
                            eof,
                        },
                    ) => {
                        *id == self.id
                            && sequence == actual_sequence
                            && offset == actual_offset
                            && bytes.len() <= *allowance as usize
                            && offset
                                .checked_add(bytes.len() as u64)
                                .is_some_and(|end| end <= self.length)
                            && ((!*eof && !bytes.is_empty())
                                || (*eof && bytes.is_empty() && *offset == self.length))
                    }
                    (
                        Operation::Opening | Operation::Reading { .. },
                        Frame::ReaderFailed { id, sequence, .. },
                    ) => *id == self.id && *sequence == self.sequence,
                    _ => false,
                };
                if !valid {
                    self.terminate(Some(ReaderFailure::Protocol), true)?;
                } else if let Frame::ReaderFailed { kind, .. } = frame {
                    self.terminate(Some(kind), true)?;
                } else {
                    self.pending = Some(frame);
                }
            }
        }
        Ok(())
    }
}

pub(super) struct Readers {
    limit: Option<u32>,
    highest: ReaderId,
    entries: Vec<Reader>,
    rejected: VecDeque<Frame>,
    terminal: Vec<Terminal>,
}

impl Readers {
    pub(super) fn new() -> Self {
        Self {
            limit: None,
            highest: [0; 16],
            entries: Vec::new(),
            rejected: VecDeque::new(),
            terminal: Vec::new(),
        }
    }

    pub(super) fn reaped(&mut self, pid: i32) {
        if let Some(reader) = self.entries.iter_mut().find(|reader| reader.pid == pid) {
            reader.reaped = true;
        }
    }

    fn reject(&mut self, id: ReaderId) -> io::Result<()> {
        if self.rejected.len() + 2 > MAX_PENDING_FRAMES {
            return Err(io::Error::other("reader refusal bound"));
        }
        self.rejected.push_back(Frame::ReaderFailed {
            id,
            sequence: 0,
            kind: ReaderFailure::Refused,
        });
        self.rejected.push_back(Frame::ReaderClosed {
            id,
            sequence: 0,
            cancelled: true,
        });
        if self.limit.is_some_and(|limit| {
            ((self.entries.len() + self.terminal.len()) as u64) < u64::from(limit)
        }) {
            self.terminal.push(Terminal {
                id,
                sequence: 0,
                reusable: false,
            });
        }
        Ok(())
    }

    pub(super) fn request(&mut self, context: &Context, frame: Frame) -> io::Result<()> {
        match frame {
            Frame::ConfigureReaders { limit } if limit != 0 && self.limit.is_none() => {
                self.limit = Some(limit)
            }
            Frame::ReaderOpen { id, path } => {
                if id <= self.highest {
                    return Err(io::Error::other("reused reader identity"));
                }
                self.highest = id;
                if self.limit.is_none_or(|limit| {
                    (self.entries.len()
                        + self
                            .terminal
                            .iter()
                            .filter(|terminal| !terminal.reusable)
                            .count()) as u64
                        >= u64::from(limit)
                }) {
                    return self.reject(id);
                }
                if self.limit.is_some_and(|limit| {
                    (self.entries.len() + self.terminal.len()) as u64 >= u64::from(limit)
                }) {
                    let reusable = self
                        .terminal
                        .iter()
                        .position(|terminal| terminal.reusable)
                        .ok_or_else(|| io::Error::other("missing reusable terminal reader slot"))?;
                    self.terminal.remove(reusable);
                }
                let (parent, child) = match transport::pair() {
                    Ok(pair) => pair,
                    Err(_) => return self.reject(id),
                };
                sys::nonblocking(parent.as_raw_fd())?;
                let (pidfd, pid) = match sys::clone_pidfd(false) {
                    Ok(sys::Spawn::Child) => {
                        reader_worker::run(context, id, &path, child.as_raw_fd())
                    }
                    Ok(sys::Spawn::Parent { pidfd, pid }) => (pidfd, pid),
                    Err(_) => return self.reject(id),
                };
                self.entries.push(Reader {
                    id,
                    pid,
                    pidfd,
                    channel: parent,
                    operation: Operation::Opening,
                    sequence: 0,
                    length: 0,
                    pending: None,
                    failed: None,
                    cancelled: false,
                    reaped: false,
                    drained: false,
                    close_requested: false,
                });
                drop(child);
                let reader = self.entries.last_mut().unwrap();
                if !matches!(sys::write(reader.channel.as_raw_fd(), b"G"), Ok(1)) {
                    reader.terminate(Some(ReaderFailure::Unreadable), true)?;
                }
            }
            Frame::ReaderRead {
                id,
                sequence,
                offset,
                allowance,
            } => {
                let reader = self
                    .entries
                    .iter_mut()
                    .find(|reader| reader.id == id)
                    .ok_or_else(|| io::Error::other("unknown reader"))?;
                if reader.sequence.checked_add(1) != Some(sequence)
                    || sequence == 0
                    || !matches!(reader.operation, Operation::Idle)
                    || reader.pending.is_some()
                {
                    reader.terminate(Some(ReaderFailure::Protocol), true)?;
                } else {
                    reader.sequence = sequence;
                    let frame = Frame::ReaderRead {
                        id,
                        sequence,
                        offset,
                        allowance,
                    };
                    if transport::send(reader.channel.as_raw_fd(), &frame)? {
                        reader.operation = Operation::Reading {
                            sequence,
                            offset,
                            allowance,
                        };
                    } else {
                        reader.terminate(Some(ReaderFailure::Protocol), true)?;
                    }
                }
            }
            Frame::ReaderCancel { id, sequence } | Frame::ReaderClose { id, sequence } => {
                let cancelled = matches!(frame, Frame::ReaderCancel { .. });
                if let Some(terminal) = self.terminal.iter_mut().find(|terminal| terminal.id == id)
                {
                    if terminal.sequence.checked_add(1) != Some(sequence) || sequence == 0 {
                        return Err(io::Error::other("replayed terminal reader close"));
                    }
                    if self.rejected.len() == MAX_PENDING_FRAMES {
                        return Err(io::Error::other("terminal reader receipt bound"));
                    }
                    terminal.sequence = sequence;
                    terminal.reusable = true;
                    self.rejected.push_back(Frame::ReaderClosed {
                        id,
                        sequence,
                        cancelled,
                    });
                    return Ok(());
                }
                let reader = self
                    .entries
                    .iter_mut()
                    .find(|reader| reader.id == id)
                    .ok_or_else(|| io::Error::other("unknown reader"))?;
                if reader.sequence.checked_add(1) != Some(sequence) || sequence == 0 {
                    reader.terminate(Some(ReaderFailure::Protocol), true)?;
                } else {
                    reader.sequence = sequence;
                    reader.close_requested = true;
                    reader.terminate(None, cancelled)?;
                }
            }
            _ => return Err(io::Error::other("illegal reader request")),
        }
        Ok(())
    }

    pub(super) fn pump(&mut self, channel: RawFd) -> io::Result<()> {
        while let Some(frame) = self.rejected.front() {
            if !transport::send(channel, frame)? {
                break;
            }
            self.rejected.pop_front();
        }
        let mut index = 0;
        while index < self.entries.len() {
            let reader = &mut self.entries[index];
            reader.receive()?;
            if let Some(frame) = reader.pending.as_ref() {
                if transport::send(channel, frame)? {
                    reader.pending = None;
                    reader.operation = Operation::Idle;
                }
            }
            if matches!(reader.operation, Operation::Closing) {
                if let Some(kind) = reader.failed {
                    if transport::send(
                        channel,
                        &Frame::ReaderFailed {
                            id: reader.id,
                            sequence: reader.sequence,
                            kind,
                        },
                    )? {
                        reader.failed = None;
                    }
                }
                if reader.reaped
                    && reader.drained
                    && reader.failed.is_none()
                    && transport::send(
                        channel,
                        &Frame::ReaderClosed {
                            id: reader.id,
                            sequence: reader.sequence,
                            cancelled: reader.cancelled,
                        },
                    )?
                {
                    let closed = self.entries.swap_remove(index);
                    self.terminal.push(Terminal {
                        id: closed.id,
                        sequence: closed.sequence,
                        reusable: closed.close_requested,
                    });
                    continue;
                }
            }
            index += 1;
        }
        Ok(())
    }
}

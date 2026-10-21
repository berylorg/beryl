use super::decode::header;
use super::*;
use std::io::{self, Read};
pub struct FrameReader {
    nonce: Nonce,
    pending: Vec<u8>,
    expected: Option<usize>,
    failed: bool,
}
impl FrameReader {
    pub fn new(nonce: Nonce) -> Self {
        Self {
            nonce,
            pending: Vec::new(),
            expected: None,
            failed: false,
        }
    }
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Frame>> {
        if self.failed {
            return invalid("reader already failed");
        }
        let result = self.feed_inner(bytes);
        if result.is_err() {
            self.failed = true;
            self.pending = Vec::new();
        }
        result
    }
    fn feed_inner(&mut self, mut bytes: &[u8]) -> Result<Vec<Frame>> {
        let mut frames = Vec::new();
        while !bytes.is_empty() {
            let target = self.expected.unwrap_or(HEADER_LEN);
            let count = (target - self.pending.len()).min(bytes.len());
            self.pending.extend_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            if self.pending.len() == HEADER_LEN && self.expected.is_none() {
                let (_, length, _) = header(&self.pending, Some(&self.nonce))?;
                self.pending.reserve_exact(length);
                self.expected = Some(HEADER_LEN + length);
            }
            if self.expected == Some(self.pending.len()) {
                if frames.len() == MAX_PENDING_FRAMES {
                    return invalid("too many queued frames");
                }
                frames.push(super::decode::decode_owned(
                    &self.nonce,
                    std::mem::take(&mut self.pending),
                )?);
                self.expected = None;
            }
        }
        Ok(frames)
    }
    pub fn finish(&self) -> Result<()> {
        if self.failed || !self.pending.is_empty() {
            return invalid("truncated or failed stream");
        }
        Ok(())
    }
    pub fn read_frame(&mut self, mut reader: impl Read) -> Result<Option<Frame>> {
        if self.failed {
            return invalid("reader already failed");
        }
        let result = self.read_frame_inner(&mut reader);
        if result.is_err() {
            self.failed = true;
            self.pending = Vec::new();
        }
        result
    }
    fn read_frame_inner(&mut self, reader: &mut impl Read) -> Result<Option<Frame>> {
        loop {
            let start = self.pending.len();
            let target = self.expected.unwrap_or(HEADER_LEN);
            let wanted = (target - start).min(MAX_READER_PAGE);
            self.pending.resize(start + wanted, 0);
            let result = reader.read(&mut self.pending[start..]);
            match result {
                Ok(0) => {
                    self.pending.truncate(start);
                    self.finish()?;
                    return Ok(None);
                }
                Ok(count) => {
                    self.pending.truncate(start + count);
                    if self.pending.len() == HEADER_LEN && self.expected.is_none() {
                        let (_, length, _) = header(&self.pending, Some(&self.nonce))?;
                        self.pending.reserve_exact(length);
                        self.expected = Some(HEADER_LEN + length);
                    }
                    if self.expected == Some(self.pending.len()) {
                        self.expected = None;
                        return super::decode::decode_owned(
                            &self.nonce,
                            std::mem::take(&mut self.pending),
                        )
                        .map(Some);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    self.pending.truncate(start);
                    continue;
                }
                Err(error) => {
                    self.pending.truncate(start);
                    return Err(error.into());
                }
            }
        }
    }
}
pub fn read_initial_frame(mut reader: impl Read) -> Result<(Nonce, Frame)> {
    let mut bytes = [0u8; HEADER_LEN];
    reader.read_exact(&mut bytes)?;
    let (nonce, length, _) = header(&bytes, None)?;
    let mut frame = Vec::with_capacity(HEADER_LEN + length);
    frame.extend_from_slice(&bytes);
    frame.resize(HEADER_LEN + length, 0);
    reader.read_exact(&mut frame[HEADER_LEN..])?;
    Ok((nonce, super::decode::decode_owned(&nonce, frame)?))
}
pub struct ControlState {
    role: Role,
    ready: bool,
    started: bool,
    namespace_closed: bool,
    closed: bool,
}
impl ControlState {
    pub fn new(role: Role) -> Self {
        Self {
            role,
            ready: false,
            started: false,
            namespace_closed: false,
            closed: false,
        }
    }
    pub fn accept(&mut self, frame: &Frame) -> Result<()> {
        if self.closed {
            return invalid("frame after terminal closure");
        }
        match frame {
            Frame::Ready { role } if *role == self.role && !self.ready => self.ready = true,
            Frame::WorkloadStarted
                if self.role == Role::Supervisor
                    && self.ready
                    && !self.started
                    && !self.namespace_closed =>
            {
                self.started = true
            }
            Frame::OwnedNamespaceClosed { .. }
                if self.role == Role::Supervisor && self.ready && !self.namespace_closed =>
            {
                self.namespace_closed = true
            }
            Frame::LinuxCompanionsClosed
                if self.role == Role::ContextBroker || self.namespace_closed =>
            {
                self.closed = true
            }
            Frame::Failure { .. } | Frame::ShutdownPending | Frame::Stop | Frame::BrokerClose => {}
            Frame::ReaderOpened { .. }
            | Frame::ReaderPage { .. }
            | Frame::ReaderFailed { .. }
            | Frame::ReaderClosed { .. }
                if self.role == Role::Supervisor && self.started && !self.namespace_closed => {}
            _ => return invalid("illegal control stage"),
        }
        Ok(())
    }
}

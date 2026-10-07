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
            self.pending.clear();
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
                self.expected = Some(HEADER_LEN + length);
            }
            if self.expected == Some(self.pending.len()) {
                if frames.len() == MAX_PENDING_FRAMES {
                    return invalid("too many queued frames");
                }
                frames.push(decode_frame(&self.nonce, &self.pending)?);
                self.pending.clear();
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
        loop {
            let mut scratch = [0u8; 8192];
            let wanted =
                (self.expected.unwrap_or(HEADER_LEN) - self.pending.len()).min(scratch.len());
            match reader.read(&mut scratch[..wanted]) {
                Ok(0) => {
                    self.finish()?;
                    return Ok(None);
                }
                Ok(count) => {
                    let mut frames = self.feed(&scratch[..count])?;
                    if !frames.is_empty() {
                        return Ok(Some(frames.remove(0)));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
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
    let mut remaining = length;
    while remaining != 0 {
        let mut scratch = [0u8; 8192];
        let count = remaining.min(scratch.len());
        reader.read_exact(&mut scratch[..count])?;
        frame.extend_from_slice(&scratch[..count]);
        remaining -= count;
    }
    Ok((nonce, decode_frame(&nonce, &frame)?))
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
            _ => return invalid("illegal control stage"),
        }
        Ok(())
    }
}

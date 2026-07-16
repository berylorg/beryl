use std::{
    cell::RefCell,
    io::{self, Read},
    rc::Rc,
    time::{Duration, Instant},
};

use super::{MessagePayload, PayloadRead, WebSocketClientTransport};
use crate::session::{ManagedBackendError, ManagedWebSocketError};

#[derive(Clone, Default)]
pub(super) struct PayloadReaderState {
    shared: Rc<RefCell<PayloadReaderShared>>,
}

#[derive(Default)]
struct PayloadReaderShared {
    started: bool,
    complete: bool,
    bytes_read: usize,
    maximum_chunk_bytes: usize,
    first_frame_after: Option<Duration>,
    first_payload_after: Option<Duration>,
    failure: Option<ManagedBackendError>,
}

impl PayloadReaderState {
    pub(super) fn started(&self) -> bool {
        self.shared.borrow().started
    }

    pub(super) fn complete(&self) -> bool {
        self.shared.borrow().complete
    }

    pub(super) fn bytes_read(&self) -> usize {
        self.shared.borrow().bytes_read
    }

    pub(super) fn maximum_chunk_bytes(&self) -> usize {
        self.shared.borrow().maximum_chunk_bytes
    }

    pub(super) fn first_frame_after(&self) -> Option<Duration> {
        self.shared.borrow().first_frame_after
    }

    pub(super) fn first_payload_after(&self) -> Option<Duration> {
        self.shared.borrow().first_payload_after
    }

    pub(super) fn take_failure(&self) -> Option<ManagedBackendError> {
        self.shared.borrow_mut().failure.take()
    }

    fn note_read(&self, was_started: bool, started: bool, count: usize, elapsed: Duration) {
        let mut shared = self.shared.borrow_mut();
        shared.started |= started;
        if !was_started && started && shared.first_frame_after.is_none() {
            shared.first_frame_after = Some(elapsed);
        }
        if count > 0 && shared.first_payload_after.is_none() {
            shared.first_payload_after = Some(elapsed);
        }
        shared.bytes_read = shared.bytes_read.saturating_add(count);
        shared.maximum_chunk_bytes = shared.maximum_chunk_bytes.max(count);
    }

    fn note_complete(&self, started: bool, elapsed: Duration) {
        let mut shared = self.shared.borrow_mut();
        shared.started |= started;
        if started && shared.first_frame_after.is_none() {
            shared.first_frame_after = Some(elapsed);
        }
        shared.complete = true;
    }

    fn fail(&self, error: ManagedBackendError) -> io::Error {
        self.shared.borrow_mut().failure = Some(error);
        io::Error::new(io::ErrorKind::Other, "backend WebSocket ingress failed")
    }
}

pub(super) struct WebSocketPayloadReader<'transport, 'method> {
    transport: &'transport mut WebSocketClientTransport,
    method: &'method str,
    payload: MessagePayload<'method>,
    state: PayloadReaderState,
    started_at: Instant,
}

impl<'transport, 'method> WebSocketPayloadReader<'transport, 'method> {
    pub(super) fn new(
        transport: &'transport mut WebSocketClientTransport,
        method: &'method str,
        message_budget: usize,
        state: PayloadReaderState,
    ) -> Self {
        Self {
            transport,
            method,
            payload: MessagePayload::new(method, message_budget),
            state,
            started_at: Instant::now(),
        }
    }

    fn transport_failure(&self, source: ManagedWebSocketError) -> io::Error {
        self.state
            .fail(self.transport.transport_error(self.method, source))
    }
}

impl Read for WebSocketPayloadReader<'_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            let was_started = self.payload.started;
            match self
                .transport
                .read_message_payload_chunk(self.method, &mut self.payload, output)
            {
                Ok(PayloadRead::Idle) if !self.payload.started => return Ok(0),
                Ok(PayloadRead::Idle) => {
                    return Err(self.transport_failure(ManagedWebSocketError::protocol(
                        "timed out while reading WebSocket message",
                    )));
                }
                Ok(PayloadRead::Bytes(count)) => {
                    self.state.note_read(
                        was_started,
                        self.payload.started,
                        count,
                        self.started_at.elapsed(),
                    );
                    return Ok(count);
                }
                Ok(PayloadRead::Complete) => {
                    self.state
                        .note_complete(self.payload.started, self.started_at.elapsed());
                    return Ok(0);
                }
                Ok(PayloadRead::Pong) => {}
                Ok(PayloadRead::Close) => {
                    return Err(self.state.fail(ManagedBackendError::TransportClosed {
                        method: self.method.to_string(),
                    }));
                }
                Ok(PayloadRead::Binary) => {
                    return Err(self.state.fail(ManagedBackendError::UnexpectedMessageShape));
                }
                Err(error) => return Err(self.transport_failure(error)),
            }
        }
    }
}

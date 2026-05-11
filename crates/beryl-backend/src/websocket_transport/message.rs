use std::{
    io::{self, ErrorKind, Read},
    time::{Duration, Instant},
};

use soketto::base::OpCode;

use super::{
    HeaderRead, READ_CHUNK_BYTES, WEBSOCKET_TEXT_MESSAGE_BUDGET, WebSocketClientTransport,
};
use crate::session::ManagedWebSocketError;

pub(super) struct MessagePayload<'a> {
    method: &'a str,
    message_budget: usize,
    bytes_read: usize,
    text_frame_count: usize,
    continuation_frame_count: usize,
    control_frame_count: usize,
    state: MessageReadState,
    complete_pending: bool,
    pub(super) started: bool,
}

impl<'a> MessagePayload<'a> {
    pub(super) fn new(method: &'a str, message_budget: usize) -> Self {
        Self {
            method,
            message_budget,
            bytes_read: 0,
            text_frame_count: 0,
            continuation_frame_count: 0,
            control_frame_count: 0,
            state: MessageReadState::WaitingForFirstFrame,
            complete_pending: false,
            started: false,
        }
    }

    fn note_bytes(&mut self, count: usize) -> Result<(), ManagedWebSocketError> {
        let next_len = self.bytes_read.saturating_add(count);
        if next_len > self.message_budget {
            return Err(ManagedWebSocketError::protocol(format!(
                "WebSocket text message for {} exceeded {} byte budget",
                self.method, self.message_budget
            )));
        }
        self.bytes_read = next_len;
        Ok(())
    }

    pub(super) fn bytes_read(&self) -> usize {
        self.bytes_read
    }

    fn note_text_frame(&mut self) {
        self.text_frame_count += 1;
    }

    fn note_continuation_frame(&mut self) {
        self.continuation_frame_count += 1;
    }

    fn note_control_frame(&mut self) {
        self.control_frame_count += 1;
    }
}

pub(super) struct WebSocketTextMessageReader<'a> {
    pub(super) transport: &'a mut WebSocketClientTransport,
    method: &'a str,
    payload: MessagePayload<'a>,
    buffer: [u8; READ_CHUNK_BYTES],
    offset: usize,
    len: usize,
    complete: bool,
    started_at: Instant,
    read_wait: Duration,
    fill_buffer_calls: usize,
    payload_chunk_count: usize,
    max_payload_chunk_bytes: usize,
    first_frame_after: Option<Duration>,
    first_payload_after: Option<Duration>,
}

impl<'a> WebSocketTextMessageReader<'a> {
    pub(super) fn new(transport: &'a mut WebSocketClientTransport, method: &'a str) -> Self {
        Self {
            transport,
            method,
            payload: MessagePayload::new(method, WEBSOCKET_TEXT_MESSAGE_BUDGET),
            buffer: [0_u8; READ_CHUNK_BYTES],
            offset: 0,
            len: 0,
            complete: false,
            started_at: Instant::now(),
            read_wait: Duration::ZERO,
            fill_buffer_calls: 0,
            payload_chunk_count: 0,
            max_payload_chunk_bytes: 0,
            first_frame_after: None,
            first_payload_after: None,
        }
    }

    pub(super) fn bytes_read(&self) -> usize {
        self.payload.bytes_read()
    }

    pub(super) fn read_wait(&self) -> Duration {
        self.read_wait
    }

    pub(super) fn first_frame_after(&self) -> Option<Duration> {
        self.first_frame_after
    }

    pub(super) fn first_payload_after(&self) -> Option<Duration> {
        self.first_payload_after
    }

    pub(super) fn fill_buffer_calls(&self) -> usize {
        self.fill_buffer_calls
    }

    pub(super) fn payload_chunk_count(&self) -> usize {
        self.payload_chunk_count
    }

    pub(super) fn max_payload_chunk_bytes(&self) -> usize {
        self.max_payload_chunk_bytes
    }

    pub(super) fn text_frame_count(&self) -> usize {
        self.payload.text_frame_count
    }

    pub(super) fn continuation_frame_count(&self) -> usize {
        self.payload.continuation_frame_count
    }

    pub(super) fn control_frame_count(&self) -> usize {
        self.payload.control_frame_count
    }

    pub(super) fn prime(&mut self) -> Result<MessagePrime, ManagedWebSocketError> {
        match self.fill_buffer()? {
            ReaderFill::Bytes => Ok(MessagePrime::Ready),
            ReaderFill::Complete => Ok(MessagePrime::Ready),
            ReaderFill::Idle => Ok(MessagePrime::Idle),
            ReaderFill::Close => Ok(MessagePrime::Close),
            ReaderFill::Binary => Ok(MessagePrime::Binary),
        }
    }

    pub(super) fn discard_to_end(&mut self) -> Result<(), ManagedWebSocketError> {
        self.offset = self.len;
        while !self.complete {
            match self.fill_buffer()? {
                ReaderFill::Bytes => {
                    self.offset = self.len;
                }
                ReaderFill::Complete => return Ok(()),
                ReaderFill::Idle => {
                    return Err(ManagedWebSocketError::protocol(
                        "timed out while draining WebSocket text message",
                    ));
                }
                ReaderFill::Close => {
                    return Err(ManagedWebSocketError::protocol(
                        "WebSocket closed while draining text message",
                    ));
                }
                ReaderFill::Binary => {
                    return Err(ManagedWebSocketError::protocol(
                        "received binary WebSocket message while draining text message",
                    ));
                }
            }
        }
        Ok(())
    }

    fn fill_buffer(&mut self) -> Result<ReaderFill, ManagedWebSocketError> {
        if self.complete {
            return Ok(ReaderFill::Complete);
        }

        loop {
            self.fill_buffer_calls += 1;
            let was_started = self.payload.started;
            let read_started = Instant::now();
            let read = self.transport.read_message_payload_chunk(
                self.method,
                &mut self.payload,
                &mut self.buffer,
            );
            self.read_wait += read_started.elapsed();

            if !was_started && self.payload.started && self.first_frame_after.is_none() {
                self.first_frame_after = Some(self.started_at.elapsed());
            }

            match read? {
                PayloadRead::Idle if !self.payload.started => return Ok(ReaderFill::Idle),
                PayloadRead::Idle => {
                    return Err(ManagedWebSocketError::protocol(
                        "timed out while reading WebSocket text message",
                    ));
                }
                PayloadRead::Bytes(count) => {
                    if self.first_payload_after.is_none() {
                        self.first_payload_after = Some(self.started_at.elapsed());
                    }
                    self.payload_chunk_count += 1;
                    self.max_payload_chunk_bytes = self.max_payload_chunk_bytes.max(count);
                    self.offset = 0;
                    self.len = count;
                    return Ok(ReaderFill::Bytes);
                }
                PayloadRead::Complete => {
                    self.complete = true;
                    return Ok(ReaderFill::Complete);
                }
                PayloadRead::Pong => {}
                PayloadRead::Close => return Ok(ReaderFill::Close),
                PayloadRead::Binary => return Ok(ReaderFill::Binary),
            }
        }
    }
}

impl Read for WebSocketTextMessageReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }

        if self.offset == self.len {
            match self.fill_buffer() {
                Ok(ReaderFill::Bytes) => {}
                Ok(ReaderFill::Complete) => return Ok(0),
                Ok(ReaderFill::Idle) => {
                    return Err(io::Error::new(
                        ErrorKind::TimedOut,
                        "timed out while reading WebSocket text message",
                    ));
                }
                Ok(ReaderFill::Close) => {
                    return Err(io::Error::new(
                        ErrorKind::ConnectionAborted,
                        "backend WebSocket transport closed",
                    ));
                }
                Ok(ReaderFill::Binary) => {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        "received binary WebSocket message",
                    ));
                }
                Err(error) => return Err(io::Error::other(error)),
            }
        }

        let count = output.len().min(self.len - self.offset);
        output[..count].copy_from_slice(&self.buffer[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

pub(super) enum MessagePrime {
    Idle,
    Ready,
    Close,
    Binary,
}

enum ReaderFill {
    Idle,
    Bytes,
    Complete,
    Close,
    Binary,
}

enum MessageReadState {
    WaitingForFirstFrame,
    WaitingForContinuation,
    ReadingTextFrame { remaining: usize, final_frame: bool },
}

pub(super) enum PayloadRead {
    Idle,
    Bytes(usize),
    Complete,
    Pong,
    Close,
    Binary,
}

impl WebSocketClientTransport {
    pub(super) fn read_message_payload_chunk(
        &mut self,
        method: &str,
        payload: &mut MessagePayload<'_>,
        output: &mut [u8],
    ) -> Result<PayloadRead, ManagedWebSocketError> {
        loop {
            if payload.complete_pending {
                payload.complete_pending = false;
                return Ok(PayloadRead::Complete);
            }

            if let MessageReadState::ReadingTextFrame {
                remaining,
                final_frame,
            } = &mut payload.state
            {
                let count = self.read_payload_chunk(*remaining, output)?;
                *remaining -= count;
                if *remaining == 0 {
                    if *final_frame {
                        payload.complete_pending = true;
                        payload.state = MessageReadState::WaitingForFirstFrame;
                    } else {
                        payload.state = MessageReadState::WaitingForContinuation;
                    }
                }
                if count > 0 {
                    payload.note_bytes(count)?;
                    return Ok(PayloadRead::Bytes(count));
                }
                if payload.complete_pending {
                    continue;
                }
            }

            let header = match self.read_header()? {
                HeaderRead::Idle => return Ok(PayloadRead::Idle),
                HeaderRead::Header(header) => header,
            };

            if header.is_masked() {
                return Err(ManagedWebSocketError::protocol(
                    "server-to-client WebSocket frame was masked",
                ));
            }

            if header.opcode().is_control() {
                payload.note_control_frame();
                let control = self.read_control_payload(&header)?;
                match header.opcode() {
                    OpCode::Ping => {
                        self.write_frame_payload(OpCode::Pong, &control)?;
                        return Ok(PayloadRead::Pong);
                    }
                    OpCode::Pong => return Ok(PayloadRead::Pong),
                    OpCode::Close => {
                        let _ = self.write_close_frame(method);
                        return Ok(PayloadRead::Close);
                    }
                    _ => {
                        return Err(ManagedWebSocketError::protocol(format!(
                            "unexpected control opcode {}",
                            header.opcode()
                        )));
                    }
                }
            }

            match (&payload.state, header.opcode()) {
                (MessageReadState::WaitingForFirstFrame, OpCode::Text) => {
                    payload.note_text_frame();
                    payload.started = true;
                    payload.state = MessageReadState::ReadingTextFrame {
                        remaining: header.payload_len(),
                        final_frame: header.is_fin(),
                    };
                }
                (MessageReadState::WaitingForContinuation, OpCode::Continue) => {
                    payload.note_continuation_frame();
                    payload.state = MessageReadState::ReadingTextFrame {
                        remaining: header.payload_len(),
                        final_frame: header.is_fin(),
                    };
                }
                (MessageReadState::WaitingForFirstFrame, OpCode::Binary) => {
                    return Ok(PayloadRead::Binary);
                }
                (MessageReadState::WaitingForFirstFrame, OpCode::Continue) => {
                    return Err(ManagedWebSocketError::protocol(
                        "received continuation frame before a data frame",
                    ));
                }
                (MessageReadState::WaitingForContinuation, OpCode::Text | OpCode::Binary) => {
                    return Err(ManagedWebSocketError::protocol(
                        "received new data frame before fragmented message completed",
                    ));
                }
                (_, opcode) => {
                    return Err(ManagedWebSocketError::protocol(format!(
                        "unexpected WebSocket opcode {opcode}"
                    )));
                }
            }
        }
    }
}

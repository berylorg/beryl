use soketto::base::OpCode;

use super::{HeaderRead, WebSocketClientTransport};
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
            #[cfg(feature = "lifecycle-test-support")]
            self.diagnostics.record_inbound_frame(header.payload_len());

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

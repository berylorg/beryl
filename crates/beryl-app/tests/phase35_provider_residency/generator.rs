use std::{
    io::{self, Write},
    net::TcpStream,
    time::Duration,
};

use tungstenite::{Message, WebSocket};

use super::server::{CAS_THREAD_ID, CAS_TURN_ID, ObservationReport, ObservationSpec, TIMEOUT};

const FRAME_PAYLOAD_BYTES: usize = 4_093;
const BACKPRESSURE_PREFIX_WIRE_BYTES: u64 = 116 * 1_024;
const WIRE_PATTERN: &[u8] = b"a\xC3\xA9\\n\\u20AC\\\"\\\\";
pub const SEMANTIC_PATTERN: &[u8] = "aé\n€\"\\".as_bytes();

pub(crate) fn write_observation(
    socket: &mut WebSocket<TcpStream>,
    spec: ObservationSpec,
) -> ObservationReport {
    finish_observation(socket, WireMessage::new(spec))
}

pub(crate) fn finish_observation(
    socket: &mut WebSocket<TcpStream>,
    mut message: WireMessage,
) -> ObservationReport {
    let mut scratch = [0_u8; FRAME_PAYLOAD_BYTES];
    while !message.complete() {
        let count = message.fill(&mut scratch);
        let final_frame = message.complete();
        write_frame(
            socket.get_mut(),
            if message.frame_count == 0 { 0x1 } else { 0x0 },
            final_frame,
            &scratch[..count],
        )
        .unwrap();
        message.note_frame(count);
    }
    socket.get_mut().flush().unwrap();
    message.report()
}

pub(crate) fn write_prefix(socket: &mut WebSocket<TcpStream>, message: &mut WireMessage) {
    let mut scratch = [0_u8; FRAME_PAYLOAD_BYTES];
    while message.wire_bytes < BACKPRESSURE_PREFIX_WIRE_BYTES {
        assert!(!message.complete());
        let count = message.fill(&mut scratch);
        assert!(
            !message.complete(),
            "backpressure fixture requires a non-final prefix"
        );
        write_frame(
            socket.get_mut(),
            if message.frame_count == 0 { 0x1 } else { 0x0 },
            false,
            &scratch[..count],
        )
        .unwrap();
        message.note_frame(count);
    }
    socket.get_mut().flush().unwrap();
}

pub(crate) fn probe_while_blocked(socket: &mut WebSocket<TcpStream>, message: &mut WireMessage) {
    write_frame(socket.get_mut(), 0x9, true, b"phase35").unwrap();
    let mut scratch = [0_u8; FRAME_PAYLOAD_BYTES];
    let count = message.fill(&mut scratch);
    assert!(!message.complete());
    write_frame(socket.get_mut(), 0x0, false, &scratch[..count]).unwrap();
    message.note_frame(count);
    socket.get_mut().flush().unwrap();

    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    assert!(matches!(
        socket.read(),
        Err(tungstenite::Error::Io(error))
            if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
    ));
}

pub(crate) fn wait_for_pong(socket: &mut WebSocket<TcpStream>) {
    socket.get_mut().set_read_timeout(Some(TIMEOUT)).unwrap();
    loop {
        match socket.read() {
            Ok(Message::Pong(payload)) => {
                assert_eq!(payload.as_ref(), b"phase35");
                return;
            }
            Ok(Message::Ping(payload)) => socket.send(Message::Pong(payload)).unwrap(),
            Ok(Message::Close(_)) => panic!("provider client closed before releasing backpressure"),
            Ok(Message::Text(text)) => panic!("unexpected client text while probing: {text}"),
            Ok(Message::Binary(bytes)) => panic!("unexpected client binary: {bytes:?}"),
            Ok(Message::Frame(_)) => {}
            Err(error) => panic!("provider server did not receive resumed Pong: {error}"),
        }
    }
}

pub(crate) fn write_missing_text(socket: &mut WebSocket<TcpStream>, sequence: u64) {
    let value = format!(
        r#"{{"method":"item/started","params":{{"item":{{"type":"agentMessage","id":"phase35-item-{sequence}"}},"threadId":"{CAS_THREAD_ID}","turnId":"{CAS_TURN_ID}","startedAtMs":123}}}}"#,
    );
    write_frame(socket.get_mut(), 0x1, true, value.as_bytes()).unwrap();
    socket.get_mut().flush().unwrap();
}

fn write_frame(
    stream: &mut TcpStream,
    opcode: u8,
    final_frame: bool,
    payload: &[u8],
) -> io::Result<()> {
    let mut header = [0_u8; 10];
    header[0] = (if final_frame { 0x80 } else { 0 }) | opcode;
    let header_len = match payload.len() {
        length @ 0..=125 => {
            header[1] = u8::try_from(length).unwrap();
            2
        }
        length @ 126..=65_535 => {
            header[1] = 126;
            header[2..4].copy_from_slice(&u16::try_from(length).unwrap().to_be_bytes());
            4
        }
        length => {
            header[1] = 127;
            header[2..10].copy_from_slice(&u64::try_from(length).unwrap().to_be_bytes());
            10
        }
    };
    stream.write_all(&header[..header_len])?;
    stream.write_all(payload)
}

enum WireStage {
    Prefix,
    Pattern,
    Suffix,
    Complete,
}

pub(crate) struct WireMessage {
    spec: ObservationSpec,
    prefix: Box<[u8]>,
    suffix: Box<[u8]>,
    stage: WireStage,
    offset: usize,
    patterns_remaining: u64,
    wire_bytes: u64,
    frame_count: u64,
}

impl WireMessage {
    pub(crate) fn new(spec: ObservationSpec) -> Self {
        assert!(spec.pattern_repetitions > 0);
        let prefix = format!(
            r#"{{"method":"item/started","params":{{"item":{{"type":"agentMessage","id":"{}","text":""#,
            spec.item_id(),
        )
        .into_bytes()
        .into_boxed_slice();
        let suffix = format!(
            r#""}},"threadId":"{CAS_THREAD_ID}","turnId":"{CAS_TURN_ID}","startedAtMs":123}}}}"#,
        )
        .into_bytes()
        .into_boxed_slice();
        Self {
            spec,
            prefix,
            suffix,
            stage: WireStage::Prefix,
            offset: 0,
            patterns_remaining: spec.pattern_repetitions,
            wire_bytes: 0,
            frame_count: 0,
        }
    }

    fn complete(&self) -> bool {
        matches!(self.stage, WireStage::Complete)
    }

    fn fill(&mut self, output: &mut [u8]) -> usize {
        let mut written = 0;
        while written < output.len() && !self.complete() {
            match self.stage {
                WireStage::Prefix => {
                    written += copy_from(&self.prefix, &mut self.offset, &mut output[written..]);
                    if self.offset == self.prefix.len() {
                        self.stage = WireStage::Pattern;
                        self.offset = 0;
                    }
                }
                WireStage::Pattern => {
                    if self.patterns_remaining == 0 {
                        self.stage = WireStage::Suffix;
                        self.offset = 0;
                        continue;
                    }
                    written += copy_from(WIRE_PATTERN, &mut self.offset, &mut output[written..]);
                    if self.offset == WIRE_PATTERN.len() {
                        self.patterns_remaining -= 1;
                        self.offset = 0;
                    }
                }
                WireStage::Suffix => {
                    written += copy_from(&self.suffix, &mut self.offset, &mut output[written..]);
                    if self.offset == self.suffix.len() {
                        self.stage = WireStage::Complete;
                    }
                }
                WireStage::Complete => {}
            }
        }
        assert!(written > 0 || self.complete());
        written
    }

    fn note_frame(&mut self, bytes: usize) {
        self.wire_bytes = self
            .wire_bytes
            .checked_add(u64::try_from(bytes).unwrap())
            .unwrap();
        self.frame_count = self.frame_count.checked_add(1).unwrap();
    }

    fn report(&self) -> ObservationReport {
        assert!(self.complete());
        ObservationReport {
            sequence: self.spec.sequence,
            wire_bytes: self.wire_bytes,
            semantic_bytes: self.spec.semantic_bytes(),
            frame_count: self.frame_count,
        }
    }
}

fn copy_from(source: &[u8], offset: &mut usize, output: &mut [u8]) -> usize {
    let count = output.len().min(source.len() - *offset);
    output[..count].copy_from_slice(&source[*offset..*offset + count]);
    *offset += count;
    count
}

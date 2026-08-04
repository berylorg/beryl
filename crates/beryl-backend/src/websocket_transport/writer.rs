use std::{
    io::{self, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

use soketto::base::{Codec, Header, OpCode};

#[cfg(feature = "lifecycle-test-support")]
use super::WebSocketDiagnostics;
use crate::session::{
    ManagedWebSocketError,
    outbound::{DispatchProgress, JsonMessageWriter, write_all_tracked},
};

pub(super) const OUTBOUND_FRAME_PAYLOAD_BYTES: usize = 64 * 1024;

const CONTROL_FRAME_PAYLOAD_LIMIT: usize = 125;
const TRANSPORT_WRITE_SENTINEL: &str = "bounded WebSocket transport write failed";
const ENCODED_LENGTH_OVERFLOW: &str = "outbound JSON byte length exceeded platform representation";

pub(super) struct WebSocketMessageWriter<'a> {
    stream: &'a mut TcpStream,
    codec: &'a mut Codec,
    payload: &'a mut [u8],
    buffered: usize,
    encoded_bytes: usize,
    emitted_frames: usize,
    progress: DispatchProgress,
    transport_elapsed: Duration,
    failure: Option<ManagedWebSocketError>,
    encoding_failed: bool,
    #[cfg(feature = "lifecycle-test-support")]
    diagnostics: WebSocketDiagnostics,
}

impl<'a> WebSocketMessageWriter<'a> {
    pub(super) fn new(
        stream: &'a mut TcpStream,
        codec: &'a mut Codec,
        payload: &'a mut [u8],
        #[cfg(feature = "lifecycle-test-support")] diagnostics: WebSocketDiagnostics,
    ) -> Self {
        debug_assert!(!payload.is_empty());
        Self {
            stream,
            codec,
            payload,
            buffered: 0,
            encoded_bytes: 0,
            emitted_frames: 0,
            progress: DispatchProgress::NeverWritten,
            transport_elapsed: Duration::ZERO,
            failure: None,
            encoding_failed: false,
            #[cfg(feature = "lifecycle-test-support")]
            diagnostics,
        }
    }

    fn append(&mut self, mut bytes: &[u8]) -> Result<(), ManagedWebSocketError> {
        while !bytes.is_empty() {
            if self.buffered == self.payload.len() {
                self.emit_frame(false)?;
            }
            let count = bytes.len().min(self.payload.len() - self.buffered);
            self.payload[self.buffered..self.buffered + count].copy_from_slice(&bytes[..count]);
            self.buffered += count;
            #[cfg(feature = "lifecycle-test-support")]
            self.diagnostics.record_outbound_buffered(self.buffered);
            bytes = &bytes[count..];
        }
        Ok(())
    }

    fn emit_frame(&mut self, is_final: bool) -> Result<(), ManagedWebSocketError> {
        let started = Instant::now();
        let result = (|| {
            let opcode = if self.emitted_frames == 0 {
                OpCode::Text
            } else {
                OpCode::Continue
            };
            let mut header = Header::new(opcode);
            let mut mask = [0_u8; 4];
            getrandom::fill(&mut mask).map_err(ManagedWebSocketError::from_mask_generation)?;
            header
                .set_fin(is_final)
                .set_masked(true)
                .set_mask(u32::from_be_bytes(mask))
                .set_payload_len(self.buffered);

            Codec::apply_mask(&header, &mut self.payload[..self.buffered]);
            let header_bytes = self.codec.encode_header(&header);
            write_all_tracked(self.stream, header_bytes, &mut self.progress)
                .map_err(ManagedWebSocketError::from_io)?;
            write_all_tracked(
                self.stream,
                &self.payload[..self.buffered],
                &mut self.progress,
            )
            .map_err(ManagedWebSocketError::from_io)?;
            Ok(())
        })();
        self.transport_elapsed += started.elapsed();
        result?;
        #[cfg(feature = "lifecycle-test-support")]
        self.diagnostics.record_outbound_frame(self.buffered);
        self.buffered = 0;
        self.emitted_frames += 1;
        Ok(())
    }

    fn flush_stream(&mut self) -> Result<(), ManagedWebSocketError> {
        let started = Instant::now();
        let result = self.stream.flush().map_err(ManagedWebSocketError::from_io);
        self.transport_elapsed += started.elapsed();
        result
    }

    fn record_failure(&mut self, source: ManagedWebSocketError) -> io::Error {
        self.failure = Some(source);
        io::Error::other(TRANSPORT_WRITE_SENTINEL)
    }

    fn failed_write(&self) -> io::Error {
        io::Error::other(TRANSPORT_WRITE_SENTINEL)
    }

    fn encoded_length_overflow(&mut self) -> io::Error {
        self.encoding_failed = true;
        io::Error::new(io::ErrorKind::InvalidData, ENCODED_LENGTH_OVERFLOW)
    }
}

impl Write for WebSocketMessageWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failure.is_some() {
            return Err(self.failed_write());
        }
        if self.encoding_failed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                ENCODED_LENGTH_OVERFLOW,
            ));
        }
        let Some(encoded_bytes) = self.encoded_bytes.checked_add(bytes.len()) else {
            return Err(self.encoded_length_overflow());
        };
        if let Err(source) = self.append(bytes) {
            return Err(self.record_failure(source));
        }
        self.encoded_bytes = encoded_bytes;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.failure.is_some() {
            return Err(self.failed_write());
        }
        if let Err(source) = self.flush_stream() {
            return Err(self.record_failure(source));
        }
        Ok(())
    }
}

impl JsonMessageWriter for WebSocketMessageWriter<'_> {
    type TransportError = ManagedWebSocketError;

    fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    fn dispatch_progress(&self) -> DispatchProgress {
        self.progress
    }

    fn transport_elapsed(&self) -> Duration {
        self.transport_elapsed
    }

    fn take_transport_failure(&mut self) -> Option<Self::TransportError> {
        self.failure.take()
    }

    fn finish_message(&mut self) -> Result<(), Self::TransportError> {
        self.emit_frame(true)?;
        self.flush_stream()
    }
}

pub(super) fn write_control_frame(
    stream: &mut TcpStream,
    codec: &mut Codec,
    opcode: OpCode,
    payload: &[u8],
) -> Result<(), ManagedWebSocketError> {
    if payload.len() > CONTROL_FRAME_PAYLOAD_LIMIT {
        return Err(ManagedWebSocketError::protocol(format!(
            "outbound WebSocket control payload exceeded {CONTROL_FRAME_PAYLOAD_LIMIT} bytes"
        )));
    }

    let mut in_place_payload = [0_u8; CONTROL_FRAME_PAYLOAD_LIMIT];
    in_place_payload[..payload.len()].copy_from_slice(payload);
    let in_place_payload = &mut in_place_payload[..payload.len()];
    let mut header = Header::new(opcode);
    let mut mask = [0_u8; 4];
    getrandom::fill(&mut mask).map_err(ManagedWebSocketError::from_mask_generation)?;
    header
        .set_masked(true)
        .set_mask(u32::from_be_bytes(mask))
        .set_payload_len(in_place_payload.len());
    Codec::apply_mask(&header, in_place_payload);

    let mut progress = DispatchProgress::NeverWritten;
    write_all_tracked(stream, codec.encode_header(&header), &mut progress)
        .map_err(ManagedWebSocketError::from_io)?;
    write_all_tracked(stream, in_place_payload, &mut progress)
        .map_err(ManagedWebSocketError::from_io)?;
    stream.flush().map_err(ManagedWebSocketError::from_io)
}

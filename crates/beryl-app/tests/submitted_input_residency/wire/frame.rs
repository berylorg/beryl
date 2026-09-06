use std::{
    io::{self, Read, Write},
    net::TcpStream,
    num::NonZeroU64,
};

use super::{RequestAbortObservation, RequestAbortReason, RequestObservation, RequestOutcome};

const CLIENT_FRAME_PAYLOAD_BYTES: usize = 64 * 1024;
const SERVER_FRAME_PAYLOAD_BYTES: usize = 4 * 1024;
const CONTROL_PAYLOAD_BYTES: usize = 125;

const OPCODE_CONTINUATION: u8 = 0x0;
const OPCODE_TEXT: u8 = 0x1;
const OPCODE_CLOSE: u8 = 0x8;
const OPCODE_PING: u8 = 0x9;
const OPCODE_PONG: u8 = 0xa;

#[derive(Clone, Copy, Debug)]
pub(crate) enum RequestCutoff {
    None,
    Bytes(NonZeroU64),
    Frames(NonZeroU64),
}

pub(crate) fn verify_masked_text_message(
    stream: &mut TcpStream,
    request_id: u64,
    mut expected: impl Iterator<Item = u8>,
    cutoff: RequestCutoff,
) -> io::Result<RequestOutcome> {
    let mut payload = [0; CLIENT_FRAME_PAYLOAD_BYTES];
    let mut started = false;
    let mut logical_bytes = 0_u64;
    let mut frame_count = 0_u64;
    let mut maximum_frame_payload_bytes = 0;

    loop {
        let header = match read_client_header(stream) {
            Ok(header) => header,
            Err(error) if is_transport_eof(&error) => {
                return Ok(aborted(
                    request_id,
                    logical_bytes,
                    frame_count,
                    maximum_frame_payload_bytes,
                    RequestAbortReason::TransportEof,
                ));
            }
            Err(error) => return Err(error),
        };
        if header.is_control() {
            match consume_control(stream, header, &mut payload[..CONTROL_PAYLOAD_BYTES]) {
                Ok(ControlDisposition::Continue) => continue,
                Ok(ControlDisposition::PeerClose) => {
                    return Ok(aborted(
                        request_id,
                        logical_bytes,
                        frame_count,
                        maximum_frame_payload_bytes,
                        RequestAbortReason::PeerClose,
                    ));
                }
                Err(error) if is_transport_eof(&error) => {
                    return Ok(aborted(
                        request_id,
                        logical_bytes,
                        frame_count,
                        maximum_frame_payload_bytes,
                        RequestAbortReason::TransportEof,
                    ));
                }
                Err(error) => return Err(error),
            }
        }

        let expected_opcode = if started {
            OPCODE_CONTINUATION
        } else {
            OPCODE_TEXT
        };
        if header.opcode != expected_opcode {
            return Err(protocol_error("unexpected client data-frame opcode"));
        }
        started = true;
        frame_count = frame_count
            .checked_add(1)
            .ok_or_else(|| protocol_error("client frame count overflowed"))?;

        let frame_len = usize::try_from(header.payload_len)
            .map_err(|_| protocol_error("client frame payload did not fit usize"))?;
        if frame_len > payload.len() {
            return Err(protocol_error(
                "client frame exceeded the production 64 KiB payload boundary",
            ));
        }
        let accepted_len = accepted_frame_bytes(cutoff, logical_bytes, frame_len);
        let (read, eof) = read_with_eof(stream, &mut payload[..accepted_len])?;
        unmask(&mut payload[..read], header.mask, 0);
        compare_expected(&mut expected, &payload[..read], logical_bytes)?;
        logical_bytes = logical_bytes
            .checked_add(u64::try_from(read).unwrap())
            .ok_or_else(|| protocol_error("client logical byte count overflowed"))?;
        maximum_frame_payload_bytes = maximum_frame_payload_bytes.max(read);
        if eof {
            return Ok(aborted(
                request_id,
                logical_bytes,
                frame_count,
                maximum_frame_payload_bytes,
                RequestAbortReason::TransportEof,
            ));
        }
        if accepted_len < frame_len {
            return Ok(aborted(
                request_id,
                logical_bytes,
                frame_count,
                maximum_frame_payload_bytes,
                RequestAbortReason::ServerByteCutoff,
            ));
        }

        if header.fin {
            if expected.next().is_some() {
                return Err(protocol_error(
                    "client text message ended before expected JSON",
                ));
            }
            return Ok(RequestOutcome::Complete(RequestObservation::new(
                request_id,
                logical_bytes,
                frame_count,
                maximum_frame_payload_bytes,
            )));
        }
        if matches!(cutoff, RequestCutoff::Frames(limit) if frame_count >= limit.get()) {
            return Ok(aborted(
                request_id,
                logical_bytes,
                frame_count,
                maximum_frame_payload_bytes,
                RequestAbortReason::ServerFrameCutoff,
            ));
        }
        if matches!(cutoff, RequestCutoff::Bytes(limit) if logical_bytes >= limit.get()) {
            return Ok(aborted(
                request_id,
                logical_bytes,
                frame_count,
                maximum_frame_payload_bytes,
                RequestAbortReason::ServerByteCutoff,
            ));
        }
    }
}

fn accepted_frame_bytes(cutoff: RequestCutoff, seen: u64, frame_len: usize) -> usize {
    let RequestCutoff::Bytes(limit) = cutoff else {
        return frame_len;
    };
    let remaining = limit.get().saturating_sub(seen);
    frame_len.min(usize::try_from(remaining).unwrap_or(usize::MAX))
}

fn compare_expected(
    expected: &mut impl Iterator<Item = u8>,
    actual: &[u8],
    offset: u64,
) -> io::Result<()> {
    for (index, actual) in actual.iter().copied().enumerate() {
        let Some(expected) = expected.next() else {
            return Err(protocol_error(
                "client sent bytes after expected JSON ended",
            ));
        };
        if actual != expected {
            let position = offset + u64::try_from(index).unwrap();
            return Err(protocol_error(&format!(
                "client JSON mismatch at byte {position}: expected {expected:#04x}, got {actual:#04x}",
            )));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ClientFrameHeader {
    fin: bool,
    opcode: u8,
    payload_len: u64,
    mask: [u8; 4],
}

impl ClientFrameHeader {
    const fn is_control(self) -> bool {
        self.opcode & 0x08 != 0
    }
}

fn read_client_header(stream: &mut TcpStream) -> io::Result<ClientFrameHeader> {
    let mut base = [0; 2];
    stream.read_exact(&mut base)?;
    if base[0] & 0x70 != 0 {
        return Err(protocol_error("client frame used reserved RFC 6455 bits"));
    }
    if base[1] & 0x80 == 0 {
        return Err(protocol_error("client frame was not masked"));
    }
    let fin = base[0] & 0x80 != 0;
    let opcode = base[0] & 0x0f;
    let short_len = base[1] & 0x7f;
    let payload_len = match short_len {
        0..=125 => u64::from(short_len),
        126 => {
            let mut extended = [0; 2];
            stream.read_exact(&mut extended)?;
            u64::from(u16::from_be_bytes(extended))
        }
        127 => {
            let mut extended = [0; 8];
            stream.read_exact(&mut extended)?;
            if extended[0] & 0x80 != 0 {
                return Err(protocol_error(
                    "client frame length used the reserved high bit",
                ));
            }
            u64::from_be_bytes(extended)
        }
        _ => unreachable!(),
    };
    let mut mask = [0; 4];
    stream.read_exact(&mut mask)?;
    if opcode & 0x08 != 0 && (!fin || payload_len > CONTROL_PAYLOAD_BYTES as u64) {
        return Err(protocol_error(
            "invalid fragmented or oversized control frame",
        ));
    }
    Ok(ClientFrameHeader {
        fin,
        opcode,
        payload_len,
        mask,
    })
}

enum ControlDisposition {
    Continue,
    PeerClose,
}

fn consume_control(
    stream: &mut TcpStream,
    header: ClientFrameHeader,
    payload: &mut [u8],
) -> io::Result<ControlDisposition> {
    let len = usize::try_from(header.payload_len)
        .map_err(|_| protocol_error("control payload did not fit usize"))?;
    stream.read_exact(&mut payload[..len])?;
    unmask(&mut payload[..len], header.mask, 0);
    match header.opcode {
        OPCODE_CLOSE => {
            let _ = write_control(stream, OPCODE_CLOSE, &payload[..len]);
            Ok(ControlDisposition::PeerClose)
        }
        OPCODE_PING => {
            write_control(stream, OPCODE_PONG, &payload[..len])?;
            Ok(ControlDisposition::Continue)
        }
        OPCODE_PONG => Ok(ControlDisposition::Continue),
        _ => Err(protocol_error("unknown client control-frame opcode")),
    }
}

fn read_with_eof(stream: &mut TcpStream, output: &mut [u8]) -> io::Result<(usize, bool)> {
    let mut read = 0;
    while read < output.len() {
        match stream.read(&mut output[read..]) {
            Ok(0) => return Ok((read, true)),
            Ok(count) => read += count,
            Err(error) if is_transport_eof(&error) => return Ok((read, true)),
            Err(error) => return Err(error),
        }
    }
    Ok((read, false))
}

fn unmask(payload: &mut [u8], mask: [u8; 4], offset: usize) {
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[(offset + index) % mask.len()];
    }
}

fn aborted(
    request_id: u64,
    compared_bytes: u64,
    frame_count: u64,
    maximum_frame_payload_bytes: usize,
    reason: RequestAbortReason,
) -> RequestOutcome {
    RequestOutcome::Aborted(RequestAbortObservation::new(
        request_id,
        compared_bytes,
        frame_count,
        maximum_frame_payload_bytes,
        reason,
    ))
}

pub(crate) fn write_unmasked_text_message(
    stream: &mut TcpStream,
    mut source: impl Iterator<Item = u8>,
) -> io::Result<()> {
    let mut payload = [0; SERVER_FRAME_PAYLOAD_BYTES];
    let mut pending = source.next();
    let mut first = true;
    loop {
        let mut len = 0;
        if let Some(byte) = pending.take() {
            payload[0] = byte;
            len = 1;
        }
        while len < payload.len() {
            let Some(byte) = source.next() else {
                break;
            };
            payload[len] = byte;
            len += 1;
        }
        if len == payload.len() {
            pending = source.next();
        }
        let fin = pending.is_none();
        write_unmasked_frame(
            stream,
            fin,
            if first {
                OPCODE_TEXT
            } else {
                OPCODE_CONTINUATION
            },
            &payload[..len],
        )?;
        first = false;
        if fin {
            stream.flush()?;
            return Ok(());
        }
    }
}

pub(crate) fn await_masked_client_close(stream: &mut TcpStream) -> io::Result<()> {
    let mut payload = [0; CLIENT_FRAME_PAYLOAD_BYTES];
    loop {
        let header = match read_client_header(stream) {
            Ok(header) => header,
            Err(error) if is_transport_eof(&error) => return Ok(()),
            Err(error) => return Err(error),
        };
        if !header.is_control() {
            return Err(protocol_error(
                "unexpected client data after terminal event",
            ));
        }
        match consume_control(stream, header, &mut payload[..CONTROL_PAYLOAD_BYTES]) {
            Ok(ControlDisposition::Continue) => {}
            Ok(ControlDisposition::PeerClose) => return Ok(()),
            Err(error) if is_transport_eof(&error) => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

fn write_control(stream: &mut TcpStream, opcode: u8, payload: &[u8]) -> io::Result<()> {
    if payload.len() > CONTROL_PAYLOAD_BYTES {
        return Err(protocol_error("server control payload exceeded 125 bytes"));
    }
    write_unmasked_frame(stream, true, opcode, payload)?;
    stream.flush()
}

fn write_unmasked_frame(
    stream: &mut TcpStream,
    fin: bool,
    opcode: u8,
    payload: &[u8],
) -> io::Result<()> {
    let mut header = [0; 10];
    header[0] = if fin { 0x80 | opcode } else { opcode };
    let header_len = match payload.len() {
        0..=125 => {
            header[1] = u8::try_from(payload.len()).unwrap();
            2
        }
        126..=65_535 => {
            header[1] = 126;
            header[2..4].copy_from_slice(&u16::try_from(payload.len()).unwrap().to_be_bytes());
            4
        }
        _ => {
            header[1] = 127;
            header[2..10].copy_from_slice(&u64::try_from(payload.len()).unwrap().to_be_bytes());
            10
        }
    };
    stream.write_all(&header[..header_len])?;
    stream.write_all(payload)
}

fn is_transport_eof(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::BrokenPipe
    )
}

fn protocol_error(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

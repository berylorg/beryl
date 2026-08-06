use std::io::{self, BufRead, Cursor, ErrorKind, Read, Write};

use beryl::diagnostic_startup_gate::{
    DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME, DIAGNOSTIC_ACCEPTANCE_STARTUP_READY_FRAME,
    MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES, enforce_diagnostic_acceptance_startup_gate,
    read_diagnostic_acceptance_startup_gate,
};

#[test]
fn exact_gate_frame_is_accepted() {
    let trailing_input = b"post-gate input";
    let mut input = DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME.to_vec();
    input.extend_from_slice(trailing_input);
    let mut input = TrackingBufRead::new(input);

    read_diagnostic_acceptance_startup_gate(&mut input).unwrap();

    assert_eq!(
        input.consumed,
        DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME.len()
    );
    assert_eq!(input.remaining(), trailing_input);
}

#[test]
fn ready_frame_is_exact_and_flushed_after_gate_validation() {
    let mut output = RecordingWriter::default();

    enforce_diagnostic_acceptance_startup_gate(
        Cursor::new(DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_FRAME),
        &mut output,
    )
    .unwrap();

    assert_eq!(output.bytes, DIAGNOSTIC_ACCEPTANCE_STARTUP_READY_FRAME);
    assert_eq!(output.flushes, 1);
}

#[test]
fn invalid_gate_writes_and_flushes_nothing() {
    let mut output = RecordingWriter::default();

    let error = enforce_diagnostic_acceptance_startup_gate(Cursor::new(b"invalid\n"), &mut output)
        .unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
    assert!(output.bytes.is_empty());
    assert_eq!(output.flushes, 0);
}

#[test]
fn gate_eof_invalid_and_oversize_frames_fail_closed() {
    let cases = [
        (Vec::new(), ErrorKind::UnexpectedEof),
        (b"invalid\n".to_vec(), ErrorKind::InvalidData),
        (b"unterminated".to_vec(), ErrorKind::UnexpectedEof),
        (
            format!(
                "{}\n",
                "x".repeat(MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES)
            )
            .into_bytes(),
            ErrorKind::InvalidData,
        ),
    ];
    for (frame, expected_kind) in cases {
        let error = read_diagnostic_acceptance_startup_gate(Cursor::new(frame)).unwrap_err();
        assert_eq!(error.kind(), expected_kind);
    }
}

#[test]
fn oversized_unterminated_gate_consumes_only_one_detection_byte() {
    let input_len = MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES + 1_024;
    let mut input = TrackingBufRead::new(vec![b'x'; input_len]);

    let error = read_diagnostic_acceptance_startup_gate(&mut input).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
    assert_eq!(
        input.consumed,
        MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES + 1
    );
    assert_eq!(
        input.remaining().len(),
        input_len - MAX_DIAGNOSTIC_ACCEPTANCE_STARTUP_GATE_BYTES - 1
    );
}

struct TrackingBufRead {
    inner: Cursor<Vec<u8>>,
    consumed: usize,
}

impl TrackingBufRead {
    fn new(input: Vec<u8>) -> Self {
        Self {
            inner: Cursor::new(input),
            consumed: 0,
        }
    }

    fn remaining(&self) -> &[u8] {
        &self.inner.get_ref()[self.inner.position() as usize..]
    }
}

impl Read for TrackingBufRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.consumed += read;
        Ok(read)
    }
}

impl BufRead for TrackingBufRead {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.consumed += amount;
        self.inner.consume(amount);
    }
}

#[derive(Default)]
struct RecordingWriter {
    bytes: Vec<u8>,
    flushes: usize,
}

impl Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

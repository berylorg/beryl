use std::{
    fmt,
    io::{self, Write},
};

use serde::{
    Serialize, Serializer,
    ser::{Error as _, SerializeSeq},
};
use serde_json::json;

#[path = "../src/session/outbound.rs"]
#[allow(
    dead_code,
    reason = "the integration test includes the production writer core without its session adapters"
)]
mod outbound;

use outbound::{
    DispatchProgress, OutboundWriteFailure, STDIO_OUTBOUND_BUFFER_BYTES, StdioJsonWriter,
    write_json,
};

#[derive(Default)]
struct RecordingSink {
    bytes: Vec<u8>,
    maximum_write_bytes: usize,
    flushes: usize,
}

impl Write for RecordingSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.maximum_write_bytes = self.maximum_write_bytes.max(bytes.len());
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

#[derive(Default)]
struct CountingSink {
    bytes: usize,
    maximum_write_bytes: usize,
    flushes: usize,
}

impl Write for CountingSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes += bytes.len();
        self.maximum_write_bytes = self.maximum_write_bytes.max(bytes.len());
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

struct CutSink {
    bytes: Vec<u8>,
    fail_after: usize,
    failed: bool,
}

struct FailBeforeWrite;

impl Serialize for FailBeforeWrite {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Err(S::Error::custom("injected pre-write serialization failure"))
    }
}

struct FailAfterLargeText;

impl Serialize for FailAfterLargeText {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        sequence.serialize_element(&"x".repeat(STDIO_OUTBOUND_BUFFER_BYTES * 2))?;
        Err(S::Error::custom(
            "injected post-write serialization failure",
        ))
    }
}

struct FragmentedDisplay;

impl fmt::Display for FragmentedDisplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for _ in 0..(STDIO_OUTBOUND_BUFFER_BYTES / 4) {
            formatter.write_str("x\nŽ")?;
        }
        Ok(())
    }
}

impl Serialize for FragmentedDisplay {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl CutSink {
    fn new(fail_after: usize) -> Self {
        Self {
            bytes: Vec::new(),
            fail_after,
            failed: false,
        }
    }
}

impl Write for CutSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.failed && self.bytes.len() >= self.fail_after {
            self.failed = true;
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "injected cut"));
        }
        let allowed = if self.failed {
            bytes.len()
        } else {
            bytes.len().min(self.fail_after - self.bytes.len())
        };
        self.bytes.extend_from_slice(&bytes[..allowed]);
        Ok(allowed)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn stdio_writer_emits_exact_compact_json_and_one_newline() {
    let message = json!({
        "text": "quotes \" slashes \\ controls \n non-ASCII Žluťoučký",
        "enabled": true
    });
    let mut expected = serde_json::to_vec(&message).unwrap();
    expected.push(b'\n');

    let mut sink = RecordingSink::default();
    let metrics = write_json(&mut StdioJsonWriter::new(&mut sink), &message).unwrap();

    assert_eq!(sink.bytes, expected);
    assert_eq!(metrics.bytes + 1, sink.bytes.len());
    assert_eq!(sink.flushes, 1);
    assert!(sink.maximum_write_bytes <= STDIO_OUTBOUND_BUFFER_BYTES);
}

#[test]
fn stdio_writer_memory_and_write_size_stay_fixed_for_large_json() {
    let text = "x".repeat(STDIO_OUTBOUND_BUFFER_BYTES * 257 + 37);
    let message = json!({ "text": text });
    let expected_json_bytes = text.len() + r#"{"text":""}"#.len();
    let mut sink = CountingSink::default();

    let metrics = write_json(&mut StdioJsonWriter::new(&mut sink), &message).unwrap();

    assert_eq!(metrics.bytes, expected_json_bytes);
    assert_eq!(sink.bytes, expected_json_bytes + 1);
    assert_eq!(sink.flushes, 1);
    assert!(sink.maximum_write_bytes <= STDIO_OUTBOUND_BUFFER_BYTES);
}

#[test]
fn stdio_writer_accepts_streamed_collect_str_fragments() {
    let expected = serde_json::to_vec(&FragmentedDisplay).unwrap();
    let mut sink = RecordingSink::default();

    let metrics = write_json(&mut StdioJsonWriter::new(&mut sink), &FragmentedDisplay).unwrap();

    assert_eq!(metrics.bytes, expected.len());
    assert_eq!(&sink.bytes[..expected.len()], expected);
    assert_eq!(sink.bytes.last(), Some(&b'\n'));
    assert!(sink.maximum_write_bytes <= STDIO_OUTBOUND_BUFFER_BYTES);
}

#[test]
fn stdio_writer_holds_an_exact_buffer_until_newline_finalization() {
    let text = "x".repeat(STDIO_OUTBOUND_BUFFER_BYTES - 2);
    let mut sink = RecordingSink::default();

    let metrics = write_json(&mut StdioJsonWriter::new(&mut sink), &text).unwrap();

    assert_eq!(metrics.bytes, STDIO_OUTBOUND_BUFFER_BYTES);
    assert_eq!(sink.bytes.len(), STDIO_OUTBOUND_BUFFER_BYTES + 1);
    assert_eq!(sink.maximum_write_bytes, STDIO_OUTBOUND_BUFFER_BYTES);
    assert_eq!(sink.bytes.last(), Some(&b'\n'));
}

#[test]
fn zero_byte_stdio_failure_is_proven_before_dispatch_and_reusable() {
    let mut sink = CutSink::new(0);
    let first = write_json(
        &mut StdioJsonWriter::new(&mut sink),
        &json!({ "first": true }),
    );

    assert!(matches!(
        first,
        Err(OutboundWriteFailure::Transport {
            progress: DispatchProgress::NeverWritten,
            ..
        })
    ));
    assert!(sink.bytes.is_empty());

    let second_message = json!({ "second": true });
    write_json(&mut StdioJsonWriter::new(&mut sink), &second_message).unwrap();
    let mut expected = serde_json::to_vec(&second_message).unwrap();
    expected.push(b'\n');
    assert_eq!(sink.bytes, expected);
}

#[test]
fn partial_stdio_prefix_failure_is_completion_unknown_evidence() {
    let mut sink = CutSink::new(7);
    let result = write_json(
        &mut StdioJsonWriter::new(&mut sink),
        &json!({ "request": "must not be followed by another line" }),
    );

    assert!(matches!(
        result,
        Err(OutboundWriteFailure::Transport {
            progress: DispatchProgress::SomeBytes,
            ..
        })
    ));
    assert_eq!(sink.bytes.len(), 7);
    assert!(!sink.bytes.ends_with(b"\n"));
}

#[test]
fn serialization_failure_before_transport_bytes_leaves_stdio_reusable() {
    let mut sink = RecordingSink::default();
    let first = write_json(&mut StdioJsonWriter::new(&mut sink), &FailBeforeWrite);

    assert!(matches!(
        first,
        Err(OutboundWriteFailure::Serialize {
            progress: DispatchProgress::NeverWritten,
            ..
        })
    ));
    assert!(sink.bytes.is_empty());

    write_json(
        &mut StdioJsonWriter::new(&mut sink),
        &json!({ "later": "request" }),
    )
    .unwrap();
    assert!(sink.bytes.ends_with(b"\n"));
}

#[test]
fn serialization_failure_after_transport_bytes_is_completion_unknown_evidence() {
    let mut sink = RecordingSink::default();
    let result = write_json(&mut StdioJsonWriter::new(&mut sink), &FailAfterLargeText);

    assert!(matches!(
        result,
        Err(OutboundWriteFailure::Serialize {
            progress: DispatchProgress::SomeBytes,
            ..
        })
    ));
    assert!(sink.bytes.len() >= STDIO_OUTBOUND_BUFFER_BYTES);
    assert!(!sink.bytes.ends_with(b"\n"));
}

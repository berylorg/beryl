use super::*;
use crate::ThreadClosed;

pub(super) struct ThreadClosedMachine<'a> {
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    expected: Expected,
    scalar: Scalar,
    thread_id: Option<CasThreadId>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Expected {
    RootParamsName,
    ParamsObject,
    ParamsThreadName,
    ThreadValue,
    ParamsEnd,
    RootEnd,
    Done,
}

enum Scalar {
    None,
    Name {
        matched: bool,
        len: usize,
        expected: &'static [u8],
        next: Expected,
    },
    Thread {
        bytes: IdentityBytes,
        next: Expected,
    },
}

impl<'a> ThreadClosedMachine<'a> {
    pub(super) fn new(sink: Option<&'a mut dyn OrderedTurnStreamSink>) -> Self {
        Self {
            sink,
            expected: Expected::RootParamsName,
            scalar: Scalar::None,
            thread_id: None,
        }
    }

    pub(super) fn scratch_bytes(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let accepted = match &mut self.scalar {
            Scalar::Name {
                matched,
                len,
                expected,
                ..
            } => {
                for byte in bytes {
                    if expected.get(*len) != Some(byte) {
                        *matched = false;
                    }
                    *len = len.saturating_add(1);
                }
                true
            }
            Scalar::Thread { bytes: fixed, .. } => fixed.push(bytes),
            Scalar::None => bytes.is_empty(),
        };
        accepted.then_some(()).ok_or_else(malformed)
    }

    pub(super) fn event(&mut self, event: Event) -> Result<(), MachineError> {
        if !matches!(self.scalar, Scalar::None) {
            return self.scalar_event(event);
        }
        match self.expected {
            Expected::RootParamsName => self.start_name(event, b"params", Expected::ParamsObject),
            Expected::ParamsObject => self.start_object(event, Expected::ParamsThreadName),
            Expected::ParamsThreadName => {
                self.start_name(event, b"threadId", Expected::ThreadValue)
            }
            Expected::ThreadValue => self.start_thread(event, Expected::ParamsEnd),
            Expected::ParamsEnd => self.end_object(event, Expected::RootEnd),
            Expected::RootEnd => self.end_object(event, Expected::Done),
            Expected::Done => Err(malformed()),
        }
    }

    fn scalar_event(&mut self, event: Event) -> Result<(), MachineError> {
        let expected_kind = match self.scalar {
            Scalar::Name { .. } => ScalarKind::Name,
            Scalar::Thread { .. } => ScalarKind::String,
            Scalar::None => return Err(malformed()),
        };
        match event {
            Event::ScalarFragment(kind) if kind == expected_kind => Ok(()),
            Event::ScalarEnd(kind) if kind == expected_kind => self.finish_scalar(),
            _ => Err(malformed()),
        }
    }

    fn finish_scalar(&mut self) -> Result<(), MachineError> {
        match std::mem::replace(&mut self.scalar, Scalar::None) {
            Scalar::Name {
                matched: true,
                len,
                expected,
                next,
            } if len == expected.len() => {
                self.expected = next;
                Ok(())
            }
            Scalar::Thread { bytes, next } => {
                self.thread_id = Some(CasThreadId::new(bytes.as_str()?).map_err(|_| malformed())?);
                self.expected = next;
                Ok(())
            }
            _ => Err(malformed()),
        }
    }

    fn start_name(
        &mut self,
        event: Event,
        expected: &'static [u8],
        next: Expected,
    ) -> Result<(), MachineError> {
        if event != Event::ScalarStart(ScalarKind::Name) {
            return Err(malformed());
        }
        self.scalar = Scalar::Name {
            matched: true,
            len: 0,
            expected,
            next,
        };
        Ok(())
    }

    fn start_thread(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::ScalarStart(ScalarKind::String) {
            return Err(malformed());
        }
        self.scalar = Scalar::Thread {
            bytes: IdentityBytes::new(),
            next,
        };
        Ok(())
    }

    fn start_object(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::ContainerStart(ContainerKind::Object) {
            return Err(malformed());
        }
        self.expected = next;
        Ok(())
    }

    fn end_object(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::ContainerEnd(ContainerKind::Object) {
            return Err(malformed());
        }
        self.expected = next;
        Ok(())
    }

    pub(super) fn map_parse_failure(&self, failure: ParseFailure) -> DecodeReaderError {
        json_failure(failure)
    }

    pub(super) fn finish(&mut self) -> Result<DecodedIncoming, MachineError> {
        if self.expected != Expected::Done || !matches!(self.scalar, Scalar::None) {
            return Err(malformed());
        }
        let operation = OrderedTurnStreamOperation::ThreadClosed(ThreadClosed::decoded(
            self.thread_id.take().ok_or_else(malformed)?,
        ));
        let Some(sink) = self.sink.as_deref_mut() else {
            return Err(MachineError::Ordered(Box::new(
                OrderedTurnStreamSubmitError::new(
                    operation,
                    OrderedTurnStreamSubmitCause::Unavailable,
                ),
            )));
        };
        match sink.submit(operation) {
            Ok(OrderedTurnStreamCompletion::Applied) => Ok(DecodedIncoming::OrderedHandled),
            Ok(_) => Err(MachineError::OrderedUnexpectedCompletion),
            Err(source) => Err(MachineError::Ordered(Box::new(source))),
        }
    }
}

struct IdentityBytes {
    bytes: [u8; crate::PROTOCOL_IDENTITY_MAX_BYTES],
    len: usize,
}

impl IdentityBytes {
    const fn new() -> Self {
        Self {
            bytes: [0; crate::PROTOCOL_IDENTITY_MAX_BYTES],
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> bool {
        let Some(end) = self.len.checked_add(bytes.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
        true
    }

    fn as_str(&self) -> Result<&str, MachineError> {
        std::str::from_utf8(&self.bytes[..self.len]).map_err(|_| malformed())
    }
}

fn malformed() -> MachineError {
    ForegroundIngressError::MalformedThreadClosed.into()
}

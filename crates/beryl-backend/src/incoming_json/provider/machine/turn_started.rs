use super::*;
use crate::TurnStarted;

pub(super) struct TurnStartedMachine<'a> {
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    expected: Expected,
    scalar: Scalar,
    thread_id: Option<CasThreadId>,
    turn_id: Option<CasTurnId>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Expected {
    RootParamsName,
    ParamsObject,
    ParamsThreadName,
    ThreadValue,
    ParamsTurnName,
    TurnObject,
    TurnIdName,
    TurnIdValue,
    ItemsName,
    ItemsArray,
    ItemsEnd,
    ItemsViewName,
    ItemsViewValue,
    StatusName,
    StatusValue,
    ErrorName,
    ErrorValue,
    StartedAtName,
    StartedAtValue,
    CompletedAtName,
    CompletedAtValue,
    DurationMsName,
    DurationMsValue,
    TurnEnd,
    ParamsEnd,
    RootEnd,
    Done,
}

enum Scalar {
    None,
    Name {
        probe: Probe,
        expected: &'static [u8],
        next: Expected,
    },
    Identity {
        bytes: IdentityBytes,
        thread: bool,
        next: Expected,
    },
    Choice {
        probe: Probe,
        expected: &'static [u8],
        next: Expected,
    },
    Integer {
        valid: bool,
        saw_digit: bool,
        next: Expected,
    },
}

impl<'a> TurnStartedMachine<'a> {
    pub(super) fn new(sink: Option<&'a mut dyn OrderedTurnStreamSink>) -> Self {
        Self {
            sink,
            expected: Expected::RootParamsName,
            scalar: Scalar::None,
            thread_id: None,
            turn_id: None,
        }
    }

    pub(super) fn scratch_bytes(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let accepted = match &mut self.scalar {
            Scalar::Name {
                probe, expected, ..
            }
            | Scalar::Choice {
                probe, expected, ..
            } => {
                probe.push(bytes, &[*expected]);
                true
            }
            Scalar::Identity { bytes: fixed, .. } => fixed.push(bytes),
            Scalar::Integer {
                valid, saw_digit, ..
            } => {
                for byte in bytes {
                    if byte.is_ascii_digit() {
                        *saw_digit = true;
                    } else if *byte != b'-' || *saw_digit {
                        *valid = false;
                    }
                }
                true
            }
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
            Expected::ThreadValue => self.start_identity(event, true, Expected::ParamsTurnName),
            Expected::ParamsTurnName => self.start_name(event, b"turn", Expected::TurnObject),
            Expected::TurnObject => self.start_object(event, Expected::TurnIdName),
            Expected::TurnIdName => self.start_name(event, b"id", Expected::TurnIdValue),
            Expected::TurnIdValue => self.start_identity(event, false, Expected::ItemsName),
            Expected::ItemsName => self.start_name(event, b"items", Expected::ItemsArray),
            Expected::ItemsArray => self.start_array(event, Expected::ItemsEnd),
            Expected::ItemsEnd => self.end_array(event, Expected::ItemsViewName),
            Expected::ItemsViewName => {
                self.start_name(event, b"itemsView", Expected::ItemsViewValue)
            }
            Expected::ItemsViewValue => {
                self.start_choice(event, b"notLoaded", Expected::StatusName)
            }
            Expected::StatusName => self.start_name(event, b"status", Expected::StatusValue),
            Expected::StatusValue => self.start_choice(event, b"inProgress", Expected::ErrorName),
            Expected::ErrorName => self.start_name(event, b"error", Expected::ErrorValue),
            Expected::ErrorValue => self.expect_null(event, Expected::StartedAtName),
            Expected::StartedAtName => {
                self.start_name(event, b"startedAt", Expected::StartedAtValue)
            }
            Expected::StartedAtValue => {
                self.start_optional_integer(event, Expected::CompletedAtName)
            }
            Expected::CompletedAtName => {
                self.start_name(event, b"completedAt", Expected::CompletedAtValue)
            }
            Expected::CompletedAtValue => self.expect_null(event, Expected::DurationMsName),
            Expected::DurationMsName => {
                self.start_name(event, b"durationMs", Expected::DurationMsValue)
            }
            Expected::DurationMsValue => self.expect_null(event, Expected::TurnEnd),
            Expected::TurnEnd => self.end_object(event, Expected::ParamsEnd),
            Expected::ParamsEnd => self.end_object(event, Expected::RootEnd),
            Expected::RootEnd => self.end_object(event, Expected::Done),
            Expected::Done => Err(malformed()),
        }
    }

    fn scalar_event(&mut self, event: Event) -> Result<(), MachineError> {
        let kind = match self.scalar {
            Scalar::Name { .. } => ScalarKind::Name,
            Scalar::Identity { .. } | Scalar::Choice { .. } => ScalarKind::String,
            Scalar::Integer { .. } => ScalarKind::Number,
            Scalar::None => return Err(malformed()),
        };
        match event {
            Event::ScalarFragment(actual) if actual == kind => Ok(()),
            Event::ScalarEnd(actual) if actual == kind => self.finish_scalar(),
            _ => Err(malformed()),
        }
    }

    fn finish_scalar(&mut self) -> Result<(), MachineError> {
        match std::mem::replace(&mut self.scalar, Scalar::None) {
            Scalar::Name {
                probe,
                expected,
                next,
            }
            | Scalar::Choice {
                probe,
                expected,
                next,
            } if probe.exact(expected.len()) => {
                self.expected = next;
                Ok(())
            }
            Scalar::Identity {
                bytes,
                thread,
                next,
            } => {
                if thread {
                    self.thread_id =
                        Some(CasThreadId::new(bytes.as_str()?).map_err(|_| malformed())?);
                } else {
                    self.turn_id = Some(CasTurnId::new(bytes.as_str()?).map_err(|_| malformed())?);
                }
                self.expected = next;
                Ok(())
            }
            Scalar::Integer {
                valid: true,
                saw_digit: true,
                next,
            } => {
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
            probe: Probe::new(),
            expected,
            next,
        };
        Ok(())
    }

    fn start_identity(
        &mut self,
        event: Event,
        thread: bool,
        next: Expected,
    ) -> Result<(), MachineError> {
        if event != Event::ScalarStart(ScalarKind::String) {
            return Err(malformed());
        }
        self.scalar = Scalar::Identity {
            bytes: IdentityBytes::new(),
            thread,
            next,
        };
        Ok(())
    }

    fn start_choice(
        &mut self,
        event: Event,
        expected: &'static [u8],
        next: Expected,
    ) -> Result<(), MachineError> {
        if event != Event::ScalarStart(ScalarKind::String) {
            return Err(malformed());
        }
        self.scalar = Scalar::Choice {
            probe: Probe::new(),
            expected,
            next,
        };
        Ok(())
    }

    fn start_optional_integer(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        match event {
            Event::Null => {
                self.expected = next;
                Ok(())
            }
            Event::ScalarStart(ScalarKind::Number) => {
                self.scalar = Scalar::Integer {
                    valid: true,
                    saw_digit: false,
                    next,
                };
                Ok(())
            }
            _ => Err(malformed()),
        }
    }

    fn expect_null(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::Null {
            return Err(malformed());
        }
        self.expected = next;
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

    fn start_array(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::ContainerStart(ContainerKind::Array) {
            return Err(malformed());
        }
        self.expected = next;
        Ok(())
    }

    fn end_array(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::ContainerEnd(ContainerKind::Array) {
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
        let operation = OrderedTurnStreamOperation::TurnStarted(TurnStarted::decoded(
            self.thread_id.take().ok_or_else(malformed)?,
            self.turn_id.take().ok_or_else(malformed)?,
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

struct Probe {
    matched: bool,
    len: usize,
}

impl Probe {
    const fn new() -> Self {
        Self {
            matched: true,
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8], wires: &[&[u8]]) {
        let expected = wires[0];
        for byte in bytes {
            if expected.get(self.len) != Some(byte) {
                self.matched = false;
            }
            self.len = self.len.saturating_add(1);
        }
    }

    const fn exact(&self, len: usize) -> bool {
        self.matched && self.len == len
    }
}

fn malformed() -> MachineError {
    ForegroundIngressError::MalformedTurnStarted.into()
}

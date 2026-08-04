use super::*;
use crate::{LoadedThreadStatus, ThreadActiveFlags, ThreadStatusChanged};

pub(super) struct ThreadStatusChangedMachine<'a> {
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    expected: Expected,
    scalar: Scalar,
    thread_id: Option<CasThreadId>,
    status: Option<LoadedThreadStatus>,
    waiting_on_approval: bool,
    waiting_on_user_input: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Expected {
    RootParamsName,
    ParamsObject,
    ParamsThreadName,
    ThreadValue,
    ParamsStatusName,
    StatusObject,
    StatusTypeName,
    StatusTypeValue,
    StatusTail,
    ActiveFlagsValue,
    ActiveFlagOrEnd,
    StatusEnd,
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
    Thread {
        bytes: IdentityBytes,
        next: Expected,
    },
    Status(Probe),
    ActiveFlag(Probe),
}

impl<'a> ThreadStatusChangedMachine<'a> {
    pub(super) fn new(sink: Option<&'a mut dyn OrderedTurnStreamSink>) -> Self {
        Self {
            sink,
            expected: Expected::RootParamsName,
            scalar: Scalar::None,
            thread_id: None,
            status: None,
            waiting_on_approval: false,
            waiting_on_user_input: false,
        }
    }

    pub(super) fn scratch_bytes(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let accepted = match &mut self.scalar {
            Scalar::Name {
                probe, expected, ..
            } => {
                probe.push(bytes, &[*expected]);
                true
            }
            Scalar::Thread { bytes: fixed, .. } => fixed.push(bytes),
            Scalar::Status(probe) => {
                probe.push(bytes, &STATUS_WIRES);
                true
            }
            Scalar::ActiveFlag(probe) => {
                probe.push(bytes, &ACTIVE_FLAG_WIRES);
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
            Expected::ThreadValue => self.start_thread(event, Expected::ParamsStatusName),
            Expected::ParamsStatusName => self.start_name(event, b"status", Expected::StatusObject),
            Expected::StatusObject => self.start_object(event, Expected::StatusTypeName),
            Expected::StatusTypeName => self.start_name(event, b"type", Expected::StatusTypeValue),
            Expected::StatusTypeValue => self.start_status(event),
            Expected::StatusTail => match self.status {
                Some(LoadedThreadStatus::Active { .. }) => {
                    self.start_name(event, b"activeFlags", Expected::ActiveFlagsValue)
                }
                Some(LoadedThreadStatus::Idle | LoadedThreadStatus::SystemError) => {
                    self.end_object(event, Expected::ParamsEnd)
                }
                None => Err(malformed()),
            },
            Expected::ActiveFlagsValue => self.start_array(event, Expected::ActiveFlagOrEnd),
            Expected::ActiveFlagOrEnd => match event {
                Event::ContainerEnd(ContainerKind::Array) => {
                    self.expected = Expected::StatusEnd;
                    Ok(())
                }
                Event::ScalarStart(ScalarKind::String) => {
                    self.scalar = Scalar::ActiveFlag(Probe::new(&ACTIVE_FLAG_WIRES));
                    Ok(())
                }
                _ => Err(malformed()),
            },
            Expected::StatusEnd => self.end_object(event, Expected::ParamsEnd),
            Expected::ParamsEnd => self.end_object(event, Expected::RootEnd),
            Expected::RootEnd => self.end_object(event, Expected::Done),
            Expected::Done => Err(malformed()),
        }
    }

    fn scalar_event(&mut self, event: Event) -> Result<(), MachineError> {
        let expected_kind = match self.scalar {
            Scalar::Name { .. } => ScalarKind::Name,
            Scalar::Thread { .. } | Scalar::Status(_) | Scalar::ActiveFlag(_) => ScalarKind::String,
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
                probe,
                expected,
                next,
            } if probe.exact(0, expected.len()) => {
                self.expected = next;
                Ok(())
            }
            Scalar::Thread { bytes, next } => {
                self.thread_id = Some(CasThreadId::new(bytes.as_str()?).map_err(|_| malformed())?);
                self.expected = next;
                Ok(())
            }
            Scalar::Status(probe) => {
                self.status = Some(match probe.finish(&STATUS_WIRES) {
                    Some(0) => LoadedThreadStatus::active(ThreadActiveFlags::empty()),
                    Some(1) => LoadedThreadStatus::Idle,
                    Some(2) => LoadedThreadStatus::SystemError,
                    _ => return Err(malformed()),
                });
                self.expected = Expected::StatusTail;
                Ok(())
            }
            Scalar::ActiveFlag(probe) => {
                match probe.finish(&ACTIVE_FLAG_WIRES) {
                    Some(0) => self.waiting_on_approval = true,
                    Some(1) => self.waiting_on_user_input = true,
                    None => {}
                    Some(_) => unreachable!(),
                }
                self.expected = Expected::ActiveFlagOrEnd;
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
            probe: Probe::new(&[expected]),
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

    fn start_status(&mut self, event: Event) -> Result<(), MachineError> {
        if event != Event::ScalarStart(ScalarKind::String) {
            return Err(malformed());
        }
        self.scalar = Scalar::Status(Probe::new(&STATUS_WIRES));
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

    pub(super) fn map_parse_failure(&self, failure: ParseFailure) -> DecodeReaderError {
        json_failure(failure)
    }

    pub(super) fn finish(&mut self) -> Result<DecodedIncoming, MachineError> {
        if self.expected != Expected::Done || !matches!(self.scalar, Scalar::None) {
            return Err(malformed());
        }
        let thread_id = self.thread_id.take().ok_or_else(malformed)?;
        let mut status = self.status.ok_or_else(malformed)?;
        if matches!(status, LoadedThreadStatus::Active { .. }) {
            status = LoadedThreadStatus::active(ThreadActiveFlags::new(
                self.waiting_on_approval,
                self.waiting_on_user_input,
            ));
        }
        let operation = OrderedTurnStreamOperation::ThreadStatusChanged(
            ThreadStatusChanged::decoded(thread_id, status),
        );
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
    candidates: u16,
    len: usize,
}

impl Probe {
    fn new(wires: &[&[u8]]) -> Self {
        Self {
            candidates: (1_u16 << wires.len()) - 1,
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8], wires: &[&[u8]]) {
        for byte in bytes {
            for (index, wire) in wires.iter().enumerate() {
                let bit = 1_u16 << index;
                if self.candidates & bit != 0 && wire.get(self.len) != Some(byte) {
                    self.candidates &= !bit;
                }
            }
            self.len = self.len.saturating_add(1);
        }
    }

    fn exact(&self, index: usize, len: usize) -> bool {
        self.candidates & (1_u16 << index) != 0 && self.len == len
    }

    fn finish(&self, wires: &[&[u8]]) -> Option<usize> {
        wires.iter().enumerate().find_map(|(index, wire)| {
            (self.candidates & (1_u16 << index) != 0 && self.len == wire.len()).then_some(index)
        })
    }
}

const STATUS_WIRES: [&[u8]; 3] = [b"active", b"idle", b"systemError"];
const ACTIVE_FLAG_WIRES: [&[u8]; 2] = [b"waitingOnApproval", b"waitingOnUserInput"];

fn malformed() -> MachineError {
    ForegroundIngressError::MalformedThreadStatusChanged.into()
}

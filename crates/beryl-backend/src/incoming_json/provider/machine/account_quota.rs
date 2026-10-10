use super::*;

pub(super) struct AccountQuotaMachine<'a> {
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    step: u8,
    name: Option<Name>,
    field: Field,
    snapshot_seen: u8,
    window_seen: u8,
    scalar: Option<ScalarKind>,
    integer: bool,
    discard: Option<ValueTracker>,
    envelope: super::notification_envelope::NotificationEnvelopeTail,
}

struct Name {
    bytes: [u8; 32],
    len: usize,
}

#[derive(Clone, Copy)]
enum Field {
    Unknown,
    Identity,
    Window,
    Integer,
    OptionalInteger,
}

impl<'a> AccountQuotaMachine<'a> {
    pub(super) fn new(sink: Option<&'a mut dyn OrderedTurnStreamSink>) -> Self {
        Self {
            sink,
            step: 0,
            name: None,
            field: Field::Unknown,
            snapshot_seen: 0,
            window_seen: 0,
            scalar: None,
            integer: true,
            discard: None,
            envelope: Default::default(),
        }
    }

    pub(super) fn scratch_bytes(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        if self.step == 7 {
            return self.envelope.scratch_bytes(bytes);
        }
        if let Some(name) = &mut self.name {
            for byte in bytes {
                if let Some(slot) = name.bytes.get_mut(name.len) {
                    *slot = *byte;
                }
                name.len = name.len.saturating_add(1);
            }
        } else if self.scalar == Some(ScalarKind::Number) {
            self.integer &= bytes
                .iter()
                .all(|byte| byte.is_ascii_digit() || *byte == b'-');
        }
        Ok(())
    }

    pub(super) fn event(&mut self, event: Event) -> Result<(), MachineError> {
        if self.step == 7 {
            return self.envelope.event(event);
        }
        if let Some(tracker) = &mut self.discard {
            if !tracker.event(event) {
                return Err(malformed());
            }
            if tracker.is_complete() {
                self.discard = None;
            }
            return Ok(());
        }
        if self.name.is_some() {
            return match event {
                Event::ScalarFragment(ScalarKind::Name) => Ok(()),
                Event::ScalarEnd(ScalarKind::Name) => self.finish_name(),
                _ => Err(malformed()),
            };
        }
        if let Some(kind) = self.scalar {
            return match event {
                Event::ScalarFragment(actual) if actual == kind => Ok(()),
                Event::ScalarEnd(actual)
                    if actual == kind && (kind != ScalarKind::Number || self.integer) =>
                {
                    self.scalar = None;
                    Ok(())
                }
                _ => Err(malformed()),
            };
        }
        match (self.step, event) {
            (0 | 2 | 4 | 5, Event::ScalarStart(ScalarKind::Name)) => {
                self.name = Some(Name {
                    bytes: [0; 32],
                    len: 0,
                })
            }
            (1, Event::ContainerStart(ContainerKind::Object)) => self.step = 2,
            (3, Event::ContainerStart(ContainerKind::Object)) => self.step = 4,
            (4, Event::ContainerEnd(ContainerKind::Object)) => self.step = 6,
            (5, Event::ContainerEnd(ContainerKind::Object)) if self.window_seen & 1 != 0 => {
                self.step = 4
            }
            (6, Event::ContainerEnd(ContainerKind::Object)) => self.step = 7,
            (8 | 9, _) => {
                let next = if self.step == 8 { 4 } else { 5 };
                match (self.field, event) {
                    (Field::Window, Event::ContainerStart(ContainerKind::Object)) => {
                        self.window_seen = 0;
                        self.step = 5;
                        return Ok(());
                    }
                    (Field::Window | Field::Identity | Field::OptionalInteger, Event::Null) => {}
                    (Field::Identity, Event::ScalarStart(ScalarKind::String)) => {
                        self.scalar = Some(ScalarKind::String)
                    }
                    (
                        Field::Integer | Field::OptionalInteger,
                        Event::ScalarStart(ScalarKind::Number),
                    ) => {
                        self.scalar = Some(ScalarKind::Number);
                        self.integer = true;
                    }
                    (Field::Unknown, _) => {
                        let mut tracker = ValueTracker::new();
                        if !tracker.event(event) {
                            return Err(malformed());
                        }
                        if !tracker.is_complete() {
                            self.discard = Some(tracker);
                        }
                    }
                    _ => return Err(malformed()),
                }
                self.step = next;
            }
            _ => return Err(malformed()),
        }
        Ok(())
    }

    fn finish_name(&mut self) -> Result<(), MachineError> {
        let name = self.name.take().ok_or_else(malformed)?;
        let bytes = name.bytes.get(..name.len).unwrap_or(&[]);
        if self.step == 0 || self.step == 2 {
            if bytes
                != if self.step == 0 {
                    b"params".as_slice()
                } else {
                    b"rateLimits".as_slice()
                }
            {
                return Err(malformed());
            }
            self.step += 1;
            return Ok(());
        }
        let (field, bit) = if self.step == 4 {
            match bytes {
                b"limitId" => (Field::Identity, 1),
                b"limitName" => (Field::Identity, 2),
                b"primary" => (Field::Window, 4),
                b"secondary" => (Field::Window, 8),
                _ => (Field::Unknown, 0),
            }
        } else {
            match bytes {
                b"usedPercent" => (Field::Integer, 1),
                b"windowDurationMins" => (Field::OptionalInteger, 2),
                b"resetsAt" => (Field::OptionalInteger, 4),
                _ => (Field::Unknown, 0),
            }
        };
        let seen = if self.step == 4 {
            &mut self.snapshot_seen
        } else {
            &mut self.window_seen
        };
        if *seen & bit != 0 {
            return Err(malformed());
        }
        *seen |= bit;
        self.field = field;
        self.step = if self.step == 4 { 8 } else { 9 };
        Ok(())
    }

    pub(super) fn map_parse_failure(&self, failure: ParseFailure) -> DecodeReaderError {
        json_failure(failure)
    }

    pub(super) fn finish(&mut self) -> Result<DecodedIncoming, MachineError> {
        if self.step != 7 || !self.envelope.complete() || self.name.is_some() {
            return Err(malformed());
        }
        let operation = OrderedTurnStreamOperation::AccountQuotaObservation(
            crate::AccountQuotaObservation::Unavailable,
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
            Err(error) => Err(MachineError::Ordered(Box::new(error))),
        }
    }
}

fn malformed() -> MachineError {
    ForegroundIngressError::MalformedContextObservation.into()
}

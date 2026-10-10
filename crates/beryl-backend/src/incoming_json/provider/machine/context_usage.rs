use super::*;
use crate::{ContextTokenBreakdown, ContextTokenUsage, ThreadContextObservation};

pub(super) struct ContextUsageMachine<'a> {
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    step: usize,
    scalar: Option<Scalar>,
    thread_id: Option<CasThreadId>,
    turn_id: Option<CasTurnId>,
    counters: [[i64; 6]; 2],
    available: bool,
    window: Option<i64>,
    envelope: super::notification_envelope::NotificationEnvelopeTail,
}

enum Instruction {
    Name(&'static [u8]),
    Object,
    End,
    Identity,
    Number,
    Window,
    Done,
}

enum Scalar {
    Bytes {
        kind: ScalarKind,
        bytes: [u8; crate::PROTOCOL_IDENTITY_MAX_BYTES],
        len: usize,
    },
    Number {
        value: u64,
        negative: bool,
        overflow: bool,
        non_integer: bool,
    },
}

impl<'a> ContextUsageMachine<'a> {
    pub(super) fn new(sink: Option<&'a mut dyn OrderedTurnStreamSink>) -> Self {
        Self {
            sink,
            step: 0,
            scalar: None,
            thread_id: None,
            turn_id: None,
            counters: [[0; 6]; 2],
            available: true,
            window: None,
            envelope: Default::default(),
        }
    }

    pub(super) fn scratch_bytes(&mut self, fragment: &[u8]) -> Result<(), MachineError> {
        if self.step == 42 {
            return self.envelope.scratch_bytes(fragment);
        }
        match self.scalar.as_mut() {
            Some(Scalar::Bytes { bytes, len, .. }) => {
                let end = len.checked_add(fragment.len()).ok_or_else(malformed)?;
                if end > bytes.len() {
                    return Err(malformed());
                }
                bytes[*len..end].copy_from_slice(fragment);
                *len = end;
            }
            Some(Scalar::Number {
                value,
                negative,
                overflow,
                non_integer,
            }) => {
                for byte in fragment {
                    match byte {
                        b'-' => *negative = true,
                        b'0'..=b'9' if !*non_integer => {
                            match value
                                .checked_mul(10)
                                .and_then(|v| v.checked_add(u64::from(byte - b'0')))
                            {
                                Some(v) => *value = v,
                                None => *overflow = true,
                            }
                        }
                        _ => *non_integer = true,
                    }
                }
            }
            None if fragment.is_empty() => {}
            None => return Err(malformed()),
        }
        Ok(())
    }

    pub(super) fn event(&mut self, event: Event) -> Result<(), MachineError> {
        if self.step == 42 {
            return self.envelope.event(event);
        }
        if let Some(scalar) = &self.scalar {
            let kind = match scalar {
                Scalar::Bytes { kind, .. } => *kind,
                Scalar::Number { .. } => ScalarKind::Number,
            };
            return match event {
                Event::ScalarFragment(actual) if actual == kind => Ok(()),
                Event::ScalarEnd(actual) if actual == kind => self.finish_scalar(),
                _ => Err(malformed()),
            };
        }
        // The nullable window may be omitted; the observation still replaces prior usage.
        if self.step == 38 && event == Event::ContainerEnd(ContainerKind::Object) {
            self.step = 41;
            return Ok(());
        }
        match (instruction(self.step), event) {
            (Instruction::Name(_), Event::ScalarStart(ScalarKind::Name)) => {
                self.scalar = Some(Scalar::Bytes {
                    kind: ScalarKind::Name,
                    bytes: [0; crate::PROTOCOL_IDENTITY_MAX_BYTES],
                    len: 0,
                });
            }
            (Instruction::Identity, Event::ScalarStart(ScalarKind::String)) => {
                self.scalar = Some(Scalar::Bytes {
                    kind: ScalarKind::String,
                    bytes: [0; crate::PROTOCOL_IDENTITY_MAX_BYTES],
                    len: 0,
                });
            }
            (Instruction::Number | Instruction::Window, Event::ScalarStart(ScalarKind::Number)) => {
                self.scalar = Some(Scalar::Number {
                    value: 0,
                    negative: false,
                    overflow: false,
                    non_integer: false,
                });
            }
            (Instruction::Window, Event::Null) => {
                self.window = None;
                self.step += 1;
            }
            (Instruction::Object, Event::ContainerStart(ContainerKind::Object))
            | (Instruction::End, Event::ContainerEnd(ContainerKind::Object)) => self.step += 1,
            _ => return Err(malformed()),
        }
        Ok(())
    }

    fn finish_scalar(&mut self) -> Result<(), MachineError> {
        match self.scalar.take().ok_or_else(malformed)? {
            Scalar::Bytes {
                kind: ScalarKind::Name,
                bytes,
                len,
            } => {
                let Instruction::Name(expected) = instruction(self.step) else {
                    return Err(malformed());
                };
                let name = &bytes[..len];
                // The producer's only defaulted breakdown field can be absent.
                if matches!(self.step, 16 | 31) && name == b"outputTokens" {
                    self.step += 2;
                } else if name != expected {
                    return Err(malformed());
                }
            }
            Scalar::Bytes {
                kind: ScalarKind::String,
                bytes,
                len,
            } => {
                let text = std::str::from_utf8(&bytes[..len]).map_err(|_| malformed())?;
                match self.step {
                    3 => self.thread_id = Some(CasThreadId::new(text).map_err(|_| malformed())?),
                    5 => self.turn_id = Some(CasTurnId::new(text).map_err(|_| malformed())?),
                    _ => return Err(malformed()),
                }
            }
            Scalar::Number {
                value,
                negative,
                overflow,
                non_integer,
            } => {
                if non_integer {
                    return Err(malformed());
                }
                if self.step == 39 {
                    self.window = (!negative && !overflow && value > 0)
                        .then(|| i64::try_from(value).ok())
                        .flatten();
                } else {
                    let block = (self.step - 8) / 15;
                    let counter = ((self.step - 8) % 15 - 3) / 2;
                    match i64::try_from(value)
                        .ok()
                        .filter(|_| (!negative || value == 0) && !overflow)
                    {
                        Some(value) => self.counters[block][counter] = value,
                        None => self.available = false,
                    }
                }
            }
            _ => return Err(malformed()),
        }
        self.step += 1;
        Ok(())
    }

    pub(super) fn map_parse_failure(&self, failure: ParseFailure) -> DecodeReaderError {
        json_failure(failure)
    }

    pub(super) fn finish(&mut self) -> Result<DecodedIncoming, MachineError> {
        if self.step != 42 || !self.envelope.complete() || self.scalar.is_some() {
            return Err(malformed());
        }
        let usage = self.available.then(|| ContextTokenUsage {
            total: breakdown(self.counters[0]),
            last: breakdown(self.counters[1]),
            model_context_window: self.window,
        });
        let operation = OrderedTurnStreamOperation::ThreadContextObservation(
            ThreadContextObservation::decoded(
                self.thread_id.take().ok_or_else(malformed)?,
                self.turn_id.take().ok_or_else(malformed)?,
                usage,
            ),
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

fn instruction(step: usize) -> Instruction {
    match step {
        0 => Instruction::Name(b"params"),
        1 | 7 => Instruction::Object,
        2 => Instruction::Name(b"threadId"),
        3 | 5 => Instruction::Identity,
        4 => Instruction::Name(b"turnId"),
        6 => Instruction::Name(b"tokenUsage"),
        8..=37 => match (step - 8) % 15 {
            0 => Instruction::Name(if step < 23 { b"total" } else { b"last" }),
            1 => Instruction::Object,
            2 => Instruction::Name(b"totalTokens"),
            4 => Instruction::Name(b"inputTokens"),
            6 => Instruction::Name(b"cachedInputTokens"),
            8 => Instruction::Name(b"cacheWriteInputTokens"),
            10 => Instruction::Name(b"outputTokens"),
            12 => Instruction::Name(b"reasoningOutputTokens"),
            14 => Instruction::End,
            _ => Instruction::Number,
        },
        38 => Instruction::Name(b"modelContextWindow"),
        39 => Instruction::Window,
        40..=42 => Instruction::End,
        _ => Instruction::Done,
    }
}

fn breakdown(values: [i64; 6]) -> ContextTokenBreakdown {
    ContextTokenBreakdown {
        total_tokens: values[0],
        input_tokens: values[1],
        cached_input_tokens: values[2],
        cache_write_input_tokens: values[3],
        output_tokens: values[4],
        reasoning_output_tokens: values[5],
    }
}

fn malformed() -> MachineError {
    ForegroundIngressError::MalformedContextObservation.into()
}

use super::{OutageBufferError, OutageFact, OutageTarget};
use beryl_backend::{
    ProviderObservationBegin, ProviderObservationControl, ProviderScalar,
    ProviderStructuredPosition, ProviderValueContext,
};

struct Encoder<'a> {
    output: Option<&'a mut Vec<u8>>,
    length: usize,
    field_limit: usize,
}

impl Encoder<'_> {
    fn raw(&mut self, value: &[u8]) -> Result<(), OutageBufferError> {
        self.length = self
            .length
            .checked_add(value.len())
            .ok_or(OutageBufferError::EncodedByteLimit)?;
        if let Some(output) = &mut self.output {
            output.extend_from_slice(value);
        }
        Ok(())
    }

    fn byte(&mut self, value: u8) -> Result<(), OutageBufferError> {
        self.raw(&[value])
    }
    fn number(&mut self, value: u64) -> Result<(), OutageBufferError> {
        self.raw(&value.to_le_bytes())
    }

    fn text(&mut self, value: &str) -> Result<(), OutageBufferError> {
        if value.len() > self.field_limit {
            return Err(OutageBufferError::FieldLimit);
        }
        self.number(u64::try_from(value.len()).map_err(|_| OutageBufferError::EncodedByteLimit)?)?;
        self.raw(value.as_bytes())
    }

    fn context(&mut self, context: ProviderValueContext) -> Result<(), OutageBufferError> {
        match context {
            ProviderValueContext::Field(field) => {
                self.byte(0)?;
                self.byte(field as u8)
            }
            ProviderValueContext::Structured {
                root,
                depth,
                position,
            } => {
                self.byte(1)?;
                self.byte(root as u8)?;
                self.byte(depth)?;
                let (tag, index) = match position {
                    ProviderStructuredPosition::ListElement { index } => (0, index),
                    ProviderStructuredPosition::ObjectKey { entry } => (1, entry),
                    ProviderStructuredPosition::ObjectValue { entry } => (2, entry),
                };
                self.byte(tag)?;
                self.number(index)
            }
        }
    }

    fn control(&mut self, control: ProviderObservationControl) -> Result<(), OutageBufferError> {
        use ProviderObservationControl::*;
        match control {
            BeginField(context) => {
                self.byte(0)?;
                self.context(context)
            }
            EndField(context) => {
                self.byte(1)?;
                self.context(context)
            }
            BeginContainer { context, container } => {
                self.byte(2)?;
                self.context(context)?;
                self.byte(container as u8)
            }
            EndContainer { context, container } => {
                self.byte(3)?;
                self.context(context)?;
                self.byte(container as u8)
            }
            BeginElement { context, index } => {
                self.byte(4)?;
                self.context(context)?;
                self.number(index)
            }
            EndElement { context, index } => {
                self.byte(5)?;
                self.context(context)?;
                self.number(index)
            }
            BeginObjectEntry { root, depth, entry } => {
                self.byte(6)?;
                self.byte(root as u8)?;
                self.byte(depth)?;
                self.number(entry)
            }
            EndObjectEntry { root, depth, entry } => {
                self.byte(7)?;
                self.byte(root as u8)?;
                self.byte(depth)?;
                self.number(entry)
            }
            Enum { context, value } => {
                self.byte(8)?;
                self.context(context)?;
                self.byte(value as u8)
            }
            Scalar { context, value } => {
                self.byte(9)?;
                self.context(context)?;
                match value {
                    ProviderScalar::Null => self.byte(0),
                    ProviderScalar::Boolean(value) => {
                        self.byte(1)?;
                        self.byte(u8::from(value))
                    }
                    ProviderScalar::Signed(value) => {
                        self.byte(2)?;
                        self.raw(&value.to_le_bytes())
                    }
                    ProviderScalar::Unsigned(value) => {
                        self.byte(3)?;
                        self.number(value)
                    }
                    ProviderScalar::FiniteFloat(value) => {
                        self.byte(4)?;
                        self.number(value.bits())
                    }
                }
            }
        }
    }

    fn target(&mut self, target: &OutageTarget) -> Result<(), OutageBufferError> {
        let identity = &target.identity;
        self.raw(identity.runtime_id().as_bytes())?;
        self.number(identity.process_generation().get())?;
        self.number(identity.connection_generation())?;
        self.number(identity.registration_serial())?;
        self.raw(identity.thread_id().as_bytes())?;
        self.text(identity.cas_thread_id().as_str())?;
        self.number(identity.loaded_generation().process().get())?;
        self.number(identity.loaded_generation().thread().get())?;
        self.number(identity.home_generation())?;
        self.raw(target.turn.as_bytes())?;
        self.text(target.cas_turn.as_str())?;
        self.byte(0)
    }

    fn fact(&mut self, target: usize, fact: OutageFact<'_>) -> Result<(), OutageBufferError> {
        self.number(u64::try_from(target).map_err(|_| OutageBufferError::EncodedByteLimit)?)?;
        self.byte(fact.priority() as u8)?;
        match fact {
            OutageFact::Identity { observation, item } => {
                self.byte(0)?;
                self.raw(observation.as_bytes())?;
                self.text(item.as_str())
            }
            OutageFact::Lifecycle { observation, begin } => {
                self.byte(1)?;
                self.raw(observation.as_bytes())?;
                match begin {
                    ProviderObservationBegin::Item { lifecycle, kind } => {
                        self.byte(0)?;
                        self.byte(lifecycle as u8)?;
                        self.byte(kind as u8)
                    }
                    ProviderObservationBegin::Delta { kind } => {
                        self.byte(1)?;
                        self.byte(kind as u8)
                    }
                }
            }
            OutageFact::Control {
                observation,
                ordinal,
                control,
                ..
            } => {
                self.byte(2)?;
                self.raw(observation.as_bytes())?;
                self.number(ordinal)?;
                self.control(control)
            }
            OutageFact::Terminal(status) => {
                self.byte(3)?;
                self.byte(status as u8)
            }
            OutageFact::CompleteField {
                observation,
                ordinal,
                context,
                kind,
                text,
            } => {
                self.byte(4)?;
                self.raw(observation.as_bytes())?;
                self.number(ordinal)?;
                self.context(context)?;
                self.byte(kind as u8)?;
                self.text(text)
            }
            OutageFact::UserCorrelation { item, client } => {
                self.byte(5)?;
                self.text(item.as_str())?;
                self.text(client.as_str())
            }
            OutageFact::MediaHandoff { item, saved_path } => {
                self.byte(6)?;
                self.text(item.as_str())?;
                self.text(saved_path)
            }
        }
    }
}

pub(super) fn target_len(
    target: &OutageTarget,
    field_limit: usize,
) -> Result<usize, OutageBufferError> {
    let mut encoder = Encoder {
        output: None,
        length: 0,
        field_limit,
    };
    encoder.target(target)?;
    Ok(encoder.length)
}

pub(super) fn fact_len(
    target: usize,
    fact: OutageFact<'_>,
    field_limit: usize,
) -> Result<usize, OutageBufferError> {
    let mut encoder = Encoder {
        output: None,
        length: 0,
        field_limit,
    };
    encoder.fact(target, fact)?;
    Ok(encoder.length)
}

pub(super) fn encode_fact(target: usize, fact: OutageFact<'_>, length: usize) -> Vec<u8> {
    let mut output = Vec::with_capacity(length);
    let mut encoder = Encoder {
        output: Some(&mut output),
        length: 0,
        field_limit: usize::MAX,
    };
    encoder
        .fact(target, fact)
        .expect("the identical borrowed fact was counted before allocation");
    debug_assert_eq!(output.len(), length);
    output
}

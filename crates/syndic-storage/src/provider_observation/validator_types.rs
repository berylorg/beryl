use super::{
    ProviderContainer, ProviderEnumValue, ProviderField, ProviderObservationValidatorError,
    ProviderValueContext,
    schema::{ListKind, ObjectSchema},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderObservationFrame {
    List {
        context: ProviderValueContext,
        kind: ListKind,
        next: u64,
    },
    Object {
        context: ProviderValueContext,
        schema: ObjectSchema,
        seen: [u64; 2],
        variant: Option<ProviderEnumValue>,
    },
    Structured {
        context: ProviderValueContext,
        container: ProviderContainer,
        next: u64,
        depth: u8,
    },
    AgentStates {
        context: ProviderValueContext,
        next: u64,
    },
    Element {
        context: ProviderValueContext,
        index: u64,
        kind: ProviderObservationElementKind,
        started: bool,
        complete: bool,
    },
    StructuredEntry {
        root: ProviderField,
        depth: u8,
        entry: u64,
        key_started: bool,
        key_complete: bool,
        value_started: bool,
        value_complete: bool,
    },
    AgentStateEntry {
        entry: u64,
        key_started: bool,
        key_complete: bool,
        seen: [u64; 2],
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderObservationElementKind {
    Typed(ListKind),
    Structured { root: ProviderField, depth: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProviderIdentityValidatorState {
    pub(crate) bytes: u64,
    pub(crate) saw_scalar: bool,
    pub(crate) first_whitespace: bool,
    pub(crate) last_whitespace: bool,
}

impl ProviderIdentityValidatorState {
    pub(crate) const MAX_BYTES: u64 = 256;

    pub(crate) const fn new() -> Self {
        Self {
            bytes: 0,
            saw_scalar: false,
            first_whitespace: false,
            last_whitespace: false,
        }
    }

    pub(crate) fn push(
        &mut self,
        scalar: Option<char>,
    ) -> Result<(), ProviderObservationValidatorError> {
        self.bytes = self
            .bytes
            .checked_add(1)
            .filter(|bytes| *bytes <= Self::MAX_BYTES)
            .ok_or(ProviderObservationValidatorError::InvalidIdentity)?;
        if let Some(scalar) = scalar {
            if scalar.is_control() {
                return Err(ProviderObservationValidatorError::InvalidIdentity);
            }
            let whitespace = scalar.is_whitespace();
            if !self.saw_scalar {
                self.first_whitespace = whitespace;
                self.saw_scalar = true;
            }
            self.last_whitespace = whitespace;
        }
        Ok(())
    }

    pub(crate) const fn finish(self) -> Result<(), ProviderObservationValidatorError> {
        if self.bytes == 0 || self.first_whitespace || self.last_whitespace {
            Err(ProviderObservationValidatorError::InvalidIdentity)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Utf8ValidatorState {
    pub(crate) remaining: u8,
    pub(crate) codepoint: u32,
    pub(crate) minimum: u32,
}

impl Utf8ValidatorState {
    pub(crate) const fn new() -> Self {
        Self {
            remaining: 0,
            codepoint: 0,
            minimum: 0,
        }
    }

    pub(crate) fn push(
        &mut self,
        byte: u8,
    ) -> Result<Option<char>, ProviderObservationValidatorError> {
        if self.remaining == 0 {
            match byte {
                0x00..=0x7f => Ok(Some(char::from(byte))),
                0xc2..=0xdf => {
                    self.remaining = 1;
                    self.codepoint = u32::from(byte & 0x1f);
                    self.minimum = 0x80;
                    Ok(None)
                }
                0xe0..=0xef => {
                    self.remaining = 2;
                    self.codepoint = u32::from(byte & 0x0f);
                    self.minimum = 0x800;
                    Ok(None)
                }
                0xf0..=0xf4 => {
                    self.remaining = 3;
                    self.codepoint = u32::from(byte & 0x07);
                    self.minimum = 0x1_0000;
                    Ok(None)
                }
                _ => Err(ProviderObservationValidatorError::InvalidUtf8),
            }
        } else {
            if byte & 0xc0 != 0x80 {
                return Err(ProviderObservationValidatorError::InvalidUtf8);
            }
            self.codepoint = (self.codepoint << 6) | u32::from(byte & 0x3f);
            self.remaining -= 1;
            if self.remaining == 0
                && (self.codepoint < self.minimum
                    || self.codepoint > 0x10_ffff
                    || (0xd800..=0xdfff).contains(&self.codepoint))
            {
                return Err(ProviderObservationValidatorError::InvalidUtf8);
            }
            if self.remaining == 0 {
                Ok(char::from_u32(self.codepoint))
            } else {
                Ok(None)
            }
        }
    }
}

use sha2::{Digest, Sha256};

use super::{
    ProviderObservationBegin, ProviderObservationChunkPayload, ProviderObservationControl,
    ProviderObservationDigest, ProviderScalar, ProviderStructuredPosition, ProviderValueContext,
};

const DOMAIN: &[u8] = b"beryl.syndic.provider-observation.v1\0";
const CONTROL_ESCAPE: u8 = 0xff;

#[derive(Clone)]
pub(crate) struct CanonicalObservationState {
    hasher: Sha256,
    bytes: u64,
}

impl CanonicalObservationState {
    pub(crate) fn initial(begin: ProviderObservationBegin) -> Self {
        let mut value = Self {
            hasher: Sha256::new(),
            bytes: 0,
        };
        value.raw(DOMAIN).expect("static domain length fits u64");
        let mut encoded = Vec::with_capacity(4);
        encoded.extend_from_slice(&[CONTROL_ESCAPE, 1]);
        match begin {
            ProviderObservationBegin::Item { lifecycle, kind } => {
                encoded.extend_from_slice(&[0, lifecycle as u8, kind as u8]);
            }
            ProviderObservationBegin::Delta { kind } => {
                encoded.extend_from_slice(&[1, kind as u8]);
            }
        }
        value.raw(&encoded).expect("begin token length fits u64");
        value
    }

    pub(crate) fn control(
        &mut self,
        control: ProviderObservationControl,
    ) -> Result<(), CanonicalObservationError> {
        let mut encoded = Vec::with_capacity(32);
        encoded.push(CONTROL_ESCAPE);
        match control {
            ProviderObservationControl::BeginField(context) => {
                encoded.push(2);
                encode_context(&mut encoded, context);
            }
            ProviderObservationControl::EndField(context) => {
                encoded.push(3);
                encode_context(&mut encoded, context);
            }
            ProviderObservationControl::BeginContainer { context, container } => {
                encoded.push(4);
                encode_context(&mut encoded, context);
                encoded.push(container as u8);
            }
            ProviderObservationControl::EndContainer { context, container } => {
                encoded.push(5);
                encode_context(&mut encoded, context);
                encoded.push(container as u8);
            }
            ProviderObservationControl::BeginElement { context, index } => {
                encoded.push(6);
                encode_context(&mut encoded, context);
                encoded.extend_from_slice(&index.to_be_bytes());
            }
            ProviderObservationControl::EndElement { context, index } => {
                encoded.push(7);
                encode_context(&mut encoded, context);
                encoded.extend_from_slice(&index.to_be_bytes());
            }
            ProviderObservationControl::BeginObjectEntry { root, depth, entry } => {
                encoded.extend_from_slice(&[8, root.tag(), depth]);
                encoded.extend_from_slice(&entry.to_be_bytes());
            }
            ProviderObservationControl::EndObjectEntry { root, depth, entry } => {
                encoded.extend_from_slice(&[9, root.tag(), depth]);
                encoded.extend_from_slice(&entry.to_be_bytes());
            }
            ProviderObservationControl::Enum { context, value } => {
                encoded.push(10);
                encode_context(&mut encoded, context);
                encoded.push(value.tag());
            }
            ProviderObservationControl::Scalar { context, value } => {
                encoded.push(11);
                encode_context(&mut encoded, context);
                encode_scalar(&mut encoded, value);
            }
        }
        self.raw(&encoded)
    }

    pub(crate) fn fragment(&mut self, bytes: &[u8]) -> Result<(), CanonicalObservationError> {
        for segment in bytes.split_inclusive(|byte| *byte == CONTROL_ESCAPE) {
            if segment.last() == Some(&CONTROL_ESCAPE) {
                let prefix = &segment[..segment.len() - 1];
                self.raw(prefix)?;
                self.raw(&[CONTROL_ESCAPE, 0])?;
            } else {
                self.raw(segment)?;
            }
        }
        Ok(())
    }

    pub(crate) fn apply_chunk(
        &mut self,
        payload: &ProviderObservationChunkPayload,
    ) -> Result<(), CanonicalObservationError> {
        match payload {
            ProviderObservationChunkPayload::Control(control) => self.control(*control),
            ProviderObservationChunkPayload::Fragment { bytes, .. } => self.fragment(bytes),
        }
    }

    pub(crate) fn canonical_bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) fn digest(&self) -> ProviderObservationDigest {
        ProviderObservationDigest::from_bytes(self.hasher.clone().finalize().into())
    }

    fn raw(&mut self, bytes: &[u8]) -> Result<(), CanonicalObservationError> {
        let count = u64::try_from(bytes.len()).map_err(|_| CanonicalObservationError::Overflow)?;
        self.bytes = self
            .bytes
            .checked_add(count)
            .ok_or(CanonicalObservationError::Overflow)?;
        self.hasher.update(bytes);
        Ok(())
    }
}

fn encode_context(encoded: &mut Vec<u8>, context: ProviderValueContext) {
    match context {
        ProviderValueContext::Field(field) => encoded.extend_from_slice(&[0, field.tag()]),
        ProviderValueContext::Structured {
            root,
            depth,
            position,
        } => {
            encoded.extend_from_slice(&[1, root.tag(), depth]);
            match position {
                ProviderStructuredPosition::ListElement { index } => {
                    encoded.push(0);
                    encoded.extend_from_slice(&index.to_be_bytes());
                }
                ProviderStructuredPosition::ObjectKey { entry } => {
                    encoded.push(1);
                    encoded.extend_from_slice(&entry.to_be_bytes());
                }
                ProviderStructuredPosition::ObjectValue { entry } => {
                    encoded.push(2);
                    encoded.extend_from_slice(&entry.to_be_bytes());
                }
            }
        }
    }
}

fn encode_scalar(encoded: &mut Vec<u8>, scalar: ProviderScalar) {
    match scalar {
        ProviderScalar::Null => encoded.push(0),
        ProviderScalar::Boolean(value) => encoded.extend_from_slice(&[1, u8::from(value)]),
        ProviderScalar::Signed(value) => {
            encoded.push(2);
            encoded.extend_from_slice(&value.to_be_bytes());
        }
        ProviderScalar::Unsigned(value) => {
            encoded.push(3);
            encoded.extend_from_slice(&value.to_be_bytes());
        }
        ProviderScalar::FiniteFloat(value) => {
            encoded.push(4);
            encoded.extend_from_slice(&value.bits().to_be_bytes());
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CanonicalObservationError {
    Overflow,
}

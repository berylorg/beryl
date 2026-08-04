use crate::ProviderFrameSinkV1;

use super::{Encoder, ObservationEncodeError};
use crate::provider_observation::compiler::replay::{StructuredNode, StructuredPath, TextSelector};

impl<S: ProviderFrameSinkV1> Encoder<'_, '_, S> {
    pub(super) fn structured(
        &mut self,
        path: StructuredPath,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        match self.reader.structured_node(path)? {
            StructuredNode::Null => self.u8(0),
            StructuredNode::Boolean(false) => self.u8(1),
            StructuredNode::Boolean(true) => self.u8(2),
            StructuredNode::Signed(value) => {
                self.u8(3)?;
                self.i64(value)
            }
            StructuredNode::Unsigned(value) => {
                self.u8(4)?;
                self.u64(value)
            }
            StructuredNode::FiniteFloat(bits) => {
                self.u8(5)?;
                self.u64(bits)
            }
            StructuredNode::String(_) => {
                self.u8(6)?;
                self.text(TextSelector::Structured(path), None)
            }
            StructuredNode::List(count) => {
                self.u8(7)?;
                self.u64(count)?;
                for index in 0..count {
                    let mut child = path;
                    child.push_list(index).map_err(|error| {
                        ObservationEncodeError::Replay(super::ReplayError::Semantic(error))
                    })?;
                    self.structured(child)?;
                }
                Ok(())
            }
            StructuredNode::Object(count) => {
                self.u8(8)?;
                self.u64(count)?;
                for entry in 0..count {
                    let mut key = path;
                    key.push_object_key(entry).map_err(|error| {
                        ObservationEncodeError::Replay(super::ReplayError::Semantic(error))
                    })?;
                    self.raw_text(TextSelector::Structured(key))?;
                    let mut value = path;
                    value.push_object_value(entry).map_err(|error| {
                        ObservationEncodeError::Replay(super::ReplayError::Semantic(error))
                    })?;
                    self.structured(value)?;
                }
                Ok(())
            }
        }
    }
}

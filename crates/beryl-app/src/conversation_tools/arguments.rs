use beryl_backend::{
    DynamicToolArgumentContainer, DynamicToolArgumentControl, DynamicToolArgumentScalarKind,
};

use super::{DynamicToolRejection, DynamicToolSchemaRejection};

pub(crate) trait StringValueSink {
    type Output;

    fn start(&mut self) -> Result<(), DynamicToolRejection>;

    fn fragment(&mut self, bytes: &[u8]) -> Result<(), DynamicToolRejection>;

    fn finish(&mut self) -> Result<Self::Output, DynamicToolRejection>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProductState {
    Root,
    NameOrEnd,
    Name,
    Value,
    ValueString,
    AfterValue,
    Done,
}

pub(crate) struct SingleStringObjectBuilder<V: StringValueSink> {
    expected_field: &'static [u8],
    state: ProductState,
    field_seen: bool,
    name_matches: bool,
    name_offset: u64,
    value_offset: u64,
    value: V,
    output: Option<V::Output>,
    rejection: Option<DynamicToolRejection>,
}

impl<V: StringValueSink> SingleStringObjectBuilder<V> {
    pub(crate) const fn new(expected_field: &'static str, value: V) -> Self {
        Self {
            expected_field: expected_field.as_bytes(),
            state: ProductState::Root,
            field_seen: false,
            name_matches: true,
            name_offset: 0,
            value_offset: 0,
            value,
            output: None,
            rejection: None,
        }
    }

    pub(crate) fn control(&mut self, control: DynamicToolArgumentControl) {
        if self.rejection.is_some() {
            return;
        }
        let result = match (self.state, control) {
            (
                ProductState::Root,
                DynamicToolArgumentControl::ContainerStart(DynamicToolArgumentContainer::Object),
            ) => {
                self.state = ProductState::NameOrEnd;
                Ok(())
            }
            (ProductState::Root, _) => Err(DynamicToolSchemaRejection::RootMustBeObject.into()),
            (
                ProductState::NameOrEnd | ProductState::AfterValue,
                DynamicToolArgumentControl::ScalarStart(DynamicToolArgumentScalarKind::ObjectName),
            ) => {
                self.state = ProductState::Name;
                self.name_matches = true;
                self.name_offset = 0;
                Ok(())
            }
            (
                ProductState::NameOrEnd,
                DynamicToolArgumentControl::ContainerEnd(DynamicToolArgumentContainer::Object),
            ) => Err(DynamicToolSchemaRejection::MissingRequiredField.into()),
            (
                ProductState::AfterValue,
                DynamicToolArgumentControl::ContainerEnd(DynamicToolArgumentContainer::Object),
            ) => {
                self.state = ProductState::Done;
                Ok(())
            }
            (
                ProductState::Name,
                DynamicToolArgumentControl::ScalarEnd(DynamicToolArgumentScalarKind::ObjectName),
            ) => self.finish_name(),
            (
                ProductState::Value,
                DynamicToolArgumentControl::ScalarStart(DynamicToolArgumentScalarKind::String),
            ) => match self.value.start() {
                Ok(()) => {
                    self.value_offset = 0;
                    self.state = ProductState::ValueString;
                    Ok(())
                }
                Err(rejection) => Err(rejection),
            },
            (
                ProductState::ValueString,
                DynamicToolArgumentControl::ScalarEnd(DynamicToolArgumentScalarKind::String),
            ) => match self.value.finish() {
                Ok(output) => {
                    self.output = Some(output);
                    self.field_seen = true;
                    self.state = ProductState::AfterValue;
                    Ok(())
                }
                Err(rejection) => Err(rejection),
            },
            (ProductState::Done, _) => {
                Err(DynamicToolSchemaRejection::InvalidControlSequence.into())
            }
            (ProductState::Value, _) => Err(DynamicToolSchemaRejection::WrongValueShape.into()),
            _ => Err(DynamicToolSchemaRejection::InvalidControlSequence.into()),
        };
        if let Err(rejection) = result {
            self.rejection = Some(rejection);
        }
    }

    pub(crate) fn fragment(
        &mut self,
        kind: DynamicToolArgumentScalarKind,
        offset: u64,
        bytes: &[u8],
    ) {
        if self.rejection.is_some() {
            return;
        }
        let result = match self.state {
            ProductState::Name if kind == DynamicToolArgumentScalarKind::ObjectName => {
                self.match_name(offset, bytes)
            }
            ProductState::ValueString if kind == DynamicToolArgumentScalarKind::String => {
                self.push_value(offset, bytes)
            }
            _ => Err(DynamicToolSchemaRejection::InvalidScalarFragment.into()),
        };
        if let Err(rejection) = result {
            self.rejection = Some(rejection);
        }
    }

    pub(crate) fn seal(mut self) -> Result<V::Output, DynamicToolRejection> {
        if let Some(rejection) = self.rejection {
            return Err(rejection);
        }
        if self.state != ProductState::Done || !self.field_seen {
            return Err(DynamicToolSchemaRejection::MissingRequiredField.into());
        }
        self.output
            .take()
            .ok_or_else(|| DynamicToolSchemaRejection::MissingRequiredField.into())
    }

    fn finish_name(&mut self) -> Result<(), DynamicToolRejection> {
        let expected_len = u64::try_from(self.expected_field.len())
            .map_err(|_| DynamicToolSchemaRejection::InvalidScalarFragment)?;
        if !self.name_matches || self.name_offset != expected_len {
            return Err(DynamicToolSchemaRejection::UnknownField.into());
        }
        if self.field_seen {
            return Err(DynamicToolSchemaRejection::DuplicateField.into());
        }
        self.state = ProductState::Value;
        Ok(())
    }

    fn match_name(&mut self, offset: u64, bytes: &[u8]) -> Result<(), DynamicToolRejection> {
        if offset != self.name_offset {
            return Err(DynamicToolSchemaRejection::InvalidScalarFragment.into());
        }
        let start = usize::try_from(offset)
            .map_err(|_| DynamicToolSchemaRejection::InvalidScalarFragment)?;
        let end = start
            .checked_add(bytes.len())
            .ok_or(DynamicToolSchemaRejection::InvalidScalarFragment)?;
        self.name_matches &= self
            .expected_field
            .get(start..end)
            .is_some_and(|expected| expected == bytes);
        self.name_offset = offset
            .checked_add(
                u64::try_from(bytes.len())
                    .map_err(|_| DynamicToolSchemaRejection::InvalidScalarFragment)?,
            )
            .ok_or(DynamicToolSchemaRejection::InvalidScalarFragment)?;
        Ok(())
    }

    fn push_value(&mut self, offset: u64, bytes: &[u8]) -> Result<(), DynamicToolRejection> {
        if offset != self.value_offset {
            return Err(DynamicToolSchemaRejection::InvalidScalarFragment.into());
        }
        self.value.fragment(bytes)?;
        self.value_offset = offset
            .checked_add(
                u64::try_from(bytes.len())
                    .map_err(|_| DynamicToolSchemaRejection::InvalidScalarFragment)?,
            )
            .ok_or(DynamicToolSchemaRejection::InvalidScalarFragment)?;
        Ok(())
    }
}

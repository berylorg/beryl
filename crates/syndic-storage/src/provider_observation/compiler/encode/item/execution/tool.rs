use crate::provider_observation::{ProviderEnumValue as E, ProviderField as F};
use crate::{ProviderFrameSinkV1, ProviderLogicalTextRoleV1};

use super::super::super::{Encoder, ObservationEncodeError};
use crate::provider_observation::compiler::replay::{FieldSelector, Presence, StructuredPath};

impl<S: ProviderFrameSinkV1> Encoder<'_, '_, S> {
    pub(in crate::provider_observation::compiler) fn mcp_tool_call(
        &mut self,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        self.text(FieldSelector::top(F::McpServer).into(), None)?;
        self.text(FieldSelector::top(F::McpTool).into(), None)?;
        let status = self.required_enum(FieldSelector::top(F::McpStatus))?;
        self.status3(status)?;
        self.structured(StructuredPath::new(F::McpArguments))?;

        let context = FieldSelector::top(F::McpAppContext);
        let present = self.field_presence(context)? == Presence::Value;
        self.option(present, |encoder| encoder.mcp_app_context())?;
        self.optional_text(FieldSelector::top(F::McpResourceUri), None)?;
        self.optional_text(FieldSelector::top(F::McpPluginId), None)?;

        let result = FieldSelector::top(F::McpResult);
        let present = self.field_presence(result)? == Presence::Value;
        self.option(present, |encoder| encoder.mcp_result())?;
        let error = FieldSelector::top(F::McpError);
        let present = self.field_presence(error)? == Presence::Value;
        self.option(present, |encoder| {
            encoder.text(
                FieldSelector::in_object(F::McpErrorMessage, F::McpError).into(),
                Some(ProviderLogicalTextRoleV1::Operational),
            )
        })?;
        let duration = self.optional_signed(FieldSelector::top(F::DurationMs))?;
        self.option(duration.is_some(), |encoder| encoder.i64(duration.unwrap()))
    }

    fn mcp_app_context(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let field = |value| FieldSelector::in_object(value, F::McpAppContext);
        self.text(field(F::McpConnectorId).into(), None)?;
        self.optional_text(field(F::McpLinkId), None)?;
        self.optional_text(field(F::McpResourceUri), None)?;
        self.optional_text(field(F::McpAppName), None)?;
        self.optional_text(field(F::McpTemplateId), None)?;
        self.optional_text(field(F::McpActionName), None)
    }

    fn mcp_result(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let contents = FieldSelector::in_object(F::McpResultContents, F::McpResult);
        let count = self.list_count(contents, true)?;
        self.u64(count)?;
        for index in 0..count {
            self.u8(0)?;
            self.structured(StructuredPath::in_list(
                F::McpResultContent,
                F::McpResultContents,
                index,
            ))?;
        }
        let structured = FieldSelector::in_object(F::McpStructuredContent, F::McpResult);
        let present = self.field_presence(structured)? == Presence::Value;
        self.option(present, |encoder| {
            encoder.structured(StructuredPath::new(F::McpStructuredContent))
        })?;
        let meta = FieldSelector::in_object(F::McpMeta, F::McpResult);
        let present = self.field_presence(meta)? == Presence::Value;
        self.option(present, |encoder| {
            encoder.structured(StructuredPath::new(F::McpMeta))
        })
    }

    pub(in crate::provider_observation::compiler) fn dynamic_tool_call(
        &mut self,
    ) -> Result<(), ObservationEncodeError<S::Error>> {
        self.optional_text(FieldSelector::top(F::DynamicNamespace), None)?;
        self.text(FieldSelector::top(F::DynamicTool).into(), None)?;
        self.structured(StructuredPath::new(F::DynamicArguments))?;
        let status = self.required_enum(FieldSelector::top(F::DynamicStatus))?;
        self.status3(status)?;

        let content = FieldSelector::top(F::DynamicContentItems);
        let present = self.field_presence(content)? == Presence::Value;
        self.option(present, |encoder| encoder.dynamic_content_items())?;
        let success = self.optional_boolean(FieldSelector::top(F::DynamicSuccess))?;
        self.option(success.is_some(), |encoder| {
            encoder.u8(u8::from(success.unwrap()))
        })?;
        let duration = self.optional_signed(FieldSelector::top(F::DurationMs))?;
        self.option(duration.is_some(), |encoder| encoder.i64(duration.unwrap()))
    }

    fn dynamic_content_items(&mut self) -> Result<(), ObservationEncodeError<S::Error>> {
        let count = self.list_count(FieldSelector::top(F::DynamicContentItems), true)?;
        self.u64(count)?;
        for index in 0..count {
            let field = |value| {
                FieldSelector::in_list(value, F::DynamicContentItems, index)
                    .with_object(F::DynamicContentItems)
            };
            match self.required_enum(field(F::DynamicContentItemKind))? {
                E::InputText => {
                    self.u8(0)?;
                    self.text(
                        field(F::DynamicOutputText).into(),
                        Some(ProviderLogicalTextRoleV1::Operational),
                    )?;
                }
                _ => return Err(super::super::value_mismatch(F::DynamicContentItemKind)),
            }
        }
        Ok(())
    }
}

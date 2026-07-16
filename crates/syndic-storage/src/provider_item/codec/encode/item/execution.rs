use super::super::Encoder;
use crate::provider_item::*;

impl<S: ProviderFrameSinkV1> Encoder<'_, S> {
    pub(super) fn command_execution(
        &mut self,
        value: &ProviderCommandExecutionV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.text(&value.command, Some(ProviderLogicalTextRoleV1::Operational))?;
        self.text(&value.cwd, None)?;
        self.option(&value.process_id, |encoder, value| {
            encoder.text(value, None)
        })?;
        self.enum_tag(
            value.source,
            &[
                ProviderCommandSourceV1::Agent,
                ProviderCommandSourceV1::UserShell,
                ProviderCommandSourceV1::UnifiedExecStartup,
                ProviderCommandSourceV1::UnifiedExecInteraction,
            ],
        )?;
        self.command_status(value.status)?;
        self.count(value.command_actions.len())?;
        for action in &value.command_actions {
            self.command_action(action)?;
        }
        self.option(&value.aggregated_output, |encoder, value| {
            encoder.text(value, Some(ProviderLogicalTextRoleV1::Operational))
        })?;
        self.option(&value.exit_code, |encoder, value| encoder.i32(*value))?;
        self.option(&value.duration_ms, |encoder, value| encoder.i64(*value))
    }

    fn command_status(
        &mut self,
        value: ProviderCommandStatusV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.enum_tag(
            value,
            &[
                ProviderCommandStatusV1::InProgress,
                ProviderCommandStatusV1::Completed,
                ProviderCommandStatusV1::Failed,
                ProviderCommandStatusV1::Declined,
            ],
        )
    }

    fn command_action(
        &mut self,
        value: &ProviderCommandActionV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        match value {
            ProviderCommandActionV1::Read {
                command,
                name,
                path,
            } => {
                self.u8(0)?;
                self.text(command, None)?;
                self.text(name, None)?;
                self.text(path, None)
            }
            ProviderCommandActionV1::ListFiles { command, path } => {
                self.u8(1)?;
                self.text(command, None)?;
                self.option(path, |encoder, value| encoder.text(value, None))
            }
            ProviderCommandActionV1::Search {
                command,
                query,
                path,
            } => {
                self.u8(2)?;
                self.text(command, None)?;
                self.option(query, |encoder, value| encoder.text(value, None))?;
                self.option(path, |encoder, value| encoder.text(value, None))
            }
            ProviderCommandActionV1::Unknown { command } => {
                self.u8(3)?;
                self.text(command, None)
            }
        }
    }

    pub(super) fn file_change(
        &mut self,
        value: &ProviderFileChangeV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.patch_status(value.status)?;
        self.file_changes(&value.changes)
    }

    fn patch_status(
        &mut self,
        value: ProviderPatchStatusV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.enum_tag(
            value,
            &[
                ProviderPatchStatusV1::InProgress,
                ProviderPatchStatusV1::Completed,
                ProviderPatchStatusV1::Failed,
                ProviderPatchStatusV1::Declined,
            ],
        )
    }

    pub(super) fn file_changes(
        &mut self,
        values: &[ProviderFileUpdateChangeV1],
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.count(values.len())?;
        for value in values {
            self.text(&value.path, None)?;
            self.text(&value.diff, Some(ProviderLogicalTextRoleV1::Operational))?;
            match &value.kind {
                ProviderPatchChangeKindV1::Add => self.u8(0)?,
                ProviderPatchChangeKindV1::Delete => self.u8(1)?,
                ProviderPatchChangeKindV1::Update { move_path } => {
                    self.u8(2)?;
                    self.option(move_path, |encoder, value| encoder.text(value, None))?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn mcp_tool_call(
        &mut self,
        value: &ProviderMcpToolCallV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.text(&value.server, None)?;
        self.text(&value.tool, None)?;
        self.tool_status(value.status)?;
        self.structured(&value.arguments)?;
        self.option(&value.app_context, |encoder, value| {
            encoder.text(&value.connector_id, None)?;
            encoder.option(&value.link_id, |encoder, value| encoder.text(value, None))?;
            encoder.option(&value.resource_uri, |encoder, value| {
                encoder.text(value, None)
            })?;
            encoder.option(&value.app_name, |encoder, value| encoder.text(value, None))?;
            encoder.option(&value.template_id, |encoder, value| {
                encoder.text(value, None)
            })?;
            encoder.option(&value.action_name, |encoder, value| {
                encoder.text(value, None)
            })
        })?;
        self.option(&value.mcp_app_resource_uri, |encoder, value| {
            encoder.text(value, None)
        })?;
        self.option(&value.plugin_id, |encoder, value| encoder.text(value, None))?;
        self.option(&value.result, |encoder, value| {
            encoder.count(value.content.len())?;
            for content in &value.content {
                encoder.mcp_content(content)?;
            }
            encoder.option(&value.structured_content, |encoder, value| {
                encoder.structured(value)
            })?;
            encoder.option(&value.meta, |encoder, value| encoder.structured(value))
        })?;
        self.option(&value.error, |encoder, value| {
            encoder.text(&value.message, Some(ProviderLogicalTextRoleV1::Operational))
        })?;
        self.option(&value.duration_ms, |encoder, value| encoder.i64(*value))
    }

    fn tool_status(
        &mut self,
        value: ProviderToolCallStatusV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.enum_tag(
            value,
            &[
                ProviderToolCallStatusV1::InProgress,
                ProviderToolCallStatusV1::Completed,
                ProviderToolCallStatusV1::Failed,
            ],
        )
    }

    pub(super) fn dynamic_tool_call(
        &mut self,
        value: &ProviderDynamicToolCallV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.option(&value.namespace, |encoder, value| encoder.text(value, None))?;
        self.text(&value.tool, None)?;
        self.structured(&value.arguments)?;
        self.tool_status(value.status)?;
        self.option(&value.content_items, |encoder, values| {
            encoder.count(values.len())?;
            for value in values {
                match value {
                    ProviderDynamicToolOutputV1::InputText { text } => {
                        encoder.u8(0)?;
                        encoder.text(text, Some(ProviderLogicalTextRoleV1::Operational))?;
                    }
                    ProviderDynamicToolOutputV1::InputImageLocator { locator } => {
                        encoder.u8(1)?;
                        encoder.raw_text(locator.as_str())?;
                    }
                    ProviderDynamicToolOutputV1::InputImageAsset { asset } => {
                        encoder.u8(2)?;
                        encoder.asset(asset.asset_id())?;
                    }
                }
            }
            Ok(())
        })?;
        self.option(&value.success, |encoder, value| {
            encoder.u8(u8::from(*value))
        })?;
        self.option(&value.duration_ms, |encoder, value| encoder.i64(*value))
    }
}

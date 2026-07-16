use super::super::Decoder;
use crate::provider_item::*;

impl Decoder<'_> {
    pub(super) fn command_execution(
        &mut self,
    ) -> Result<ProviderCommandExecutionV1, ProviderFrameDecodeError> {
        Ok(ProviderCommandExecutionV1 {
            command: self.text("command")?,
            cwd: self.text("command cwd")?,
            process_id: self.option("command process id", |decoder| {
                decoder.text("command process id")
            })?,
            source: self.enum_value(
                "command source",
                &[
                    ProviderCommandSourceV1::Agent,
                    ProviderCommandSourceV1::UserShell,
                    ProviderCommandSourceV1::UnifiedExecStartup,
                    ProviderCommandSourceV1::UnifiedExecInteraction,
                ],
            )?,
            status: self.command_status()?,
            command_actions: self.vector("command actions", |decoder| decoder.command_action())?,
            aggregated_output: self.option("aggregated output", |decoder| {
                decoder.text("aggregated output")
            })?,
            exit_code: self.option("exit code", Decoder::i32)?,
            duration_ms: self.option("command duration", Decoder::i64)?,
        })
    }

    fn command_status(&mut self) -> Result<ProviderCommandStatusV1, ProviderFrameDecodeError> {
        self.enum_value(
            "command status",
            &[
                ProviderCommandStatusV1::InProgress,
                ProviderCommandStatusV1::Completed,
                ProviderCommandStatusV1::Failed,
                ProviderCommandStatusV1::Declined,
            ],
        )
    }

    fn command_action(&mut self) -> Result<ProviderCommandActionV1, ProviderFrameDecodeError> {
        match self.u8()? {
            0 => Ok(ProviderCommandActionV1::Read {
                command: self.text("read command")?,
                name: self.text("read name")?,
                path: self.text("read path")?,
            }),
            1 => Ok(ProviderCommandActionV1::ListFiles {
                command: self.text("list-files command")?,
                path: self.option("list-files path", |decoder| decoder.text("list-files path"))?,
            }),
            2 => Ok(ProviderCommandActionV1::Search {
                command: self.text("search command")?,
                query: self.option("search query", |decoder| decoder.text("search query"))?,
                path: self.option("search path", |decoder| decoder.text("search path"))?,
            }),
            3 => Ok(ProviderCommandActionV1::Unknown {
                command: self.text("unknown command")?,
            }),
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "command action",
                tag,
            }),
        }
    }

    pub(super) fn file_change(&mut self) -> Result<ProviderFileChangeV1, ProviderFrameDecodeError> {
        Ok(ProviderFileChangeV1 {
            status: self.enum_value(
                "patch status",
                &[
                    ProviderPatchStatusV1::InProgress,
                    ProviderPatchStatusV1::Completed,
                    ProviderPatchStatusV1::Failed,
                    ProviderPatchStatusV1::Declined,
                ],
            )?,
            changes: self.file_changes()?,
        })
    }

    pub(super) fn file_changes(
        &mut self,
    ) -> Result<Vec<ProviderFileUpdateChangeV1>, ProviderFrameDecodeError> {
        self.vector("file changes", |decoder| {
            let path = decoder.text("file-change path")?;
            let diff = decoder.text("file-change diff")?;
            let kind = match decoder.u8()? {
                0 => ProviderPatchChangeKindV1::Add,
                1 => ProviderPatchChangeKindV1::Delete,
                2 => ProviderPatchChangeKindV1::Update {
                    move_path: decoder.option("move path", |decoder| decoder.text("move path"))?,
                },
                tag => {
                    return Err(ProviderFrameDecodeError::InvalidTag {
                        kind: "patch change",
                        tag,
                    });
                }
            };
            Ok(ProviderFileUpdateChangeV1 { path, diff, kind })
        })
    }

    pub(super) fn mcp_tool_call(
        &mut self,
    ) -> Result<ProviderMcpToolCallV1, ProviderFrameDecodeError> {
        Ok(ProviderMcpToolCallV1 {
            server: self.text("MCP server")?,
            tool: self.text("MCP tool")?,
            status: self.tool_status()?,
            arguments: self.structured(0)?,
            app_context: self.option("MCP app context", |decoder| {
                Ok(ProviderMcpAppContextV1 {
                    connector_id: decoder.text("MCP connector id")?,
                    link_id: decoder
                        .option("MCP link id", |decoder| decoder.text("MCP link id"))?,
                    resource_uri: decoder.option("MCP resource URI", |decoder| {
                        decoder.text("MCP resource URI")
                    })?,
                    app_name: decoder
                        .option("MCP app name", |decoder| decoder.text("MCP app name"))?,
                    template_id: decoder
                        .option("MCP template id", |decoder| decoder.text("MCP template id"))?,
                    action_name: decoder
                        .option("MCP action name", |decoder| decoder.text("MCP action name"))?,
                })
            })?,
            mcp_app_resource_uri: self.option("MCP app resource URI", |decoder| {
                decoder.text("MCP app resource URI")
            })?,
            plugin_id: self.option("MCP plugin id", |decoder| decoder.text("MCP plugin id"))?,
            result: self.option("MCP result", |decoder| {
                Ok(ProviderMcpResultV1 {
                    content: decoder.vector("MCP content", Decoder::mcp_content)?,
                    structured_content: decoder
                        .option("MCP structured content", |decoder| decoder.structured(0))?,
                    meta: decoder.option("MCP metadata", |decoder| decoder.structured(0))?,
                })
            })?,
            error: self.option("MCP error", |decoder| {
                Ok(ProviderMcpErrorV1 {
                    message: decoder.text("MCP error message")?,
                })
            })?,
            duration_ms: self.option("MCP duration", Decoder::i64)?,
        })
    }

    fn tool_status(&mut self) -> Result<ProviderToolCallStatusV1, ProviderFrameDecodeError> {
        self.enum_value(
            "tool-call status",
            &[
                ProviderToolCallStatusV1::InProgress,
                ProviderToolCallStatusV1::Completed,
                ProviderToolCallStatusV1::Failed,
            ],
        )
    }

    pub(super) fn dynamic_tool_call(
        &mut self,
    ) -> Result<ProviderDynamicToolCallV1, ProviderFrameDecodeError> {
        Ok(ProviderDynamicToolCallV1 {
            namespace: self.option("dynamic-tool namespace", |decoder| {
                decoder.text("dynamic-tool namespace")
            })?,
            tool: self.text("dynamic-tool name")?,
            arguments: self.structured(0)?,
            status: self.tool_status()?,
            content_items: self.option("dynamic-tool content", |decoder| {
                decoder.vector("dynamic-tool content", |decoder| match decoder.u8()? {
                    0 => Ok(ProviderDynamicToolOutputV1::InputText {
                        text: decoder.text("dynamic-tool input text")?,
                    }),
                    1 => Ok(ProviderDynamicToolOutputV1::InputImageLocator {
                        locator: ProviderImageLocatorV1::new(
                            decoder.raw_text("dynamic-tool image locator")?,
                        )?,
                    }),
                    2 => Ok(ProviderDynamicToolOutputV1::InputImageAsset {
                        asset: ProviderInlineImageAssetV1::new(decoder.asset()?),
                    }),
                    tag => Err(ProviderFrameDecodeError::InvalidTag {
                        kind: "dynamic-tool output",
                        tag,
                    }),
                })
            })?,
            success: self.option("dynamic-tool success", |decoder| match decoder.u8()? {
                0 => Ok(false),
                1 => Ok(true),
                tag => Err(ProviderFrameDecodeError::InvalidTag {
                    kind: "dynamic-tool success",
                    tag,
                }),
            })?,
            duration_ms: self.option("dynamic-tool duration", Decoder::i64)?,
        })
    }
}

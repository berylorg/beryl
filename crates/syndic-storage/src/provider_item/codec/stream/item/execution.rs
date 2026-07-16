use std::io::Read;

use super::super::StreamDecoder;
use crate::provider_item::*;

impl<R: Read, S: ProviderFrameTextSpanSinkV1> StreamDecoder<'_, R, S> {
    pub(super) fn command_execution(&mut self) -> Result<bool, ProviderFrameStreamError<S::Error>> {
        self.text("command", Some(ProviderLogicalTextRoleV1::Operational))?;
        self.text("command cwd", None)?;
        self.option("command process id", |decoder| {
            decoder.text("command process id", None)
        })?;
        self.enum_tag("command source", 4)?;
        let in_progress = self.enum_tag("command status", 4)? == 0;
        let count = self.count("command actions")?;
        for _ in 0..count {
            self.command_action()?;
        }
        self.option("aggregated output", |decoder| {
            decoder.text(
                "aggregated output",
                Some(ProviderLogicalTextRoleV1::Operational),
            )
        })?;
        self.option("exit code", |decoder| decoder.i32().map(|_| ()))?;
        self.option("command duration", |decoder| decoder.i64().map(|_| ()))?;
        Ok(in_progress)
    }

    fn command_action(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        match self.u8()? {
            0 => {
                self.text("read command", None)?;
                self.text("read name", None)?;
                self.text("read path", None)
            }
            1 => {
                self.text("list-files command", None)?;
                self.option("list-files path", |decoder| {
                    decoder.text("list-files path", None)
                })
            }
            2 => {
                self.text("search command", None)?;
                self.option("search query", |decoder| decoder.text("search query", None))?;
                self.option("search path", |decoder| decoder.text("search path", None))
            }
            3 => self.text("unknown command", None),
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "command action",
                tag,
            }
            .into()),
        }
    }

    pub(super) fn file_change(&mut self) -> Result<bool, ProviderFrameStreamError<S::Error>> {
        let in_progress = self.enum_tag("patch status", 4)? == 0;
        self.file_changes()?;
        Ok(in_progress)
    }

    pub(super) fn file_changes(&mut self) -> Result<(), ProviderFrameStreamError<S::Error>> {
        let count = self.count("file changes")?;
        for _ in 0..count {
            self.text("file-change path", None)?;
            self.text(
                "file-change diff",
                Some(ProviderLogicalTextRoleV1::Operational),
            )?;
            match self.u8()? {
                0 | 1 => {}
                2 => self.option("move path", |decoder| decoder.text("move path", None))?,
                tag => {
                    return Err(ProviderFrameDecodeError::InvalidTag {
                        kind: "patch change",
                        tag,
                    }
                    .into());
                }
            }
        }
        Ok(())
    }

    pub(super) fn mcp_tool_call(&mut self) -> Result<bool, ProviderFrameStreamError<S::Error>> {
        self.text("MCP server", None)?;
        self.text("MCP tool", None)?;
        let in_progress = self.enum_tag("tool-call status", 3)? == 0;
        self.structured(0)?;
        self.option("MCP app context", |decoder| {
            decoder.text("MCP connector id", None)?;
            decoder.option("MCP link id", |decoder| decoder.text("MCP link id", None))?;
            decoder.option("MCP resource URI", |decoder| {
                decoder.text("MCP resource URI", None)
            })?;
            decoder.option("MCP app name", |decoder| decoder.text("MCP app name", None))?;
            decoder.option("MCP template id", |decoder| {
                decoder.text("MCP template id", None)
            })?;
            decoder.option("MCP action name", |decoder| {
                decoder.text("MCP action name", None)
            })
        })?;
        self.option("MCP app resource URI", |decoder| {
            decoder.text("MCP app resource URI", None)
        })?;
        self.option("MCP plugin id", |decoder| {
            decoder.text("MCP plugin id", None)
        })?;
        self.option("MCP result", |decoder| {
            let count = decoder.count("MCP content")?;
            for _ in 0..count {
                decoder.mcp_content()?;
            }
            decoder.option("MCP structured content", |decoder| {
                decoder.structured(0).map(|_| ())
            })?;
            decoder.option("MCP metadata", |decoder| decoder.structured(0).map(|_| ()))
        })?;
        self.option("MCP error", |decoder| {
            decoder.text(
                "MCP error message",
                Some(ProviderLogicalTextRoleV1::Operational),
            )
        })?;
        self.option("MCP duration", |decoder| decoder.i64().map(|_| ()))?;
        Ok(in_progress)
    }

    pub(super) fn dynamic_tool_call(&mut self) -> Result<bool, ProviderFrameStreamError<S::Error>> {
        self.option("dynamic-tool namespace", |decoder| {
            decoder.text("dynamic-tool namespace", None)
        })?;
        self.text("dynamic-tool name", None)?;
        self.structured(0)?;
        let in_progress = self.enum_tag("tool-call status", 3)? == 0;
        self.option("dynamic-tool content", |decoder| {
            let count = decoder.count("dynamic-tool content")?;
            for _ in 0..count {
                match decoder.u8()? {
                    0 => decoder.text(
                        "dynamic-tool input text",
                        Some(ProviderLogicalTextRoleV1::Operational),
                    )?,
                    1 => decoder.raw_text_validate_image_locator("dynamic-tool image locator")?,
                    2 => decoder.asset()?,
                    tag => {
                        return Err(ProviderFrameDecodeError::InvalidTag {
                            kind: "dynamic-tool output",
                            tag,
                        }
                        .into());
                    }
                }
            }
            Ok(())
        })?;
        self.option("dynamic-tool success", |decoder| {
            decoder.boolean("dynamic-tool success")
        })?;
        self.option("dynamic-tool duration", |decoder| decoder.i64().map(|_| ()))?;
        Ok(in_progress)
    }
}

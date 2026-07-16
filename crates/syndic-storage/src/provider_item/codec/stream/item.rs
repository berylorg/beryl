mod activity;
mod execution;
mod message;

use std::io::Read;

use super::StreamDecoder;
use crate::ProviderItemKind;
use crate::provider_item::*;

impl<R: Read, S: ProviderFrameTextSpanSinkV1> StreamDecoder<'_, R, S> {
    pub(super) fn item(
        &mut self,
    ) -> Result<
        (ProviderItemKind, bool, ProviderFrameHistorySupportV1),
        ProviderFrameStreamError<S::Error>,
    > {
        let supported = ProviderFrameHistorySupportV1::Supported;
        match self.u8()? {
            0 => {
                self.user_message()?;
                Ok((ProviderItemKind::UserMessage, false, supported))
            }
            1 => {
                self.hook_prompt()?;
                Ok((ProviderItemKind::HookPrompt, false, supported))
            }
            2 => {
                self.agent_message()?;
                Ok((ProviderItemKind::AgentMessage, false, supported))
            }
            3 => {
                self.text("plan text", Some(ProviderLogicalTextRoleV1::Narrative))?;
                Ok((ProviderItemKind::Plan, false, supported))
            }
            4 => {
                let count = self.count("reasoning summary")?;
                for _ in 0..count {
                    self.text(
                        "reasoning summary text",
                        Some(ProviderLogicalTextRoleV1::Activity),
                    )?;
                }
                Ok((ProviderItemKind::Reasoning, false, supported))
            }
            5 => self
                .command_execution()
                .map(|in_progress| (ProviderItemKind::CommandExecution, in_progress, supported)),
            6 => self
                .file_change()
                .map(|in_progress| (ProviderItemKind::FileChange, in_progress, supported)),
            7 => self
                .mcp_tool_call()
                .map(|in_progress| (ProviderItemKind::McpToolCall, in_progress, supported)),
            8 => self
                .dynamic_tool_call()
                .map(|in_progress| (ProviderItemKind::DynamicToolCall, in_progress, supported)),
            9 => self.collab_tool_call().map(|in_progress| {
                (
                    ProviderItemKind::CollabAgentToolCall,
                    in_progress,
                    supported,
                )
            }),
            10 => {
                self.subagent_activity()?;
                Ok((ProviderItemKind::SubAgentActivity, false, supported))
            }
            11 => {
                let history_support = self.web_search()?;
                Ok((ProviderItemKind::WebSearch, false, history_support))
            }
            12 => {
                self.text("image-view path", Some(ProviderLogicalTextRoleV1::Activity))?;
                Ok((ProviderItemKind::ImageView, false, supported))
            }
            13 => {
                self.u64()?;
                Ok((ProviderItemKind::Sleep, false, supported))
            }
            14 => {
                let in_progress = self.image_generation()?;
                Ok((
                    ProviderItemKind::StandaloneImageGeneration,
                    in_progress,
                    supported,
                ))
            }
            15 => {
                self.text(
                    "entered-review text",
                    Some(ProviderLogicalTextRoleV1::Activity),
                )?;
                Ok((ProviderItemKind::EnteredReviewMode, false, supported))
            }
            16 => {
                self.text(
                    "exited-review text",
                    Some(ProviderLogicalTextRoleV1::Activity),
                )?;
                Ok((ProviderItemKind::ExitedReviewMode, false, supported))
            }
            17 => Ok((ProviderItemKind::ContextCompaction, false, supported)),
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "provider item",
                tag,
            }
            .into()),
        }
    }

    pub(super) fn delta(&mut self) -> Result<ProviderItemKind, ProviderFrameStreamError<S::Error>> {
        let (kind, role, has_index) = match self.u8()? {
            0 => (
                ProviderItemKind::AgentMessage,
                Some(ProviderLogicalTextRoleV1::Narrative),
                false,
            ),
            1 => (
                ProviderItemKind::Plan,
                Some(ProviderLogicalTextRoleV1::Narrative),
                false,
            ),
            2 => {
                self.u64()?;
                return Ok(ProviderItemKind::Reasoning);
            }
            3 => (
                ProviderItemKind::Reasoning,
                Some(ProviderLogicalTextRoleV1::Activity),
                true,
            ),
            4 => {
                self.u64()?;
                return Ok(ProviderItemKind::Reasoning);
            }
            5 => (
                ProviderItemKind::CommandExecution,
                Some(ProviderLogicalTextRoleV1::Operational),
                false,
            ),
            6 => (
                ProviderItemKind::FileChange,
                Some(ProviderLogicalTextRoleV1::Operational),
                false,
            ),
            7 => {
                self.file_changes()?;
                return Ok(ProviderItemKind::FileChange);
            }
            8 => (
                ProviderItemKind::McpToolCall,
                Some(ProviderLogicalTextRoleV1::Operational),
                false,
            ),
            tag => {
                return Err(ProviderFrameDecodeError::InvalidTag {
                    kind: "provider delta",
                    tag,
                }
                .into());
            }
        };
        if has_index {
            self.u64()?;
        }
        self.text("provider delta text", role)?;
        Ok(kind)
    }
}

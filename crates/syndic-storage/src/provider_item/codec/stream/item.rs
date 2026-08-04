mod activity;
mod execution;
mod message;

use std::io::Read;

use super::StreamDecoder;
use crate::provider_item::*;
use crate::{ContentReference, ProviderItemKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StreamItemSummary {
    pub(super) kind: ProviderItemKind,
    pub(super) in_progress: bool,
    pub(super) history_support: ProviderFrameHistorySupportV1,
    pub(super) message_phase: Option<ProviderMessagePhaseV1>,
    pub(super) submitted_content: Option<ContentReference>,
}

impl StreamItemSummary {
    pub(super) const fn supported(kind: ProviderItemKind) -> Self {
        Self {
            kind,
            in_progress: false,
            history_support: ProviderFrameHistorySupportV1::Supported,
            message_phase: None,
            submitted_content: None,
        }
    }

    const fn in_progress(kind: ProviderItemKind, in_progress: bool) -> Self {
        Self {
            in_progress,
            ..Self::supported(kind)
        }
    }

    const fn with_history_support(
        kind: ProviderItemKind,
        history_support: ProviderFrameHistorySupportV1,
    ) -> Self {
        Self {
            history_support,
            ..Self::supported(kind)
        }
    }
}

impl<R: Read, S: ProviderFrameTextSpanSinkV1> StreamDecoder<'_, R, S> {
    pub(super) fn item(&mut self) -> Result<StreamItemSummary, ProviderFrameStreamError<S::Error>> {
        match self.u8()? {
            0 => {
                let submitted_content = self.user_message()?;
                Ok(StreamItemSummary {
                    submitted_content: Some(submitted_content),
                    ..StreamItemSummary::supported(ProviderItemKind::UserMessage)
                })
            }
            1 => {
                self.hook_prompt()?;
                Ok(StreamItemSummary::supported(ProviderItemKind::HookPrompt))
            }
            2 => {
                let message_phase = self.agent_message()?;
                Ok(StreamItemSummary {
                    message_phase,
                    ..StreamItemSummary::supported(ProviderItemKind::AgentMessage)
                })
            }
            3 => {
                self.text("plan text", Some(ProviderLogicalTextRoleV1::Narrative))?;
                Ok(StreamItemSummary::supported(ProviderItemKind::Plan))
            }
            4 => {
                let count = self.count("reasoning summary")?;
                for _ in 0..count {
                    self.text(
                        "reasoning summary text",
                        Some(ProviderLogicalTextRoleV1::Activity),
                    )?;
                }
                Ok(StreamItemSummary::supported(ProviderItemKind::Reasoning))
            }
            5 => self.command_execution().map(|value| {
                StreamItemSummary::in_progress(ProviderItemKind::CommandExecution, value)
            }),
            6 => self
                .file_change()
                .map(|value| StreamItemSummary::in_progress(ProviderItemKind::FileChange, value)),
            7 => self
                .mcp_tool_call()
                .map(|value| StreamItemSummary::in_progress(ProviderItemKind::McpToolCall, value)),
            8 => self.dynamic_tool_call().map(|value| {
                StreamItemSummary::in_progress(ProviderItemKind::DynamicToolCall, value)
            }),
            9 => self.collab_tool_call().map(|value| {
                StreamItemSummary::in_progress(ProviderItemKind::CollabAgentToolCall, value)
            }),
            10 => {
                self.subagent_activity()?;
                Ok(StreamItemSummary::supported(
                    ProviderItemKind::SubAgentActivity,
                ))
            }
            11 => {
                let history_support = self.web_search()?;
                Ok(StreamItemSummary::with_history_support(
                    ProviderItemKind::WebSearch,
                    history_support,
                ))
            }
            12 => {
                self.text("image-view path", Some(ProviderLogicalTextRoleV1::Activity))?;
                Ok(StreamItemSummary::supported(ProviderItemKind::ImageView))
            }
            13 => {
                self.u64()?;
                Ok(StreamItemSummary::supported(ProviderItemKind::Sleep))
            }
            14 => {
                let in_progress = self.image_generation()?;
                Ok(StreamItemSummary::in_progress(
                    ProviderItemKind::StandaloneImageGeneration,
                    in_progress,
                ))
            }
            15 => {
                self.text(
                    "entered-review text",
                    Some(ProviderLogicalTextRoleV1::Activity),
                )?;
                Ok(StreamItemSummary::supported(
                    ProviderItemKind::EnteredReviewMode,
                ))
            }
            16 => {
                self.text(
                    "exited-review text",
                    Some(ProviderLogicalTextRoleV1::Activity),
                )?;
                Ok(StreamItemSummary::supported(
                    ProviderItemKind::ExitedReviewMode,
                ))
            }
            17 => Ok(StreamItemSummary::supported(
                ProviderItemKind::ContextCompaction,
            )),
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

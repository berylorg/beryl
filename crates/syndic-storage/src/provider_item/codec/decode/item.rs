mod activity;
mod execution;
mod message;

use super::Decoder;
use crate::provider_item::*;

impl Decoder<'_> {
    pub(super) fn item(&mut self) -> Result<ProviderItemV1, ProviderFrameDecodeError> {
        match self.u8()? {
            0 => self.user_message().map(ProviderItemV1::UserMessage),
            1 => self.hook_prompt().map(ProviderItemV1::HookPrompt),
            2 => self.agent_message().map(ProviderItemV1::AgentMessage),
            3 => Ok(ProviderItemV1::Plan(ProviderPlanV1 {
                text: self.text("plan text")?,
            })),
            4 => Ok(ProviderItemV1::Reasoning(ProviderReasoningV1 {
                summary: self.vector("reasoning summary", |decoder| {
                    decoder.text("reasoning summary text")
                })?,
            })),
            5 => self
                .command_execution()
                .map(ProviderItemV1::CommandExecution),
            6 => self.file_change().map(ProviderItemV1::FileChange),
            7 => self.mcp_tool_call().map(ProviderItemV1::McpToolCall),
            8 => self
                .dynamic_tool_call()
                .map(ProviderItemV1::DynamicToolCall),
            9 => self
                .collab_tool_call()
                .map(ProviderItemV1::CollabAgentToolCall),
            10 => self
                .subagent_activity()
                .map(ProviderItemV1::SubAgentActivity),
            11 => self.web_search().map(ProviderItemV1::WebSearch),
            12 => Ok(ProviderItemV1::ImageView(ProviderImageViewV1 {
                path: self.text("image-view path")?,
            })),
            13 => Ok(ProviderItemV1::Sleep(ProviderSleepV1 {
                duration_ms: self.u64()?,
            })),
            14 => self
                .image_generation()
                .map(ProviderItemV1::StandaloneImageGeneration),
            15 => Ok(ProviderItemV1::EnteredReviewMode(
                ProviderEnteredReviewModeV1 {
                    review: self.text("entered-review text")?,
                },
            )),
            16 => Ok(ProviderItemV1::ExitedReviewMode(
                ProviderExitedReviewModeV1 {
                    review: self.text("exited-review text")?,
                },
            )),
            17 => Ok(ProviderItemV1::ContextCompaction),
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "provider item",
                tag,
            }),
        }
    }

    pub(super) fn delta(&mut self) -> Result<ProviderItemDeltaV1, ProviderFrameDecodeError> {
        match self.u8()? {
            0 => Ok(ProviderItemDeltaV1::AgentMessage {
                delta: self.text("agent-message delta")?,
            }),
            1 => Ok(ProviderItemDeltaV1::Plan {
                delta: self.text("plan delta")?,
            }),
            2 => Ok(ProviderItemDeltaV1::ReasoningSummaryPartAdded {
                summary_index: self.u64()?,
            }),
            3 => Ok(ProviderItemDeltaV1::ReasoningSummaryText {
                summary_index: self.u64()?,
                delta: self.text("reasoning-summary delta")?,
            }),
            4 => Ok(ProviderItemDeltaV1::ReasoningTextObserved {
                content_index: self.u64()?,
            }),
            5 => Ok(ProviderItemDeltaV1::CommandExecutionOutput {
                delta: self.text("command-output delta")?,
            }),
            6 => Ok(ProviderItemDeltaV1::FileChangeOutput {
                delta: self.text("file-change output delta")?,
            }),
            7 => Ok(ProviderItemDeltaV1::FileChangePatchUpdated {
                changes: self.file_changes()?,
            }),
            8 => Ok(ProviderItemDeltaV1::McpToolCallProgress {
                message: self.text("MCP progress message")?,
            }),
            tag => Err(ProviderFrameDecodeError::InvalidTag {
                kind: "provider delta",
                tag,
            }),
        }
    }
}

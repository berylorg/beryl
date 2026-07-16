mod activity;
mod execution;
mod message;

use super::Encoder;
use crate::provider_item::*;

impl<S: ProviderFrameSinkV1> Encoder<'_, S> {
    pub(super) fn item(
        &mut self,
        item: &ProviderItemV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        match item {
            ProviderItemV1::UserMessage(value) => {
                self.u8(0)?;
                self.user_message(value)
            }
            ProviderItemV1::HookPrompt(value) => {
                self.u8(1)?;
                self.hook_prompt(value)
            }
            ProviderItemV1::AgentMessage(value) => {
                self.u8(2)?;
                self.agent_message(value)
            }
            ProviderItemV1::Plan(value) => {
                self.u8(3)?;
                self.text(&value.text, Some(ProviderLogicalTextRoleV1::Narrative))
            }
            ProviderItemV1::Reasoning(value) => {
                self.u8(4)?;
                self.count(value.summary.len())?;
                for summary in &value.summary {
                    self.text(summary, Some(ProviderLogicalTextRoleV1::Activity))?;
                }
                Ok(())
            }
            ProviderItemV1::CommandExecution(value) => {
                self.u8(5)?;
                self.command_execution(value)
            }
            ProviderItemV1::FileChange(value) => {
                self.u8(6)?;
                self.file_change(value)
            }
            ProviderItemV1::McpToolCall(value) => {
                self.u8(7)?;
                self.mcp_tool_call(value)
            }
            ProviderItemV1::DynamicToolCall(value) => {
                self.u8(8)?;
                self.dynamic_tool_call(value)
            }
            ProviderItemV1::CollabAgentToolCall(value) => {
                self.u8(9)?;
                self.collab_tool_call(value)
            }
            ProviderItemV1::SubAgentActivity(value) => {
                self.u8(10)?;
                self.subagent_activity(value)
            }
            ProviderItemV1::WebSearch(value) => {
                self.u8(11)?;
                self.web_search(value)
            }
            ProviderItemV1::ImageView(value) => {
                self.u8(12)?;
                self.text(&value.path, Some(ProviderLogicalTextRoleV1::Activity))
            }
            ProviderItemV1::Sleep(value) => {
                self.u8(13)?;
                self.u64(value.duration_ms)
            }
            ProviderItemV1::StandaloneImageGeneration(value) => {
                self.u8(14)?;
                self.image_generation(value)
            }
            ProviderItemV1::EnteredReviewMode(value) => {
                self.u8(15)?;
                self.text(&value.review, Some(ProviderLogicalTextRoleV1::Activity))
            }
            ProviderItemV1::ExitedReviewMode(value) => {
                self.u8(16)?;
                self.text(&value.review, Some(ProviderLogicalTextRoleV1::Activity))
            }
            ProviderItemV1::ContextCompaction => self.u8(17),
        }
    }

    pub(super) fn delta(
        &mut self,
        delta: &ProviderItemDeltaV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        match delta {
            ProviderItemDeltaV1::AgentMessage { delta } => {
                self.u8(0)?;
                self.text(delta, Some(ProviderLogicalTextRoleV1::Narrative))
            }
            ProviderItemDeltaV1::Plan { delta } => {
                self.u8(1)?;
                self.text(delta, Some(ProviderLogicalTextRoleV1::Narrative))
            }
            ProviderItemDeltaV1::ReasoningSummaryPartAdded { summary_index } => {
                self.u8(2)?;
                self.u64(*summary_index)
            }
            ProviderItemDeltaV1::ReasoningSummaryText {
                summary_index,
                delta,
            } => {
                self.u8(3)?;
                self.u64(*summary_index)?;
                self.text(delta, Some(ProviderLogicalTextRoleV1::Activity))
            }
            ProviderItemDeltaV1::ReasoningTextObserved { content_index } => {
                self.u8(4)?;
                self.u64(*content_index)
            }
            ProviderItemDeltaV1::CommandExecutionOutput { delta } => {
                self.u8(5)?;
                self.text(delta, Some(ProviderLogicalTextRoleV1::Operational))
            }
            ProviderItemDeltaV1::FileChangeOutput { delta } => {
                self.u8(6)?;
                self.text(delta, Some(ProviderLogicalTextRoleV1::Operational))
            }
            ProviderItemDeltaV1::FileChangePatchUpdated { changes } => {
                self.u8(7)?;
                self.file_changes(changes)
            }
            ProviderItemDeltaV1::McpToolCallProgress { message } => {
                self.u8(8)?;
                self.text(message, Some(ProviderLogicalTextRoleV1::Operational))
            }
        }
    }

    pub(super) fn enum_tag<T: Copy + PartialEq>(
        &mut self,
        value: T,
        values: &[T],
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        let tag = values
            .iter()
            .position(|candidate| candidate == &value)
            .expect("closed provider enum includes every variant");
        self.u8(u8::try_from(tag).expect("closed provider enum tag fits u8"))
    }
}

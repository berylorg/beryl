use super::super::Encoder;
use crate::provider_item::*;

impl<S: ProviderFrameSinkV1> Encoder<'_, S> {
    pub(super) fn collab_tool_call(
        &mut self,
        value: &ProviderCollabAgentToolCallV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.enum_tag(
            value.tool,
            &[
                ProviderCollabToolV1::SpawnAgent,
                ProviderCollabToolV1::SendInput,
                ProviderCollabToolV1::ResumeAgent,
                ProviderCollabToolV1::Wait,
                ProviderCollabToolV1::CloseAgent,
            ],
        )?;
        self.enum_tag(
            value.status,
            &[
                ProviderCollabToolStatusV1::InProgress,
                ProviderCollabToolStatusV1::Completed,
                ProviderCollabToolStatusV1::Failed,
            ],
        )?;
        self.cas_thread_id(&value.sender_thread_id)?;
        self.count(value.receiver_thread_ids.len())?;
        for thread_id in &value.receiver_thread_ids {
            self.cas_thread_id(thread_id)?;
        }
        self.option(&value.prompt, |encoder, value| {
            encoder.text(value, Some(ProviderLogicalTextRoleV1::Activity))
        })?;
        self.option(&value.model, |encoder, value| encoder.text(value, None))?;
        self.option(&value.reasoning_effort, |encoder, value| {
            encoder.text(value, None)
        })?;
        self.count(value.agents_states.len())?;
        for entry in &value.agents_states {
            self.text(&entry.agent, None)?;
            self.enum_tag(
                entry.state.status,
                &[
                    ProviderCollabAgentStatusV1::PendingInit,
                    ProviderCollabAgentStatusV1::Running,
                    ProviderCollabAgentStatusV1::Interrupted,
                    ProviderCollabAgentStatusV1::Completed,
                    ProviderCollabAgentStatusV1::Errored,
                    ProviderCollabAgentStatusV1::Shutdown,
                    ProviderCollabAgentStatusV1::NotFound,
                ],
            )?;
            self.option(&entry.state.message, |encoder, value| {
                encoder.text(value, Some(ProviderLogicalTextRoleV1::Activity))
            })?;
        }
        Ok(())
    }

    pub(super) fn subagent_activity(
        &mut self,
        value: &ProviderSubAgentActivityV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.enum_tag(
            value.kind,
            &[
                ProviderSubAgentActivityKindV1::Started,
                ProviderSubAgentActivityKindV1::Interacted,
                ProviderSubAgentActivityKindV1::Interrupted,
            ],
        )?;
        self.cas_thread_id(&value.agent_thread_id)?;
        self.text(&value.agent_path, Some(ProviderLogicalTextRoleV1::Activity))
    }

    pub(super) fn web_search(
        &mut self,
        value: &ProviderWebSearchV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.text(&value.query, Some(ProviderLogicalTextRoleV1::Activity))?;
        self.option(&value.action, |encoder, action| match action {
            ProviderWebSearchActionV1::Search { query, queries } => {
                encoder.u8(0)?;
                encoder.option(query, |encoder, value| {
                    encoder.text(value, Some(ProviderLogicalTextRoleV1::Activity))
                })?;
                encoder.option(queries, |encoder, values| {
                    encoder.count(values.len())?;
                    for value in values {
                        encoder.text(value, Some(ProviderLogicalTextRoleV1::Activity))?;
                    }
                    Ok(())
                })
            }
            ProviderWebSearchActionV1::OpenPage { url } => {
                encoder.u8(1)?;
                encoder.option(url, |encoder, value| {
                    encoder.text(value, Some(ProviderLogicalTextRoleV1::Activity))
                })
            }
            ProviderWebSearchActionV1::FindInPage { url, pattern } => {
                encoder.u8(2)?;
                encoder.option(url, |encoder, value| {
                    encoder.text(value, Some(ProviderLogicalTextRoleV1::Activity))
                })?;
                encoder.option(pattern, |encoder, value| {
                    encoder.text(value, Some(ProviderLogicalTextRoleV1::Activity))
                })
            }
            ProviderWebSearchActionV1::Other => encoder.u8(3),
        })
    }

    pub(super) fn image_generation(
        &mut self,
        value: &ProviderImageGenerationV1,
    ) -> Result<(), ProviderFrameEncodeError<S::Error>> {
        self.enum_tag(
            value.status,
            &[
                ProviderImageGenerationStatusV1::InProgress,
                ProviderImageGenerationStatusV1::Failed,
                ProviderImageGenerationStatusV1::Completed,
            ],
        )?;
        self.option(&value.revised_prompt, |encoder, value| {
            encoder.text(value, Some(ProviderLogicalTextRoleV1::Activity))
        })?;
        self.option(&value.saved_path, |encoder, value| {
            encoder.text(value, None)
        })
    }
}

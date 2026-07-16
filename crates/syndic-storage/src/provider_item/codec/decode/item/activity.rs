use super::super::Decoder;
use crate::provider_item::*;

impl Decoder<'_> {
    pub(super) fn collab_tool_call(
        &mut self,
    ) -> Result<ProviderCollabAgentToolCallV1, ProviderFrameDecodeError> {
        Ok(ProviderCollabAgentToolCallV1 {
            tool: self.enum_value(
                "collaboration tool",
                &[
                    ProviderCollabToolV1::SpawnAgent,
                    ProviderCollabToolV1::SendInput,
                    ProviderCollabToolV1::ResumeAgent,
                    ProviderCollabToolV1::Wait,
                    ProviderCollabToolV1::CloseAgent,
                ],
            )?,
            status: self.enum_value(
                "collaboration tool status",
                &[
                    ProviderCollabToolStatusV1::InProgress,
                    ProviderCollabToolStatusV1::Completed,
                    ProviderCollabToolStatusV1::Failed,
                ],
            )?,
            sender_thread_id: self.cas_thread_id()?,
            receiver_thread_ids: self.vector("receiver thread ids", Decoder::cas_thread_id)?,
            prompt: self.option("collaboration prompt", |decoder| {
                decoder.text("collaboration prompt")
            })?,
            model: self.option("collaboration model", |decoder| {
                decoder.text("collaboration model")
            })?,
            reasoning_effort: self.option("collaboration reasoning effort", |decoder| {
                decoder.text("collaboration reasoning effort")
            })?,
            agents_states: self.vector("collaboration agent states", |decoder| {
                Ok(ProviderCollabAgentStateEntryV1 {
                    agent: decoder.text("collaboration agent key")?,
                    state: ProviderCollabAgentStateV1 {
                        status: decoder.enum_value(
                            "collaboration agent status",
                            &[
                                ProviderCollabAgentStatusV1::PendingInit,
                                ProviderCollabAgentStatusV1::Running,
                                ProviderCollabAgentStatusV1::Interrupted,
                                ProviderCollabAgentStatusV1::Completed,
                                ProviderCollabAgentStatusV1::Errored,
                                ProviderCollabAgentStatusV1::Shutdown,
                                ProviderCollabAgentStatusV1::NotFound,
                            ],
                        )?,
                        message: decoder.option("collaboration agent message", |decoder| {
                            decoder.text("collaboration agent message")
                        })?,
                    },
                })
            })?,
        })
    }

    pub(super) fn subagent_activity(
        &mut self,
    ) -> Result<ProviderSubAgentActivityV1, ProviderFrameDecodeError> {
        Ok(ProviderSubAgentActivityV1 {
            kind: self.enum_value(
                "subagent activity kind",
                &[
                    ProviderSubAgentActivityKindV1::Started,
                    ProviderSubAgentActivityKindV1::Interacted,
                    ProviderSubAgentActivityKindV1::Interrupted,
                ],
            )?,
            agent_thread_id: self.cas_thread_id()?,
            agent_path: self.text("subagent path")?,
        })
    }

    pub(super) fn web_search(&mut self) -> Result<ProviderWebSearchV1, ProviderFrameDecodeError> {
        Ok(ProviderWebSearchV1 {
            query: self.text("web-search query")?,
            action: self.option("web-search action", |decoder| match decoder.u8()? {
                0 => Ok(ProviderWebSearchActionV1::Search {
                    query: decoder.option("web-search action query", |decoder| {
                        decoder.text("web-search action query")
                    })?,
                    queries: decoder.option("web-search queries", |decoder| {
                        decoder.vector("web-search queries", |decoder| {
                            decoder.text("web-search query")
                        })
                    })?,
                }),
                1 => Ok(ProviderWebSearchActionV1::OpenPage {
                    url: decoder
                        .option("web-search URL", |decoder| decoder.text("web-search URL"))?,
                }),
                2 => Ok(ProviderWebSearchActionV1::FindInPage {
                    url: decoder
                        .option("web-search URL", |decoder| decoder.text("web-search URL"))?,
                    pattern: decoder.option("web-search pattern", |decoder| {
                        decoder.text("web-search pattern")
                    })?,
                }),
                3 => Ok(ProviderWebSearchActionV1::Other),
                tag => Err(ProviderFrameDecodeError::InvalidTag {
                    kind: "web-search action",
                    tag,
                }),
            })?,
        })
    }

    pub(super) fn image_generation(
        &mut self,
    ) -> Result<ProviderImageGenerationV1, ProviderFrameDecodeError> {
        Ok(ProviderImageGenerationV1 {
            status: self.enum_value(
                "image-generation status",
                &[
                    ProviderImageGenerationStatusV1::InProgress,
                    ProviderImageGenerationStatusV1::Failed,
                    ProviderImageGenerationStatusV1::Completed,
                ],
            )?,
            revised_prompt: self.option("image-generation revised prompt", |decoder| {
                decoder.text("image-generation revised prompt")
            })?,
            saved_path: self.option("image-generation saved path", |decoder| {
                decoder.text("image-generation saved path")
            })?,
        })
    }
}

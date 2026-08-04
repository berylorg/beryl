use super::*;

pub(crate) fn enc_provider_item_kind(e: &mut Encoder, value: crate::ProviderItemKind) {
    use crate::ProviderItemKind::*;
    e.u8(match value {
        UserMessage => 0,
        HookPrompt => 1,
        AgentMessage => 2,
        Plan => 3,
        Reasoning => 4,
        CommandExecution => 5,
        FileChange => 6,
        McpToolCall => 7,
        DynamicToolCall => 8,
        CollabAgentToolCall => 9,
        SubAgentActivity => 10,
        WebSearch => 11,
        ImageView => 12,
        Sleep => 13,
        StandaloneImageGeneration => 14,
        EnteredReviewMode => 15,
        ExitedReviewMode => 16,
        ContextCompaction => 17,
    });
}

pub(crate) fn dec_provider_item_kind(
    d: &mut Decoder<'_>,
) -> Result<crate::ProviderItemKind, CodecError> {
    use crate::ProviderItemKind::*;
    Ok(match d.u8()? {
        0 => UserMessage,
        1 => HookPrompt,
        2 => AgentMessage,
        3 => Plan,
        4 => Reasoning,
        5 => CommandExecution,
        6 => FileChange,
        7 => McpToolCall,
        8 => DynamicToolCall,
        9 => CollabAgentToolCall,
        10 => SubAgentActivity,
        11 => WebSearch,
        12 => ImageView,
        13 => Sleep,
        14 => StandaloneImageGeneration,
        15 => EnteredReviewMode,
        16 => ExitedReviewMode,
        17 => ContextCompaction,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider item kind",
                tag,
            });
        }
    })
}

pub(crate) fn enc_assistant_phase(e: &mut Encoder, value: crate::AssistantMessagePhase) {
    e.u8(match value {
        crate::AssistantMessagePhase::Commentary => 0,
        crate::AssistantMessagePhase::FinalAnswer => 1,
        crate::AssistantMessagePhase::Unknown => 2,
    });
}

pub(crate) fn dec_assistant_phase(
    d: &mut Decoder<'_>,
) -> Result<crate::AssistantMessagePhase, CodecError> {
    Ok(match d.u8()? {
        0 => crate::AssistantMessagePhase::Commentary,
        1 => crate::AssistantMessagePhase::FinalAnswer,
        2 => crate::AssistantMessagePhase::Unknown,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "assistant phase",
                tag,
            });
        }
    })
}

pub(crate) fn enc_provider_item_lifecycle(e: &mut Encoder, value: crate::ProviderItemLifecycle) {
    e.u8(match value {
        crate::ProviderItemLifecycle::AwaitingCorrelation => 0,
        crate::ProviderItemLifecycle::Started => 1,
        crate::ProviderItemLifecycle::Completed => 2,
    });
}

pub(crate) fn dec_provider_item_lifecycle(
    d: &mut Decoder<'_>,
) -> Result<crate::ProviderItemLifecycle, CodecError> {
    Ok(match d.u8()? {
        0 => crate::ProviderItemLifecycle::AwaitingCorrelation,
        1 => crate::ProviderItemLifecycle::Started,
        2 => crate::ProviderItemLifecycle::Completed,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider item lifecycle",
                tag,
            });
        }
    })
}

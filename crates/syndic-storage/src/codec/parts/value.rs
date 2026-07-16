use std::num::NonZeroU64;

use super::*;

pub(crate) fn enc_content_encoding(e: &mut Encoder, value: crate::ContentEncoding) {
    e.u8(match value {
        crate::ContentEncoding::ComposerV1 => 0,
        crate::ContentEncoding::Utf8V1 => 1,
        crate::ContentEncoding::ProviderItemV1 => 2,
    });
}

pub(crate) fn dec_content_encoding(
    d: &mut Decoder<'_>,
) -> Result<crate::ContentEncoding, CodecError> {
    match d.u8()? {
        0 => Ok(crate::ContentEncoding::ComposerV1),
        1 => Ok(crate::ContentEncoding::Utf8V1),
        2 => Ok(crate::ContentEncoding::ProviderItemV1),
        tag => Err(CodecError::InvalidTag {
            kind: "content encoding",
            tag,
        }),
    }
}

pub(crate) fn enc_content_lifecycle(e: &mut Encoder, value: crate::ContentLifecycle) {
    e.u8(match value {
        crate::ContentLifecycle::Building => 0,
        crate::ContentLifecycle::Sealed => 1,
        crate::ContentLifecycle::Live => 2,
        crate::ContentLifecycle::Finalized => 3,
    });
}

pub(crate) fn dec_content_lifecycle(
    d: &mut Decoder<'_>,
) -> Result<crate::ContentLifecycle, CodecError> {
    match d.u8()? {
        0 => Ok(crate::ContentLifecycle::Building),
        1 => Ok(crate::ContentLifecycle::Sealed),
        2 => Ok(crate::ContentLifecycle::Live),
        3 => Ok(crate::ContentLifecycle::Finalized),
        tag => Err(CodecError::InvalidTag {
            kind: "content lifecycle",
            tag,
        }),
    }
}

pub(crate) fn enc_content_summary(e: &mut Encoder, value: crate::ContentSummary) {
    e.u64(value.chunk_count());
    e.u64(value.piece_count());
    e.u64(value.encoded_bytes());
    e.u64(value.logical_utf8_bytes());
    e.u64(value.atom_count());
    e.u64(value.image_marker_count());
    e.fixed32(&value.marker_digest());
    e.fixed32(value.digest().as_bytes());
}

pub(crate) fn dec_content_summary(
    d: &mut Decoder<'_>,
) -> Result<crate::ContentSummary, CodecError> {
    Ok(crate::ContentSummary::new(
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        d.fixed32()?,
        SyndicContentDigest::from_bytes(d.fixed32()?),
    ))
}

pub(crate) fn enc_content_ref(e: &mut Encoder, value: crate::ContentReference) {
    enc_content(e, value.id());
    enc_content_rev(e, value.revision());
    enc_content_encoding(e, value.encoding());
    enc_content_summary(e, value.summary());
}

pub(crate) fn dec_content_ref(d: &mut Decoder<'_>) -> Result<crate::ContentReference, CodecError> {
    Ok(crate::ContentReference::new(
        dec_content(d)?,
        dec_content_rev(d)?,
        dec_content_encoding(d)?,
        dec_content_summary(d)?,
    ))
}

pub(crate) fn enc_asset_id(e: &mut Encoder, asset_id: AssetId) {
    e.u8(match asset_id.version() {
        AssetIdentityVersion::Sha256V1 => 1,
    });
    e.fixed32(&asset_id.digest());
    e.u64(asset_id.length().get());
}

pub(crate) fn dec_asset_id(d: &mut Decoder<'_>) -> Result<AssetId, CodecError> {
    match d.u8()? {
        1 => {
            let digest = d.fixed32()?;
            let length =
                NonZeroU64::new(d.u64()?).ok_or(CodecError::InvalidLength("asset byte length"))?;
            Ok(AssetId::sha256_v1(digest, length))
        }
        tag => Err(CodecError::InvalidTag {
            kind: "asset identity version",
            tag,
        }),
    }
}

pub(crate) fn enc_input_marker_owner(e: &mut Encoder, owner: crate::InputMarkerOwner) {
    match owner {
        crate::InputMarkerOwner::AcceptedInput(id) => {
            e.u8(0);
            enc_accepted(e, id);
        }
        crate::InputMarkerOwner::CanonicalItem(id) => {
            e.u8(1);
            enc_item(e, id);
        }
    }
}

pub(crate) fn dec_input_marker_owner(
    d: &mut Decoder<'_>,
) -> Result<crate::InputMarkerOwner, CodecError> {
    match d.u8()? {
        0 => dec_accepted(d).map(crate::InputMarkerOwner::AcceptedInput),
        1 => dec_item(d).map(crate::InputMarkerOwner::CanonicalItem),
        tag => Err(CodecError::InvalidTag {
            kind: "input-marker owner",
            tag,
        }),
    }
}

pub(crate) fn enc_turn_kind(e: &mut Encoder, value: crate::TurnKind) {
    match value {
        crate::TurnKind::OrdinaryUser => e.u8(0),
        crate::TurnKind::ProviderOperation(crate::ProviderOperationKind::ContextCompaction) => {
            e.u8(1)
        }
    }
}

pub(crate) fn dec_turn_kind(d: &mut Decoder<'_>) -> Result<crate::TurnKind, CodecError> {
    match d.u8()? {
        0 => Ok(crate::TurnKind::OrdinaryUser),
        1 => Ok(crate::TurnKind::ProviderOperation(
            crate::ProviderOperationKind::ContextCompaction,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "turn kind",
            tag,
        }),
    }
}

pub(crate) fn enc_turn_lifecycle(e: &mut Encoder, value: crate::TurnLifecycle) {
    e.u8(match value {
        crate::TurnLifecycle::Pending => 0,
        crate::TurnLifecycle::Active => 1,
        crate::TurnLifecycle::Complete => 2,
        crate::TurnLifecycle::Interrupted => 3,
        crate::TurnLifecycle::Failed => 4,
        crate::TurnLifecycle::Incomplete => 5,
        crate::TurnLifecycle::UnknownTerminal => 6,
    });
}

pub(crate) fn dec_turn_lifecycle(d: &mut Decoder<'_>) -> Result<crate::TurnLifecycle, CodecError> {
    Ok(match d.u8()? {
        0 => crate::TurnLifecycle::Pending,
        1 => crate::TurnLifecycle::Active,
        2 => crate::TurnLifecycle::Complete,
        3 => crate::TurnLifecycle::Interrupted,
        4 => crate::TurnLifecycle::Failed,
        5 => crate::TurnLifecycle::Incomplete,
        6 => crate::TurnLifecycle::UnknownTerminal,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "turn lifecycle",
                tag,
            });
        }
    })
}

pub(crate) fn enc_turn_terminal_outcome(e: &mut Encoder, value: crate::TurnTerminalOutcome) {
    e.u8(match value {
        crate::TurnTerminalOutcome::Complete => 0,
        crate::TurnTerminalOutcome::Interrupted => 1,
        crate::TurnTerminalOutcome::Failed => 2,
        crate::TurnTerminalOutcome::Incomplete => 3,
        crate::TurnTerminalOutcome::UnknownTerminal => 4,
    });
}

pub(crate) fn dec_turn_terminal_outcome(
    d: &mut Decoder<'_>,
) -> Result<crate::TurnTerminalOutcome, CodecError> {
    Ok(match d.u8()? {
        0 => crate::TurnTerminalOutcome::Complete,
        1 => crate::TurnTerminalOutcome::Interrupted,
        2 => crate::TurnTerminalOutcome::Failed,
        3 => crate::TurnTerminalOutcome::Incomplete,
        4 => crate::TurnTerminalOutcome::UnknownTerminal,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "turn terminal outcome",
                tag,
            });
        }
    })
}

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

pub(crate) fn enc_unsupported_history_reason(
    e: &mut Encoder,
    value: crate::UnsupportedHistoryReason,
) {
    e.u8(match value {
        crate::UnsupportedHistoryReason::UnknownPublicItem => 0,
        crate::UnsupportedHistoryReason::MalformedRequiredField => 1,
        crate::UnsupportedHistoryReason::UnsupportedRequiredPayload => 2,
        crate::UnsupportedHistoryReason::HostedImageGeneration => 3,
        crate::UnsupportedHistoryReason::ImpossibleLifecycle => 4,
    });
}

pub(crate) fn dec_unsupported_history_reason(
    d: &mut Decoder<'_>,
) -> Result<crate::UnsupportedHistoryReason, CodecError> {
    Ok(match d.u8()? {
        0 => crate::UnsupportedHistoryReason::UnknownPublicItem,
        1 => crate::UnsupportedHistoryReason::MalformedRequiredField,
        2 => crate::UnsupportedHistoryReason::UnsupportedRequiredPayload,
        3 => crate::UnsupportedHistoryReason::HostedImageGeneration,
        4 => crate::UnsupportedHistoryReason::ImpossibleLifecycle,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "unsupported history reason",
                tag,
            });
        }
    })
}

pub(crate) fn enc_provider_item_disposition(
    e: &mut Encoder,
    value: crate::ProviderItemDisposition,
) {
    match value {
        crate::ProviderItemDisposition::CorrelatedUserInput {
            content,
            marker_count,
        } => {
            e.u8(0);
            enc_content_ref(e, content);
            e.u64(marker_count);
        }
        crate::ProviderItemDisposition::CanonicalText => e.u8(1),
        crate::ProviderItemDisposition::ActivityOnly => e.u8(2),
        crate::ProviderItemDisposition::GeneratedMedia { resource_id } => {
            e.u8(3);
            enc_resource(e, resource_id);
        }
        crate::ProviderItemDisposition::Unsupported(reason) => {
            e.u8(4);
            enc_unsupported_history_reason(e, reason);
        }
    }
}

pub(crate) fn dec_provider_item_disposition(
    d: &mut Decoder<'_>,
) -> Result<crate::ProviderItemDisposition, CodecError> {
    Ok(match d.u8()? {
        0 => crate::ProviderItemDisposition::CorrelatedUserInput {
            content: dec_content_ref(d)?,
            marker_count: d.u64()?,
        },
        1 => crate::ProviderItemDisposition::CanonicalText,
        2 => crate::ProviderItemDisposition::ActivityOnly,
        3 => crate::ProviderItemDisposition::GeneratedMedia {
            resource_id: dec_resource(d)?,
        },
        4 => crate::ProviderItemDisposition::Unsupported(dec_unsupported_history_reason(d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider item disposition",
                tag,
            });
        }
    })
}

pub(crate) fn enc_turn_incomplete_reason(e: &mut Encoder, value: crate::TurnIncompleteReason) {
    match value {
        crate::TurnIncompleteReason::StreamLost => e.u8(0),
        crate::TurnIncompleteReason::AuthorityLost => e.u8(1),
        crate::TurnIncompleteReason::WorkerStopped => e.u8(2),
        crate::TurnIncompleteReason::CompletionMismatch => e.u8(3),
        crate::TurnIncompleteReason::ItemAuditFailed => e.u8(4),
        crate::TurnIncompleteReason::UnsupportedHistory(reason) => {
            e.u8(5);
            enc_unsupported_history_reason(e, reason);
        }
    }
}

pub(crate) fn dec_turn_incomplete_reason(
    d: &mut Decoder<'_>,
) -> Result<crate::TurnIncompleteReason, CodecError> {
    Ok(match d.u8()? {
        0 => crate::TurnIncompleteReason::StreamLost,
        1 => crate::TurnIncompleteReason::AuthorityLost,
        2 => crate::TurnIncompleteReason::WorkerStopped,
        3 => crate::TurnIncompleteReason::CompletionMismatch,
        4 => crate::TurnIncompleteReason::ItemAuditFailed,
        5 => crate::TurnIncompleteReason::UnsupportedHistory(dec_unsupported_history_reason(d)?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "turn incomplete reason",
                tag,
            });
        }
    })
}

pub(crate) fn enc_turn_end_status(e: &mut Encoder, value: crate::TurnEndStatus) {
    enc_turn_terminal_outcome(e, value.outcome());
    enc_opt(e, value.incomplete_reason(), enc_turn_incomplete_reason);
}

pub(crate) fn dec_turn_end_status(d: &mut Decoder<'_>) -> Result<crate::TurnEndStatus, CodecError> {
    crate::TurnEndStatus::new(
        dec_turn_terminal_outcome(d)?,
        dec_opt(d, "turn incomplete reason", dec_turn_incomplete_reason)?,
    )
    .map_err(|source| invalid("turn end status", source))
}

pub(crate) fn enc_projection_lifecycle(e: &mut Encoder, value: crate::ProjectionLifecycle) {
    e.u8(match value {
        crate::ProjectionLifecycle::Current => 0,
        crate::ProjectionLifecycle::Stale => 1,
    });
}

pub(crate) fn dec_projection_lifecycle(
    d: &mut Decoder<'_>,
) -> Result<crate::ProjectionLifecycle, CodecError> {
    match d.u8()? {
        0 => Ok(crate::ProjectionLifecycle::Current),
        1 => Ok(crate::ProjectionLifecycle::Stale),
        tag => Err(CodecError::InvalidTag {
            kind: "projection lifecycle",
            tag,
        }),
    }
}

pub(crate) fn enc_projection_format(e: &mut Encoder, value: crate::ProjectionFormatVersion) {
    e.u8(match value {
        crate::ProjectionFormatVersion::V1 => 1,
    });
}

pub(crate) fn dec_projection_format(
    d: &mut Decoder<'_>,
) -> Result<crate::ProjectionFormatVersion, CodecError> {
    match d.u8()? {
        1 => Ok(crate::ProjectionFormatVersion::V1),
        tag => Err(CodecError::InvalidTag {
            kind: "projection format",
            tag,
        }),
    }
}

pub(crate) fn enc_projection_source_range(e: &mut Encoder, value: crate::ProjectionSourceRange) {
    e.u64(value.start());
    e.u64(value.end());
}

pub(crate) fn dec_projection_source_range(
    d: &mut Decoder<'_>,
    kind: &'static str,
) -> Result<crate::ProjectionSourceRange, CodecError> {
    crate::ProjectionSourceRange::new(d.u64()?, d.u64()?).map_err(|source| invalid(kind, source))
}

pub(crate) fn enc_markdown_block_kind(e: &mut Encoder, value: crate::MarkdownBlockKind) {
    match value {
        crate::MarkdownBlockKind::Paragraph => e.u8(0),
        crate::MarkdownBlockKind::Heading(level) => {
            e.u8(1);
            e.u8(level);
        }
        crate::MarkdownBlockKind::BlockQuote => e.u8(2),
        crate::MarkdownBlockKind::List => e.u8(3),
        crate::MarkdownBlockKind::ThematicBreak => e.u8(4),
        crate::MarkdownBlockKind::FencedCode => e.u8(5),
        crate::MarkdownBlockKind::Table => e.u8(6),
        crate::MarkdownBlockKind::Fallback => e.u8(7),
    }
}

pub(crate) fn dec_markdown_block_kind(
    d: &mut Decoder<'_>,
) -> Result<crate::MarkdownBlockKind, CodecError> {
    let value = match d.u8()? {
        0 => crate::MarkdownBlockKind::Paragraph,
        1 => crate::MarkdownBlockKind::Heading(d.u8()?),
        2 => crate::MarkdownBlockKind::BlockQuote,
        3 => crate::MarkdownBlockKind::List,
        4 => crate::MarkdownBlockKind::ThematicBreak,
        5 => crate::MarkdownBlockKind::FencedCode,
        6 => crate::MarkdownBlockKind::Table,
        7 => crate::MarkdownBlockKind::Fallback,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "Markdown block kind",
                tag,
            });
        }
    };
    value
        .validate()
        .map_err(|source| invalid("Markdown block kind", source))
}

pub(crate) fn enc_pending_steering(e: &mut Encoder, value: &crate::PendingSteeringTargetProof) {
    enc_binding_rev(e, value.binding_revision());
    enc_snapshot(e, value.snapshot_id());
    enc_turn(e, value.active_turn_id());
    enc_external(e, value.cas_thread_id().as_str());
}

pub(crate) fn dec_pending_steering(
    d: &mut Decoder<'_>,
) -> Result<crate::PendingSteeringTargetProof, CodecError> {
    Ok(crate::PendingSteeringTargetProof::new(
        dec_binding_rev(d)?,
        dec_snapshot(d)?,
        dec_turn(d)?,
        dec_cas_thread(d)?,
    ))
}

pub(crate) fn enc_steering_target(e: &mut Encoder, value: &crate::SteeringTargetProof) {
    enc_pending_steering(e, value.pending());
    enc_external(e, value.cas_turn_id().as_str());
}

pub(crate) fn dec_steering_target(
    d: &mut Decoder<'_>,
) -> Result<crate::SteeringTargetProof, CodecError> {
    Ok(crate::SteeringTargetProof::new(
        dec_pending_steering(d)?,
        dec_cas_turn(d)?,
    ))
}

pub(crate) fn enc_next_turn_reason(e: &mut Encoder, value: crate::NextTurnReason) {
    e.u8(match value {
        crate::NextTurnReason::PendingTurn => 0,
        crate::NextTurnReason::Compaction => 1,
        crate::NextTurnReason::Stop => 2,
        crate::NextTurnReason::SteeringRejected => 3,
        crate::NextTurnReason::WorkerCapacity => 4,
        crate::NextTurnReason::ProjectionLost => 5,
    });
}

pub(crate) fn dec_next_turn_reason(
    d: &mut Decoder<'_>,
) -> Result<crate::NextTurnReason, CodecError> {
    match d.u8()? {
        0 => Ok(crate::NextTurnReason::PendingTurn),
        1 => Ok(crate::NextTurnReason::Compaction),
        2 => Ok(crate::NextTurnReason::Stop),
        3 => Ok(crate::NextTurnReason::SteeringRejected),
        4 => Ok(crate::NextTurnReason::WorkerCapacity),
        5 => Ok(crate::NextTurnReason::ProjectionLost),
        tag => Err(CodecError::InvalidTag {
            kind: "next-turn reason",
            tag,
        }),
    }
}

pub(crate) fn enc_accepted_disposition(e: &mut Encoder, value: &crate::AcceptedInputDisposition) {
    match value {
        crate::AcceptedInputDisposition::AwaitingSteering(target) => {
            e.u8(0);
            enc_pending_steering(e, target);
        }
        crate::AcceptedInputDisposition::SteerActiveTurn(target) => {
            e.u8(1);
            enc_steering_target(e, target);
        }
        crate::AcceptedInputDisposition::NextTurn(reason) => {
            e.u8(2);
            enc_next_turn_reason(e, *reason);
        }
    }
}

pub(crate) fn dec_accepted_disposition(
    d: &mut Decoder<'_>,
) -> Result<crate::AcceptedInputDisposition, CodecError> {
    match d.u8()? {
        0 => dec_pending_steering(d).map(crate::AcceptedInputDisposition::AwaitingSteering),
        1 => dec_steering_target(d).map(crate::AcceptedInputDisposition::SteerActiveTurn),
        2 => dec_next_turn_reason(d).map(crate::AcceptedInputDisposition::NextTurn),
        tag => Err(CodecError::InvalidTag {
            kind: "accepted-input disposition",
            tag,
        }),
    }
}

pub(crate) fn enc_input_gate_state(e: &mut Encoder, value: &crate::InputGateState) {
    match value {
        crate::InputGateState::Idle => e.u8(0),
        crate::InputGateState::PendingTurn(turn) => {
            e.u8(1);
            enc_turn(e, *turn);
        }
        crate::InputGateState::AwaitingSteering(target) => {
            e.u8(2);
            enc_pending_steering(e, target);
        }
        crate::InputGateState::Steerable(target) => {
            e.u8(3);
            enc_steering_target(e, target);
        }
        crate::InputGateState::Compacting(turn) => {
            e.u8(4);
            enc_turn(e, *turn);
        }
        crate::InputGateState::Stopping(target) => {
            e.u8(5);
            enc_steering_target(e, target);
        }
    }
}

pub(crate) fn dec_input_gate_state(
    d: &mut Decoder<'_>,
) -> Result<crate::InputGateState, CodecError> {
    match d.u8()? {
        0 => Ok(crate::InputGateState::Idle),
        1 => dec_turn(d).map(crate::InputGateState::PendingTurn),
        2 => dec_pending_steering(d).map(crate::InputGateState::AwaitingSteering),
        3 => dec_steering_target(d).map(crate::InputGateState::Steerable),
        4 => dec_turn(d).map(crate::InputGateState::Compacting),
        5 => dec_steering_target(d).map(crate::InputGateState::Stopping),
        tag => Err(CodecError::InvalidTag {
            kind: "input-gate state",
            tag,
        }),
    }
}

pub(crate) fn enc_accepted_lifecycle(e: &mut Encoder, value: crate::AcceptedInputLifecycle) {
    e.u8(match value {
        crate::AcceptedInputLifecycle::Admitted => 0,
        crate::AcceptedInputLifecycle::Delivering => 1,
        crate::AcceptedInputLifecycle::Delivered => 2,
        crate::AcceptedInputLifecycle::Retryable => 3,
        crate::AcceptedInputLifecycle::Failed => 4,
        crate::AcceptedInputLifecycle::DeliveryUnknown => 5,
    });
}

pub(crate) fn dec_accepted_lifecycle(
    d: &mut Decoder<'_>,
) -> Result<crate::AcceptedInputLifecycle, CodecError> {
    Ok(match d.u8()? {
        0 => crate::AcceptedInputLifecycle::Admitted,
        1 => crate::AcceptedInputLifecycle::Delivering,
        2 => crate::AcceptedInputLifecycle::Delivered,
        3 => crate::AcceptedInputLifecycle::Retryable,
        4 => crate::AcceptedInputLifecycle::Failed,
        5 => crate::AcceptedInputLifecycle::DeliveryUnknown,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "accepted-input lifecycle",
                tag,
            });
        }
    })
}

pub(crate) fn enc_cas_turn_source(e: &mut Encoder, value: &crate::CasTurnSource) {
    enc_external(e, value.thread_id().as_str());
    enc_external(e, value.turn_id().as_str());
}

pub(crate) fn dec_cas_turn_source(d: &mut Decoder<'_>) -> Result<crate::CasTurnSource, CodecError> {
    Ok(crate::CasTurnSource::new(
        dec_cas_thread(d)?,
        dec_cas_turn(d)?,
    ))
}

pub(crate) fn enc_cas_item_source(e: &mut Encoder, value: &crate::CasItemSource) {
    enc_cas_turn_source(e, value.turn());
    enc_external(e, value.item_id().as_str());
}

pub(crate) fn dec_cas_item_source(d: &mut Decoder<'_>) -> Result<crate::CasItemSource, CodecError> {
    Ok(crate::CasItemSource::new(
        dec_cas_turn_source(d)?,
        dec_cas_item(d)?,
    ))
}

pub(crate) fn enc_canonical_payload(e: &mut Encoder, value: &crate::CanonicalItemPayload) {
    match value {
        crate::CanonicalItemPayload::UserInput {
            content,
            marker_count,
        } => {
            e.u8(0);
            enc_content_ref(e, *content);
            e.u64(*marker_count);
        }
        crate::CanonicalItemPayload::Text(content) => {
            e.u8(1);
            enc_content_ref(e, *content);
        }
        crate::CanonicalItemPayload::Activity => e.u8(2),
        crate::CanonicalItemPayload::GeneratedMedia(resource) => {
            e.u8(3);
            enc_resource(e, *resource);
        }
        crate::CanonicalItemPayload::Unsupported(reason) => {
            e.u8(4);
            enc_unsupported_history_reason(e, *reason);
        }
    }
}

pub(crate) fn dec_canonical_payload(
    d: &mut Decoder<'_>,
) -> Result<crate::CanonicalItemPayload, CodecError> {
    match d.u8()? {
        0 => Ok(crate::CanonicalItemPayload::user_input(
            dec_content_ref(d)?,
            d.u64()?,
        )),
        1 => Ok(crate::CanonicalItemPayload::text(dec_content_ref(d)?)),
        2 => Ok(crate::CanonicalItemPayload::activity()),
        3 => Ok(crate::CanonicalItemPayload::generated_media(dec_resource(
            d,
        )?)),
        4 => Ok(crate::CanonicalItemPayload::unsupported(
            dec_unsupported_history_reason(d)?,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "canonical item payload",
            tag,
        }),
    }
}

pub(crate) fn enc_resource_kind(e: &mut Encoder, value: crate::ResourceKind) {
    e.u8(match value {
        crate::ResourceKind::Code => 0,
        crate::ResourceKind::Table => 1,
        crate::ResourceKind::Image => 2,
        crate::ResourceKind::Attachment => 3,
        crate::ResourceKind::Log => 4,
        crate::ResourceKind::Other => 5,
    });
}

pub(crate) fn dec_resource_kind(d: &mut Decoder<'_>) -> Result<crate::ResourceKind, CodecError> {
    Ok(match d.u8()? {
        0 => crate::ResourceKind::Code,
        1 => crate::ResourceKind::Table,
        2 => crate::ResourceKind::Image,
        3 => crate::ResourceKind::Attachment,
        4 => crate::ResourceKind::Log,
        5 => crate::ResourceKind::Other,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "resource kind",
                tag,
            });
        }
    })
}

pub(crate) fn enc_context_envelope(e: &mut Encoder, value: &crate::DiscussionContextEnvelope) {
    let descriptor = value.descriptor();
    e.u8(1);
    let source = descriptor.source();
    enc_thread(e, source.thread_id());
    enc_turn(e, source.turn_id());
    enc_item(e, source.item_id());
    enc_projection(e, source.projection_id());
    enc_projection_rev(e, source.projection_revision());
    e.u64(source.range().start());
    e.u64(source.range().end());
    e.fixed32(descriptor.digest().as_bytes());
    enc_timestamp(e, descriptor.created_at());
    e.text(value.text().as_str());
}

pub(crate) fn dec_context_envelope(
    d: &mut Decoder<'_>,
) -> Result<crate::DiscussionContextEnvelope, CodecError> {
    match d.u8()? {
        1 => {}
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "context version",
                tag,
            });
        }
    }
    let source = crate::DiscussionContextSource::new(
        dec_thread(d)?,
        dec_turn(d)?,
        dec_item(d)?,
        dec_projection(d)?,
        dec_projection_rev(d)?,
        crate::DiscussionContextRange::new(d.u64()?, d.u64()?)
            .map_err(|source| invalid("context range", source))?,
    );
    let stored_digest = DiscussionContextDigest::from_bytes(d.fixed32()?);
    let created_at = dec_timestamp(d)?;
    let text = crate::DiscussionContextText::new(d.text("discussion context")?)
        .map_err(|source| invalid("discussion context", source))?;
    let envelope = crate::DiscussionContextEnvelope::new(source, text, created_at)
        .map_err(|source| invalid("context envelope", source))?;
    if envelope.descriptor().digest() != stored_digest {
        return Err(CodecError::InvalidLength("context digest"));
    }
    Ok(envelope)
}

pub(crate) fn enc_selected_path(e: &mut Encoder, value: crate::SelectedPathProof) {
    enc_opt(e, value.tail(), enc_turn);
    enc_thread_rev(e, value.thread_revision());
    enc_path_digest(e, value.digest());
}

pub(crate) fn dec_selected_path(
    d: &mut Decoder<'_>,
) -> Result<crate::SelectedPathProof, CodecError> {
    Ok(crate::SelectedPathProof::new(
        dec_opt(d, "selected tail", dec_turn)?,
        dec_thread_rev(d)?,
        dec_path_digest(d)?,
    ))
}

pub(crate) fn enc_represented_prefix(e: &mut Encoder, value: crate::CasRepresentedPrefixProof) {
    enc_opt(e, value.tail(), enc_turn);
    enc_thread_rev(e, value.source_thread_revision());
    enc_path_digest(e, value.digest());
}

pub(crate) fn dec_represented_prefix(
    d: &mut Decoder<'_>,
) -> Result<crate::CasRepresentedPrefixProof, CodecError> {
    Ok(crate::CasRepresentedPrefixProof::new(
        dec_opt(d, "represented-prefix tail", dec_turn)?,
        dec_thread_rev(d)?,
        dec_path_digest(d)?,
    ))
}

pub(crate) fn enc_loaded_generation(e: &mut Encoder, value: CasLoadedSessionGeneration) {
    e.u64(value.process().get());
    e.u64(value.thread().get());
}

pub(crate) fn dec_loaded_generation(
    d: &mut Decoder<'_>,
) -> Result<CasLoadedSessionGeneration, CodecError> {
    Ok(CasLoadedSessionGeneration::new(
        CasProcessGeneration::new(d.u64()?)
            .map_err(|source| invalid("CAS process generation", source))?,
        CasLoadedThreadGeneration::new(d.u64()?)
            .map_err(|source| invalid("loaded-thread generation", source))?,
    ))
}

pub(crate) fn enc_lineage(e: &mut Encoder, value: crate::CasLineageProof) {
    match value {
        crate::CasLineageProof::Native {
            mechanism,
            established_prefix,
        } => {
            e.u8(0);
            e.u8(match mechanism {
                crate::NativeCasLineage::Fresh => 0,
                crate::NativeCasLineage::Continuation => 1,
                crate::NativeCasLineage::Resume => 2,
                crate::NativeCasLineage::Fork => 3,
            });
            enc_represented_prefix(e, established_prefix);
        }
        crate::CasLineageProof::RecoveredInjection(proof) => {
            e.u8(1);
            e.u8(1);
            enc_represented_prefix(e, proof.established_prefix());
            e.fixed32(proof.sequence_digest().as_bytes());
            e.u32(proof.item_count().get());
            e.u64(proof.utf8_bytes().get());
            enc_timestamp(e, proof.completed_at());
            enc_loaded_generation(e, proof.loaded_generation());
        }
    }
}

pub(crate) fn dec_lineage(d: &mut Decoder<'_>) -> Result<crate::CasLineageProof, CodecError> {
    match d.u8()? {
        0 => {
            let mechanism = match d.u8()? {
                0 => crate::NativeCasLineage::Fresh,
                1 => crate::NativeCasLineage::Continuation,
                2 => crate::NativeCasLineage::Resume,
                3 => crate::NativeCasLineage::Fork,
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "native lineage",
                        tag,
                    });
                }
            };
            crate::CasLineageProof::native(mechanism, dec_represented_prefix(d)?)
                .map_err(|source| invalid("native lineage", source))
        }
        1 => {
            match d.u8()? {
                1 => {}
                tag => {
                    return Err(CodecError::InvalidTag {
                        kind: "recovery projection version",
                        tag,
                    });
                }
            };
            let proof = crate::RecoveredInjectionProof::new(
                crate::RecoveryProjectionVersion::V1,
                dec_represented_prefix(d)?,
                RecoveryItemSequenceDigest::from_bytes(d.fixed32()?),
                crate::RecoveryItemCount::new(u64::from(d.u32()?))
                    .map_err(|source| invalid("recovery item count", source))?,
                crate::RecoveryUtf8ByteCount::new(d.u64()?)
                    .map_err(|source| invalid("recovery bytes", source))?,
                dec_timestamp(d)?,
                dec_loaded_generation(d)?,
            )
            .map_err(|source| invalid("recovered lineage", source))?;
            Ok(crate::CasLineageProof::recovered(proof))
        }
        tag => Err(CodecError::InvalidTag {
            kind: "CAS lineage",
            tag,
        }),
    }
}

pub(crate) fn enc_execution(e: &mut Encoder, value: &ExecutionBinding) {
    enc_thread_like(e, value.runtime_id().as_bytes());
    enc_thread_like(e, value.root_id().as_bytes());
    match value.root_path().mode() {
        RuntimeMode::Host => e.u8(0),
        RuntimeMode::Wsl(name) => {
            e.u8(1);
            e.text(name.as_str());
        }
    }
    e.u8(match value.root_path().flavor() {
        PathFlavor::Windows => 0,
        PathFlavor::Posix => 1,
    });
    e.text(value.root_path().as_str());
}

fn enc_thread_like(e: &mut Encoder, value: &[u8; 16]) {
    e.fixed16(value);
}

pub(crate) fn dec_execution(d: &mut Decoder<'_>) -> Result<ExecutionBinding, CodecError> {
    let runtime_id = RuntimeId::from_bytes(d.fixed16()?);
    let root_id = RootId::from_bytes(d.fixed16()?);
    let mode = match d.u8()? {
        0 => RuntimeMode::Host,
        1 => RuntimeMode::wsl(d.text("WSL distribution")?)
            .map_err(|source| invalid("runtime mode", source))?,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "runtime mode",
                tag,
            });
        }
    };
    let flavor = match d.u8()? {
        0 => PathFlavor::Windows,
        1 => PathFlavor::Posix,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "path flavor",
                tag,
            });
        }
    };
    let path = RuntimeNativePath::from_admitted(mode, flavor, d.text("runtime path")?)
        .map_err(|source| invalid("runtime path", source))?;
    Ok(ExecutionBinding::new(runtime_id, root_id, path))
}

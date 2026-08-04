use super::*;

pub(crate) fn enc_turn_kind(e: &mut Encoder, value: crate::TurnKind) {
    match value {
        crate::TurnKind::OrdinaryUser => e.u8(0),
        crate::TurnKind::BerylLifecycleContinuation => e.u8(2),
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
        2 => Ok(crate::TurnKind::BerylLifecycleContinuation),
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

pub(crate) fn enc_provider_observation_issue_reason(
    e: &mut Encoder,
    value: crate::ProviderObservationIssueReason,
) {
    e.u8(match value {
        crate::ProviderObservationIssueReason::CompletionOnlyItemStarted => 0,
        crate::ProviderObservationIssueReason::DuplicateItemStart => 1,
        crate::ProviderObservationIssueReason::MissingItemStart => 2,
        crate::ProviderObservationIssueReason::EventAfterCompletion => 3,
        crate::ProviderObservationIssueReason::ItemKindMismatch => 4,
        crate::ProviderObservationIssueReason::CompletionBeforeStart => 5,
    });
}

pub(crate) fn dec_provider_observation_issue_reason(
    d: &mut Decoder<'_>,
) -> Result<crate::ProviderObservationIssueReason, CodecError> {
    Ok(match d.u8()? {
        0 => crate::ProviderObservationIssueReason::CompletionOnlyItemStarted,
        1 => crate::ProviderObservationIssueReason::DuplicateItemStart,
        2 => crate::ProviderObservationIssueReason::MissingItemStart,
        3 => crate::ProviderObservationIssueReason::EventAfterCompletion,
        4 => crate::ProviderObservationIssueReason::ItemKindMismatch,
        5 => crate::ProviderObservationIssueReason::CompletionBeforeStart,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider-observation issue reason",
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

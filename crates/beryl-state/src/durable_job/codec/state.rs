use crate::encoding::{CodecError, Decoder, Encoder};

use super::super::{
    record::failure_state_is_compatible, BranchHandoffCheckpoint, BranchHandoffJobState,
    HandoffFailureEvidence, HandoffFailureKind, ParentCasIdentity, ParentHandoffIdentity,
};

pub(super) fn encode_state(encoder: &mut Encoder, state: &BranchHandoffJobState) {
    match state {
        BranchHandoffJobState::WaitingResolvingTurn => encoder.u8(0),
        BranchHandoffJobState::WaitingParent => encoder.u8(1),
        BranchHandoffJobState::StartingParent { parent } => {
            encoder.u8(2);
            encode_parent(encoder, *parent);
        }
        BranchHandoffJobState::ParentActive { parent, cas } => {
            encoder.u8(3);
            encode_parent(encoder, *parent);
            encode_cas(encoder, cas);
        }
        BranchHandoffJobState::RetryableFailed { resume, evidence } => {
            encoder.u8(4);
            encode_checkpoint(encoder, resume);
            encode_evidence(encoder, evidence);
        }
        BranchHandoffJobState::TerminalFailed {
            stopped_at,
            evidence,
        } => {
            encoder.u8(5);
            encode_checkpoint(encoder, stopped_at);
            encode_evidence(encoder, evidence);
        }
        BranchHandoffJobState::Succeeded { parent, cas } => {
            encoder.u8(6);
            encode_parent(encoder, *parent);
            encode_cas(encoder, cas);
        }
    }
}

pub(super) fn decode_state(decoder: &mut Decoder<'_>) -> Result<BranchHandoffJobState, CodecError> {
    match decoder.u8()? {
        0 => Ok(BranchHandoffJobState::WaitingResolvingTurn),
        1 => Ok(BranchHandoffJobState::WaitingParent),
        2 => Ok(BranchHandoffJobState::StartingParent {
            parent: decode_parent(decoder)?,
        }),
        3 => Ok(BranchHandoffJobState::ParentActive {
            parent: decode_parent(decoder)?,
            cas: decode_cas(decoder)?,
        }),
        4 => {
            let resume = decode_checkpoint(decoder)?;
            let evidence = decode_evidence(decoder)?;
            if !failure_state_is_compatible(
                super::super::BranchHandoffJobLifecycle::RetryableFailed,
                &resume,
                evidence.kind(),
            ) {
                return Err(invalid(
                    "retryable failure kind is incompatible with its retained checkpoint",
                ));
            }
            Ok(BranchHandoffJobState::RetryableFailed { resume, evidence })
        }
        5 => {
            let stopped_at = decode_checkpoint(decoder)?;
            let evidence = decode_evidence(decoder)?;
            if !failure_state_is_compatible(
                super::super::BranchHandoffJobLifecycle::TerminalFailed,
                &stopped_at,
                evidence.kind(),
            ) {
                return Err(invalid(
                    "terminal failure kind is incompatible with its retained checkpoint",
                ));
            }
            Ok(BranchHandoffJobState::TerminalFailed {
                stopped_at,
                evidence,
            })
        }
        6 => Ok(BranchHandoffJobState::Succeeded {
            parent: decode_parent(decoder)?,
            cas: decode_cas(decoder)?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "branch handoff job state",
            tag,
        }),
    }
}

fn encode_checkpoint(encoder: &mut Encoder, checkpoint: &BranchHandoffCheckpoint) {
    match checkpoint {
        BranchHandoffCheckpoint::WaitingResolvingTurn => encoder.u8(0),
        BranchHandoffCheckpoint::WaitingParent => encoder.u8(1),
        BranchHandoffCheckpoint::StartingParent { parent } => {
            encoder.u8(2);
            encode_parent(encoder, *parent);
        }
        BranchHandoffCheckpoint::ParentActive { parent, cas } => {
            encoder.u8(3);
            encode_parent(encoder, *parent);
            encode_cas(encoder, cas);
        }
    }
}

fn decode_checkpoint(decoder: &mut Decoder<'_>) -> Result<BranchHandoffCheckpoint, CodecError> {
    match decoder.u8()? {
        0 => Ok(BranchHandoffCheckpoint::WaitingResolvingTurn),
        1 => Ok(BranchHandoffCheckpoint::WaitingParent),
        2 => Ok(BranchHandoffCheckpoint::StartingParent {
            parent: decode_parent(decoder)?,
        }),
        3 => Ok(BranchHandoffCheckpoint::ParentActive {
            parent: decode_parent(decoder)?,
            cas: decode_cas(decoder)?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "branch handoff retry checkpoint",
            tag,
        }),
    }
}

fn encode_parent(encoder: &mut Encoder, parent: ParentHandoffIdentity) {
    encoder.fixed(parent.accepted_input_id().as_bytes());
    encoder.fixed(parent.turn_id().as_bytes());
}

fn decode_parent(decoder: &mut Decoder<'_>) -> Result<ParentHandoffIdentity, CodecError> {
    Ok(ParentHandoffIdentity::new(
        beryl_model::SyndicAcceptedInputId::from_bytes(decoder.fixed()?),
        beryl_model::SyndicTurnId::from_bytes(decoder.fixed()?),
    ))
}

fn encode_cas(encoder: &mut Encoder, cas: &ParentCasIdentity) {
    encoder.text(cas.thread_id().as_str());
    encoder.text(cas.turn_id().as_str());
}

fn decode_cas(decoder: &mut Decoder<'_>) -> Result<ParentCasIdentity, CodecError> {
    let thread_id = beryl_model::CasThreadId::new(decoder.text("parent CAS thread identity")?)
        .map_err(|source| invalid_value("parent CAS thread identity", source))?;
    let turn_id = beryl_model::CasTurnId::new(decoder.text("parent CAS turn identity")?)
        .map_err(|source| invalid_value("parent CAS turn identity", source))?;
    Ok(ParentCasIdentity::new(thread_id, turn_id))
}

fn encode_evidence(encoder: &mut Encoder, evidence: &HandoffFailureEvidence) {
    encoder.u8(encode_failure_kind(evidence.kind()));
    match evidence.detail() {
        Some(detail) => {
            encoder.u8(1);
            encoder.text(detail);
        }
        None => encoder.u8(0),
    }
}

fn decode_evidence(decoder: &mut Decoder<'_>) -> Result<HandoffFailureEvidence, CodecError> {
    let kind = decode_failure_kind(decoder.u8()?)?;
    let detail = match decoder.u8()? {
        0 => None,
        1 => Some(decoder.text("handoff failure detail")?),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "handoff failure detail option",
                tag,
            });
        }
    };
    HandoffFailureEvidence::new(kind, detail)
        .map_err(|source| invalid_value("handoff failure evidence", source))
}

fn encode_failure_kind(kind: HandoffFailureKind) -> u8 {
    match kind {
        HandoffFailureKind::RuntimeUnavailable => 0,
        HandoffFailureKind::RootUnavailable => 1,
        HandoffFailureKind::CasUnavailable => 2,
        HandoffFailureKind::TransientDeliveryFailure => 3,
        HandoffFailureKind::CasRejectedBeforeAcceptance => 4,
        HandoffFailureKind::InvariantViolation => 5,
        HandoffFailureKind::ParentMissing => 6,
        HandoffFailureKind::UnrecoverablePostAppend => 7,
        HandoffFailureKind::ParentInterrupted => 8,
        HandoffFailureKind::ParentIncomplete => 9,
        HandoffFailureKind::ParentTerminalFailure => 10,
    }
}

fn decode_failure_kind(tag: u8) -> Result<HandoffFailureKind, CodecError> {
    match tag {
        0 => Ok(HandoffFailureKind::RuntimeUnavailable),
        1 => Ok(HandoffFailureKind::RootUnavailable),
        2 => Ok(HandoffFailureKind::CasUnavailable),
        3 => Ok(HandoffFailureKind::TransientDeliveryFailure),
        4 => Ok(HandoffFailureKind::CasRejectedBeforeAcceptance),
        5 => Ok(HandoffFailureKind::InvariantViolation),
        6 => Ok(HandoffFailureKind::ParentMissing),
        7 => Ok(HandoffFailureKind::UnrecoverablePostAppend),
        8 => Ok(HandoffFailureKind::ParentInterrupted),
        9 => Ok(HandoffFailureKind::ParentIncomplete),
        10 => Ok(HandoffFailureKind::ParentTerminalFailure),
        tag => Err(CodecError::InvalidTag {
            kind: "handoff failure kind",
            tag,
        }),
    }
}

fn invalid(message: &'static str) -> CodecError {
    invalid_value(
        "branch handoff job state",
        std::io::Error::new(std::io::ErrorKind::InvalidData, message),
    )
}

fn invalid_value(
    kind: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> CodecError {
    CodecError::InvalidValue {
        kind,
        source: Box::new(source),
    }
}

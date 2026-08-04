use super::*;

fn enc_disposition_source(e: &mut Encoder, value: StopDispositionSource) {
    enc_input_gate_rev(e, value.gate_revision());
    e.u64(value.stop_revision().get());
}

fn dec_disposition_source(d: &mut Decoder<'_>) -> Result<StopDispositionSource, CodecError> {
    Ok(StopDispositionSource::new(
        dec_input_gate_rev(d)?,
        StopOperationRevision::new(d.u64()?)
            .map_err(|source| invalid("stop disposition revision", source))?,
    ))
}

pub(super) fn enc_stop_state(e: &mut Encoder, value: StopOperationState) {
    match value {
        StopOperationState::Admitted => e.u8(0),
        StopOperationState::DispatchClaimed => e.u8(1),
        StopOperationState::SafeReopened(witness) => {
            e.u8(2);
            enc_disposition_source(e, witness.source());
            enc_input_gate_rev(e, witness.successor_gate_revision());
            match witness {
                StopSafeReopenWitness::Ordinary {
                    successor_route, ..
                } => {
                    e.u8(0);
                    enc_route_head(e, successor_route);
                }
                StopSafeReopenWitness::ProviderOperation {
                    source_compaction_revision,
                    successor_compaction_revision,
                    ..
                } => {
                    e.u8(1);
                    e.u64(source_compaction_revision.get());
                    e.u64(successor_compaction_revision.get());
                }
            }
        }
        StopOperationState::MatchingTerminal(witness) => {
            e.u8(3);
            enc_disposition_source(e, witness.source());
            enc_input_gate_rev(e, witness.successor_gate_revision());
            enc_turn_state_rev(e, witness.successor_turn_state_revision());
            match witness {
                StopMatchingTerminalWitness::Ordinary { .. } => e.u8(0),
                StopMatchingTerminalWitness::ProviderOperation {
                    source_compaction_revision,
                    successor_compaction_revision,
                    ..
                } => {
                    e.u8(1);
                    e.u64(source_compaction_revision.get());
                    e.u64(successor_compaction_revision.get());
                }
            }
        }
        StopOperationState::Abandoned(witness) => {
            e.u8(4);
            enc_disposition_source(e, witness.source());
            enc_abandonment_reason(e, witness.reason());
            enc_input_gate_rev(e, witness.successor_gate_revision());
            enc_binding_rev(e, witness.retired_binding_revision());
            enc_turn_state_rev(e, witness.successor_turn_state_revision());
            match witness {
                StopAbandonmentWitness::Ordinary { .. } => e.u8(0),
                StopAbandonmentWitness::ProviderOperation {
                    source_compaction_revision,
                    successor_compaction_revision,
                    ..
                } => {
                    e.u8(1);
                    e.u64(source_compaction_revision.get());
                    e.u64(successor_compaction_revision.get());
                }
            }
        }
    }
}

fn enc_abandonment_reason(e: &mut Encoder, reason: StopAbandonmentReason) {
    e.u8(match reason {
        StopAbandonmentReason::ProviderRejectedBeforeCoreInterrupt => 0,
        StopAbandonmentReason::TargetAuthorityLost => 1,
        StopAbandonmentReason::StartupProcessGenerationLost => 2,
    });
}

fn dec_compaction_revision(
    d: &mut Decoder<'_>,
    kind: &'static str,
) -> Result<CompactionOperationRevision, CodecError> {
    CompactionOperationRevision::new(d.u64()?).map_err(|source| invalid(kind, source))
}

pub(super) fn dec_stop_state(d: &mut Decoder<'_>) -> Result<StopOperationState, CodecError> {
    match d.u8()? {
        0 => Ok(StopOperationState::Admitted),
        1 => Ok(StopOperationState::DispatchClaimed),
        2 => dec_safe_reopened(d),
        3 => dec_matching_terminal(d),
        4 => dec_abandoned(d),
        tag => Err(CodecError::InvalidTag {
            kind: "stop-operation state",
            tag,
        }),
    }
}

fn dec_safe_reopened(d: &mut Decoder<'_>) -> Result<StopOperationState, CodecError> {
    let source = dec_disposition_source(d)?;
    let gate = dec_input_gate_rev(d)?;
    Ok(StopOperationState::SafeReopened(match d.u8()? {
        0 => StopSafeReopenWitness::new(source, gate, dec_route_head(d)?),
        1 => StopSafeReopenWitness::provider_operation(
            source,
            gate,
            dec_compaction_revision(d, "compaction reopen source revision")?,
            dec_compaction_revision(d, "compaction reopen successor revision")?,
        ),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "stop safe-reopen target",
                tag,
            });
        }
    }))
}

fn dec_matching_terminal(d: &mut Decoder<'_>) -> Result<StopOperationState, CodecError> {
    let source = dec_disposition_source(d)?;
    let gate = dec_input_gate_rev(d)?;
    let state = dec_turn_state_rev(d)?;
    Ok(StopOperationState::MatchingTerminal(match d.u8()? {
        0 => StopMatchingTerminalWitness::new(source, gate, state),
        1 => StopMatchingTerminalWitness::provider_operation(
            source,
            gate,
            state,
            dec_compaction_revision(d, "matching-terminal compaction source revision")?,
            dec_compaction_revision(d, "matching-terminal compaction successor revision")?,
        ),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "stop matching-terminal target",
                tag,
            });
        }
    }))
}

fn dec_abandoned(d: &mut Decoder<'_>) -> Result<StopOperationState, CodecError> {
    let source = dec_disposition_source(d)?;
    let reason = match d.u8()? {
        0 => StopAbandonmentReason::ProviderRejectedBeforeCoreInterrupt,
        1 => StopAbandonmentReason::TargetAuthorityLost,
        2 => StopAbandonmentReason::StartupProcessGenerationLost,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "stop abandonment reason",
                tag,
            });
        }
    };
    let gate = dec_input_gate_rev(d)?;
    let binding = dec_binding_rev(d)?;
    let state = dec_turn_state_rev(d)?;
    Ok(StopOperationState::Abandoned(match d.u8()? {
        0 => StopAbandonmentWitness::new(source, reason, gate, binding, state),
        1 => StopAbandonmentWitness::provider_operation(
            source,
            reason,
            gate,
            binding,
            state,
            dec_compaction_revision(d, "abandonment compaction source revision")?,
            dec_compaction_revision(d, "abandonment compaction successor revision")?,
        ),
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "stop abandonment target",
                tag,
            });
        }
    }))
}

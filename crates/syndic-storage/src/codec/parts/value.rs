use super::*;

mod content;
mod projection;
mod provider;
mod turn;

pub(crate) use content::*;
pub(crate) use projection::*;
pub(crate) use provider::*;
pub(crate) use turn::*;

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
        crate::NextTurnReason::ProjectionLost => 5,
        crate::NextTurnReason::TerminalHistory => 6,
        crate::NextTurnReason::UnknownTerminal => 7,
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
        // Tag 4 was the invalid worker-capacity reclassification and stays retired.
        5 => Ok(crate::NextTurnReason::ProjectionLost),
        6 => Ok(crate::NextTurnReason::TerminalHistory),
        7 => Ok(crate::NextTurnReason::UnknownTerminal),
        tag => Err(CodecError::InvalidTag {
            kind: "next-turn reason",
            tag,
        }),
    }
}

pub(crate) fn enc_route_generation(e: &mut Encoder, value: crate::AcceptedRouteGeneration) {
    e.u64(value.get());
}

pub(crate) fn dec_route_generation(
    d: &mut Decoder<'_>,
) -> Result<crate::AcceptedRouteGeneration, CodecError> {
    crate::AcceptedRouteGeneration::new(d.u64()?)
        .map_err(|source| invalid("accepted-route generation", source))
}

pub(crate) fn enc_route_revision(e: &mut Encoder, value: crate::AcceptedRouteRevision) {
    e.u64(value.get());
}

pub(crate) fn dec_route_revision(
    d: &mut Decoder<'_>,
) -> Result<crate::AcceptedRouteRevision, CodecError> {
    crate::AcceptedRouteRevision::new(d.u64()?)
        .map_err(|source| invalid("accepted-route revision", source))
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
            enc_turn(e, *target);
        }
        crate::InputGateState::Steerable(target) => {
            e.u8(3);
            enc_turn(e, *target);
        }
        crate::InputGateState::Compacting {
            turn_id,
            operation_nonce,
        } => {
            e.u8(4);
            enc_turn(e, *turn_id);
            e.fixed16(operation_nonce.as_bytes());
        }
        crate::InputGateState::Stopping {
            turn_id,
            operation_nonce,
        } => {
            e.u8(5);
            enc_turn(e, *turn_id);
            e.fixed16(operation_nonce.as_bytes());
        }
        crate::InputGateState::FinalizingHistory(turn) => {
            e.u8(6);
            enc_turn(e, *turn);
        }
        crate::InputGateState::AwaitingTerminal(turn) => {
            e.u8(7);
            enc_turn(e, *turn);
        }
    }
}

pub(crate) fn dec_input_gate_state(
    d: &mut Decoder<'_>,
) -> Result<crate::InputGateState, CodecError> {
    match d.u8()? {
        0 => Ok(crate::InputGateState::Idle),
        1 => dec_turn(d).map(crate::InputGateState::PendingTurn),
        2 => dec_turn(d).map(crate::InputGateState::AwaitingSteering),
        3 => dec_turn(d).map(crate::InputGateState::Steerable),
        4 => Ok(crate::InputGateState::compacting(
            dec_turn(d)?,
            crate::CompactionOperationNonce::from_bytes(d.fixed16()?),
        )),
        5 => Ok(crate::InputGateState::stopping(
            dec_turn(d)?,
            crate::StopOperationNonce::from_bytes(d.fixed16()?),
        )),
        6 => dec_turn(d).map(crate::InputGateState::FinalizingHistory),
        7 => dec_turn(d).map(crate::InputGateState::AwaitingTerminal),
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
        crate::AcceptedInputLifecycle::Promoted => 6,
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
        6 => crate::AcceptedInputLifecycle::Promoted,
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

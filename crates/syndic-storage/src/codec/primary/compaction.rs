use super::*;

mod settlement;

pub(crate) use settlement::*;

pub(crate) struct CompactionOperationsFamily;
pub(crate) type CompactionOperationsCodec = ExactCodec<CompactionOperationsFamily>;

impl ScanKey for CompactionOperationId {
    fn first() -> Self {
        Self::new(
            SyndicThreadId::from_bytes([0; 16]),
            CompactionOperationNonce::from_bytes([0; 16]),
        )
    }

    fn last() -> Self {
        Self::new(
            SyndicThreadId::from_bytes([u8::MAX; 16]),
            CompactionOperationNonce::from_bytes([u8::MAX; 16]),
        )
    }
}

impl Family for CompactionOperationsFamily {
    type Key = CompactionOperationId;
    type Value = CompactionOperationRecord;
    const NAME: &'static str = "compaction-operations";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 32;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_thread(&mut e, key.thread_id());
        e.fixed16(key.nonce().as_bytes());
        Ok(e.finish())
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, CodecError> {
        let mut d = Decoder::new(encoded);
        let key = CompactionOperationId::new(
            dec_thread(&mut d)?,
            CompactionOperationNonce::from_bytes(d.fixed16()?),
        );
        d.finish()?;
        Ok(key)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_id(&mut e, value.id());
        e.fixed16(value.home_id().as_bytes());
        enc_target(&mut e, value.target());
        e.u64(value.revision().get());
        e.fixed16(value.attempt().as_bytes());
        enc_opt(&mut e, value.dispatch_claim(), |e, claim| {
            e.u64(claim.source_revision().get());
            e.fixed16(claim.attempt().as_bytes());
        });
        enc_opt(&mut e, value.request(), |e, request| {
            e.u64(request.revision().get());
            e.u8(enc_request(request.disposition()));
        });
        enc_opt(&mut e, value.provider_frontier(), |e, sequence| {
            e.u64(sequence.get())
        });
        enc_opt(&mut e, value.status(), |e, status| {
            e.u64(status.sequence().get());
            e.u8(enc_status(status.status()));
        });
        enc_opt(&mut e, value.cas_turn(), |e, turn| {
            e.u64(turn.sequence().get());
            enc_external(e, turn.cas_turn_id().as_str());
        });
        enc_opt(&mut e, value.marker(), |e, marker| {
            e.u64(marker.sequence().get());
            enc_item(e, marker.item_id());
            e.u8(match marker.lifecycle() {
                CompactionMarkerLifecycle::Started => 0,
                CompactionMarkerLifecycle::Completed => 1,
            });
        });
        enc_opt(&mut e, value.terminal(), |e, terminal| {
            e.u64(terminal.sequence().get());
            enc_turn_end_status(e, terminal.status());
            enc_turn_state_rev(e, terminal.turn_state_revision());
        });
        enc_state(&mut e, value.state());
        Ok(e.finish())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(encoded);
        let value = CompactionOperationRecord::new(
            dec_id(&mut d)?,
            BerylHomeId::from_bytes(d.fixed16()?),
            dec_target(&mut d)?,
            compaction_revision(&mut d, "compaction-operation revision")?,
            CompactionAttemptNonce::from_bytes(d.fixed16()?),
            dec_opt(&mut d, "compaction dispatch claim", |d| {
                Ok(CompactionDispatchClaimWitness::new(
                    compaction_revision(d, "compaction claim source revision")?,
                    CompactionAttemptNonce::from_bytes(d.fixed16()?),
                ))
            })?,
            dec_opt(&mut d, "compaction request disposition", |d| {
                Ok(CompactionRequestObservation::new(
                    compaction_revision(d, "compaction request revision")?,
                    dec_request(d.u8()?)?,
                ))
            })?,
            dec_opt(&mut d, "compaction provider frontier", provider_sequence)?,
            dec_opt(&mut d, "compaction status observation", |d| {
                Ok(CompactionStatusObservation::new(
                    provider_sequence(d)?,
                    dec_status(d.u8()?)?,
                ))
            })?,
            dec_opt(&mut d, "compaction CAS-turn observation", |d| {
                Ok(CompactionCasTurnObservation::new(
                    provider_sequence(d)?,
                    dec_cas_turn(d)?,
                ))
            })?,
            dec_opt(&mut d, "compaction marker observation", |d| {
                let sequence = provider_sequence(d)?;
                let item = dec_item(d)?;
                let lifecycle = match d.u8()? {
                    0 => CompactionMarkerLifecycle::Started,
                    1 => CompactionMarkerLifecycle::Completed,
                    tag => {
                        return Err(CodecError::InvalidTag {
                            kind: "compaction marker lifecycle",
                            tag,
                        });
                    }
                };
                Ok(CompactionMarkerObservation::new(sequence, item, lifecycle))
            })?,
            dec_opt(&mut d, "compaction terminal observation", |d| {
                Ok(CompactionTerminalObservation::new(
                    provider_sequence(d)?,
                    dec_turn_end_status(d)?,
                    dec_turn_state_rev(d)?,
                ))
            })?,
            dec_state(&mut d)?,
        )
        .map_err(|source| invalid("compaction operation", source))?;
        d.finish()?;
        Ok(value)
    }
}

pub(super) fn enc_id(e: &mut Encoder, id: CompactionOperationId) {
    enc_thread(e, id.thread_id());
    e.fixed16(id.nonce().as_bytes());
}

pub(super) fn dec_id(d: &mut Decoder<'_>) -> Result<CompactionOperationId, CodecError> {
    Ok(CompactionOperationId::new(
        dec_thread(d)?,
        CompactionOperationNonce::from_bytes(d.fixed16()?),
    ))
}

fn enc_target(e: &mut Encoder, target: &CompactionOperationTarget) {
    enc_thread(e, target.thread_id());
    enc_turn(e, target.turn_id());
    enc_snapshot(e, target.snapshot_id());
    enc_binding_rev(e, target.binding_revision());
    e.fixed16(target.runtime_id().as_bytes());
    enc_loaded_generation(e, target.loaded_generation());
    enc_external(e, target.cas_thread_id().as_str());
}

fn dec_target(d: &mut Decoder<'_>) -> Result<CompactionOperationTarget, CodecError> {
    Ok(CompactionOperationTarget::new(
        dec_thread(d)?,
        dec_turn(d)?,
        dec_snapshot(d)?,
        dec_binding_rev(d)?,
        RuntimeId::from_bytes(d.fixed16()?),
        dec_loaded_generation(d)?,
        dec_cas_thread(d)?,
    ))
}

pub(super) fn compaction_revision(
    d: &mut Decoder<'_>,
    kind: &'static str,
) -> Result<CompactionOperationRevision, CodecError> {
    CompactionOperationRevision::new(d.u64()?).map_err(|source| invalid(kind, source))
}

fn provider_sequence(d: &mut Decoder<'_>) -> Result<CompactionProviderSequence, CodecError> {
    CompactionProviderSequence::new(d.u64()?)
        .map_err(|source| invalid("compaction provider sequence", source))
}

fn enc_request(value: CompactionRequestDisposition) -> u8 {
    match value {
        CompactionRequestDisposition::Accepted => 0,
        CompactionRequestDisposition::RejectedBeforeCore => 1,
        CompactionRequestDisposition::ProvenLocalNondispatch => 2,
        CompactionRequestDisposition::CompletionUnknown => 3,
    }
}

fn dec_request(tag: u8) -> Result<CompactionRequestDisposition, CodecError> {
    match tag {
        0 => Ok(CompactionRequestDisposition::Accepted),
        1 => Ok(CompactionRequestDisposition::RejectedBeforeCore),
        2 => Ok(CompactionRequestDisposition::ProvenLocalNondispatch),
        3 => Ok(CompactionRequestDisposition::CompletionUnknown),
        tag => Err(CodecError::InvalidTag {
            kind: "compaction request disposition",
            tag,
        }),
    }
}

fn enc_status(value: CompactionThreadStatus) -> u8 {
    match value {
        CompactionThreadStatus::Active => 0,
        CompactionThreadStatus::Idle => 1,
        CompactionThreadStatus::SystemError => 2,
    }
}

fn dec_status(tag: u8) -> Result<CompactionThreadStatus, CodecError> {
    match tag {
        0 => Ok(CompactionThreadStatus::Active),
        1 => Ok(CompactionThreadStatus::Idle),
        2 => Ok(CompactionThreadStatus::SystemError),
        tag => Err(CodecError::InvalidTag {
            kind: "compaction thread status",
            tag,
        }),
    }
}

fn enc_state(e: &mut Encoder, state: &CompactionOperationState) {
    match state {
        CompactionOperationState::Admitted => e.u8(0),
        CompactionOperationState::DispatchClaimed => e.u8(1),
        CompactionOperationState::Live => e.u8(2),
        CompactionOperationState::Stopping(nonce) => {
            e.u8(3);
            e.fixed16(nonce.as_bytes());
        }
        CompactionOperationState::Finalizing => e.u8(4),
        CompactionOperationState::Consumed(witness) => {
            e.u8(5);
            e.u64(witness.source_revision().get());
            enc_input_gate_rev(e, witness.successor_gate_revision());
            enc_settlement(e, witness.settlement());
            e.fixed32(witness.receipt_commitment().as_bytes());
        }
    }
}

fn dec_state(d: &mut Decoder<'_>) -> Result<CompactionOperationState, CodecError> {
    match d.u8()? {
        0 => Ok(CompactionOperationState::Admitted),
        1 => Ok(CompactionOperationState::DispatchClaimed),
        2 => Ok(CompactionOperationState::Live),
        3 => Ok(CompactionOperationState::Stopping(
            StopOperationNonce::from_bytes(d.fixed16()?),
        )),
        4 => Ok(CompactionOperationState::Finalizing),
        5 => Ok(CompactionOperationState::Consumed(
            CompactionConsumedWitness::new(
                compaction_revision(d, "compaction consumed source revision")?,
                dec_input_gate_rev(d)?,
                dec_settlement(d)?,
                CompactionSettlementReceiptCommitment::from_bytes(d.fixed32()?),
            ),
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "compaction-operation state",
            tag,
        }),
    }
}

pub(super) fn enc_settlement(e: &mut Encoder, value: &CompactionSettlement) {
    match value {
        CompactionSettlement::CancelledBeforeDispatch => e.u8(0),
        CompactionSettlement::LocalNondispatch => e.u8(1),
        CompactionSettlement::Abandoned(reason) => {
            e.u8(2);
            e.u8(match reason {
                CompactionAbandonmentReason::ProviderRejectedBeforeCore => 0,
                CompactionAbandonmentReason::CompletionUnknown => 1,
                CompactionAbandonmentReason::TargetAuthorityLost => 2,
                CompactionAbandonmentReason::StartupProcessGenerationLost => 3,
                CompactionAbandonmentReason::ProviderProtocolConflict => 4,
            });
        }
        CompactionSettlement::ManualSuccess => e.u8(3),
        CompactionSettlement::ManualFailure => e.u8(4),
        CompactionSettlement::LifecycleUserWorkWon => e.u8(5),
        CompactionSettlement::LifecycleContinuation {
            turn_id,
            item_id,
            content_id,
        } => {
            e.u8(6);
            enc_turn(e, *turn_id);
            enc_item(e, *item_id);
            enc_content(e, *content_id);
        }
    }
}

pub(super) fn dec_settlement(d: &mut Decoder<'_>) -> Result<CompactionSettlement, CodecError> {
    match d.u8()? {
        0 => Ok(CompactionSettlement::CancelledBeforeDispatch),
        1 => Ok(CompactionSettlement::LocalNondispatch),
        2 => Ok(CompactionSettlement::Abandoned(match d.u8()? {
            0 => CompactionAbandonmentReason::ProviderRejectedBeforeCore,
            1 => CompactionAbandonmentReason::CompletionUnknown,
            2 => CompactionAbandonmentReason::TargetAuthorityLost,
            3 => CompactionAbandonmentReason::StartupProcessGenerationLost,
            4 => CompactionAbandonmentReason::ProviderProtocolConflict,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "compaction abandonment reason",
                    tag,
                });
            }
        })),
        3 => Ok(CompactionSettlement::ManualSuccess),
        4 => Ok(CompactionSettlement::ManualFailure),
        5 => Ok(CompactionSettlement::LifecycleUserWorkWon),
        6 => Ok(CompactionSettlement::LifecycleContinuation {
            turn_id: dec_turn(d)?,
            item_id: dec_item(d)?,
            content_id: dec_content(d)?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "compaction settlement",
            tag,
        }),
    }
}

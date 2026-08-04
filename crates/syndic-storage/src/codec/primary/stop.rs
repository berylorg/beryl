use super::*;

mod state;

use state::*;

pub(crate) struct StopOperationsFamily;
pub(crate) type StopOperationsCodec = ExactCodec<StopOperationsFamily>;

impl ScanKey for StopOperationId {
    fn first() -> Self {
        Self::new(
            SyndicThreadId::from_bytes([0; 16]),
            StopOperationNonce::from_bytes([0; 16]),
        )
    }

    fn last() -> Self {
        Self::new(
            SyndicThreadId::from_bytes([u8::MAX; 16]),
            StopOperationNonce::from_bytes([u8::MAX; 16]),
        )
    }
}

impl Family for StopOperationsFamily {
    type Key = StopOperationId;
    type Value = StopOperationRecord;
    const NAME: &'static str = "stop-operations";
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
        let value = StopOperationId::new(
            dec_thread(&mut d)?,
            StopOperationNonce::from_bytes(d.fixed16()?),
        );
        d.finish()?;
        Ok(value)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        Ok(encode_stop_value(value).encoded)
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(encoded);
        let id = dec_stop_id(&mut d)?;
        let target = dec_stop_target(&mut d)?;
        let admission = dec_admission_witness(&mut d)?;
        let revision = StopOperationRevision::new(d.u64()?)
            .map_err(|source| invalid("stop-operation revision", source))?;
        let cause_first_revisions = dec_cause_first_revisions(&mut d)?;
        let dispatch_claim = match d.u8()? {
            0 => None,
            1 => Some(StopDispatchClaimWitness::new(
                StopOperationRevision::new(d.u64()?)
                    .map_err(|source| invalid("stop dispatch-claim source revision", source))?,
                StopAttemptNonce::from_bytes(d.fixed16()?),
            )),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "stop dispatch claim",
                    tag,
                });
            }
        };
        let state = dec_stop_state(&mut d)?;
        let value = StopOperationRecord::new(
            id,
            target,
            admission,
            revision,
            cause_first_revisions,
            dispatch_claim,
            state,
        )
        .map_err(|source| invalid("stop operation", source))?;
        d.finish()?;
        Ok(value)
    }
}

struct EncodedStopValue {
    encoded: Vec<u8>,
    cause_offsets: [usize; 4],
    claim_source_offset: Option<usize>,
}

fn encode_stop_value(value: &StopOperationRecord) -> EncodedStopValue {
    let mut e = Encoder::new();
    enc_stop_id(&mut e, value.id());
    enc_stop_target(&mut e, value.target());
    enc_admission_witness(&mut e, value.admission());
    e.u64(value.revision().get());
    let mut cause_offsets = [0; 4];
    for (index, cause) in StopCause::ALL.into_iter().enumerate() {
        cause_offsets[index] = e.len();
        e.u64(
            value
                .cause_first_revisions()
                .first_revision(cause)
                .map_or(0, StopOperationRevision::get),
        );
    }
    let claim_source_offset = match value.dispatch_claim() {
        None => {
            e.u8(0);
            None
        }
        Some(claim) => {
            e.u8(1);
            let offset = e.len();
            e.u64(claim.source_revision().get());
            e.fixed16(claim.attempt().as_bytes());
            Some(offset)
        }
    };
    enc_stop_state(&mut e, value.state());
    EncodedStopValue {
        encoded: e.finish(),
        cause_offsets,
        claim_source_offset,
    }
}

fn enc_stop_id(e: &mut Encoder, value: StopOperationId) {
    enc_thread(e, value.thread_id());
    e.fixed16(value.nonce().as_bytes());
}

fn dec_stop_id(d: &mut Decoder<'_>) -> Result<StopOperationId, CodecError> {
    Ok(StopOperationId::new(
        dec_thread(d)?,
        StopOperationNonce::from_bytes(d.fixed16()?),
    ))
}

fn enc_stop_target(e: &mut Encoder, value: &StopOperationTarget) {
    enc_thread(e, value.thread_id());
    enc_turn(e, value.turn_id());
    enc_turn_kind(e, value.turn_kind());
    enc_binding_rev(e, value.binding_revision());
    enc_snapshot(e, value.snapshot_id());
    e.fixed16(value.runtime_id().as_bytes());
    enc_loaded_generation(e, value.loaded_generation());
    enc_external(e, value.cas_thread_id().as_str());
    enc_external(e, value.cas_turn_id().as_str());
}

fn dec_stop_target(d: &mut Decoder<'_>) -> Result<StopOperationTarget, CodecError> {
    Ok(StopOperationTarget::new(
        dec_thread(d)?,
        dec_turn(d)?,
        dec_turn_kind(d)?,
        dec_binding_rev(d)?,
        dec_snapshot(d)?,
        RuntimeId::from_bytes(d.fixed16()?),
        dec_loaded_generation(d)?,
        dec_cas_thread(d)?,
        dec_cas_turn(d)?,
    ))
}

fn enc_turn_kind(e: &mut Encoder, value: TurnKind) {
    match value {
        TurnKind::OrdinaryUser => e.u8(0),
        TurnKind::BerylLifecycleContinuation => e.u8(2),
        TurnKind::ProviderOperation(kind) => {
            e.u8(1);
            e.u8(match kind {
                ProviderOperationKind::ContextCompaction => 0,
            });
        }
    }
}

fn dec_turn_kind(d: &mut Decoder<'_>) -> Result<TurnKind, CodecError> {
    match d.u8()? {
        0 => Ok(TurnKind::OrdinaryUser),
        1 => match d.u8()? {
            0 => Ok(TurnKind::ProviderOperation(
                ProviderOperationKind::ContextCompaction,
            )),
            tag => Err(CodecError::InvalidTag {
                kind: "stop provider-operation kind",
                tag,
            }),
        },
        2 => Ok(TurnKind::BerylLifecycleContinuation),
        tag => Err(CodecError::InvalidTag {
            kind: "stop turn kind",
            tag,
        }),
    }
}

fn enc_route_head(e: &mut Encoder, value: AcceptedRouteHeadProof) {
    enc_route_generation(e, value.generation());
    enc_route_revision(e, value.revision());
}

fn dec_route_head(d: &mut Decoder<'_>) -> Result<AcceptedRouteHeadProof, CodecError> {
    Ok(AcceptedRouteHeadProof::new(
        dec_route_generation(d)?,
        dec_route_revision(d)?,
    ))
}

fn enc_admission_witness(e: &mut Encoder, value: StopAdmissionWitness) {
    match value {
        StopAdmissionWitness::Ordinary {
            source_gate_revision,
            source_selected_route,
            successor_gate_revision,
            successor_stopped_route,
        } => {
            e.u8(0);
            enc_input_gate_rev(e, source_gate_revision);
            enc_route_head(e, source_selected_route);
            enc_input_gate_rev(e, successor_gate_revision);
            enc_route_head(e, successor_stopped_route);
        }
        StopAdmissionWitness::ProviderOperation {
            source_gate_revision,
            successor_gate_revision,
            source_compaction_revision,
            successor_compaction_revision,
        } => {
            e.u8(1);
            enc_input_gate_rev(e, source_gate_revision);
            enc_input_gate_rev(e, successor_gate_revision);
            e.u64(source_compaction_revision.get());
            e.u64(successor_compaction_revision.get());
        }
    }
}

fn dec_admission_witness(d: &mut Decoder<'_>) -> Result<StopAdmissionWitness, CodecError> {
    match d.u8()? {
        0 => Ok(StopAdmissionWitness::new(
            dec_input_gate_rev(d)?,
            dec_route_head(d)?,
            dec_input_gate_rev(d)?,
            dec_route_head(d)?,
        )),
        1 => Ok(StopAdmissionWitness::provider_operation(
            dec_input_gate_rev(d)?,
            dec_input_gate_rev(d)?,
            crate::CompactionOperationRevision::new(d.u64()?)
                .map_err(|source| invalid("stop admission source compaction revision", source))?,
            crate::CompactionOperationRevision::new(d.u64()?).map_err(|source| {
                invalid("stop admission successor compaction revision", source)
            })?,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "stop admission witness",
            tag,
        }),
    }
}

fn dec_cause_first_revisions(d: &mut Decoder<'_>) -> Result<StopCauseFirstRevisions, CodecError> {
    let mut revisions = [None; 4];
    for slot in &mut revisions {
        let revision = d.u64()?;
        *slot = if revision == 0 {
            None
        } else {
            Some(
                StopOperationRevision::new(revision)
                    .map_err(|source| invalid("stop cause first-publication revision", source))?,
            )
        };
    }
    StopCauseFirstRevisions::new(revisions[0], revisions[1], revisions[2], revisions[3])
        .map_err(|source| invalid("stop cause first-publication revisions", source))
}

#[cfg(feature = "test-faults")]
pub(crate) enum StopProvenanceCodecCorruption {
    MissingAdmissionCause,
    DuplicateLaterCause,
    GappedLaterCause,
    FutureCause,
    ZeroClaimSource,
    FutureClaimPublication,
}

#[cfg(feature = "test-faults")]
pub(crate) fn stop_provenance_codec_rejects(
    value: &StopOperationRecord,
    corruption: StopProvenanceCodecCorruption,
) -> bool {
    let encoded = encode_stop_value(value);
    let mut bytes = encoded.encoded;
    let write_revision = |bytes: &mut [u8], offset: usize, revision: u64| {
        bytes[offset..offset + 8].copy_from_slice(&revision.to_be_bytes());
    };
    match corruption {
        StopProvenanceCodecCorruption::MissingAdmissionCause => {
            for (cause, offset) in StopCause::ALL.into_iter().zip(encoded.cause_offsets) {
                if value.cause_first_revisions().first_revision(cause)
                    == Some(StopOperationRevision::FIRST)
                {
                    write_revision(&mut bytes, offset, 0);
                }
            }
        }
        StopProvenanceCodecCorruption::DuplicateLaterCause => {
            let later = StopCause::ALL
                .into_iter()
                .zip(encoded.cause_offsets)
                .filter_map(|(cause, offset)| {
                    value
                        .cause_first_revisions()
                        .first_revision(cause)
                        .filter(|revision| *revision != StopOperationRevision::FIRST)
                        .map(|revision| (offset, revision))
                })
                .collect::<Vec<_>>();
            let [(first_offset, first), (second_offset, _), ..] = later.as_slice() else {
                return false;
            };
            let _ = first_offset;
            write_revision(&mut bytes, *second_offset, first.get());
        }
        StopProvenanceCodecCorruption::GappedLaterCause => {
            let Some(offset) = StopCause::ALL
                .into_iter()
                .zip(encoded.cause_offsets)
                .find_map(|(cause, offset)| {
                    value
                        .cause_first_revisions()
                        .first_revision(cause)
                        .filter(|revision| *revision != StopOperationRevision::FIRST)
                        .map(|_| offset)
                })
            else {
                return false;
            };
            write_revision(&mut bytes, offset, 0);
        }
        StopProvenanceCodecCorruption::FutureCause => {
            let Some(offset) = StopCause::ALL
                .into_iter()
                .zip(encoded.cause_offsets)
                .find_map(|(cause, offset)| {
                    value
                        .cause_first_revisions()
                        .first_revision(cause)
                        .map(|_| offset)
                })
            else {
                return false;
            };
            let Some(future) = value.revision().get().checked_add(1) else {
                return false;
            };
            write_revision(&mut bytes, offset, future);
        }
        StopProvenanceCodecCorruption::ZeroClaimSource => {
            let Some(offset) = encoded.claim_source_offset else {
                return false;
            };
            write_revision(&mut bytes, offset, 0);
        }
        StopProvenanceCodecCorruption::FutureClaimPublication => {
            let Some(offset) = encoded.claim_source_offset else {
                return false;
            };
            write_revision(&mut bytes, offset, value.revision().get());
        }
    }
    StopOperationsFamily::decode_value(&bytes).is_err()
}

#[cfg(feature = "test-faults")]
pub(crate) fn old_aggregate_stop_encoding_is_rejected(value: &StopOperationRecord) -> bool {
    let mut e = Encoder::new();
    enc_stop_id(&mut e, value.id());
    enc_stop_target(&mut e, value.target());
    enc_admission_witness(&mut e, value.admission());
    e.u64(value.revision().get());
    e.u8(value.causes().bits());
    match value.attempt() {
        None => e.u8(0),
        Some(attempt) => {
            e.u8(1);
            e.fixed16(attempt.as_bytes());
        }
    }
    enc_stop_state(&mut e, value.state());
    StopOperationsFamily::decode_value(&e.finish()).is_err()
}

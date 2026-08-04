use super::*;

pub(crate) struct AcceptedRouteGenerationsFamily;
pub(crate) type AcceptedRouteGenerationsCodec = ExactCodec<AcceptedRouteGenerationsFamily>;

impl Family for AcceptedRouteGenerationsFamily {
    type Key = ThreadRouteKey;
    type Value = AcceptedRouteGenerationRecord;
    const NAME: &'static str = "accepted-route-generations";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(3);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        ThreadRouteKey::decode(bytes)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_thread(&mut e, value.thread_id());
        enc_route_generation(&mut e, value.generation());
        enc_route_revision(&mut e, value.revision());
        match value.target() {
            AcceptedRouteTarget::AwaitingSteering(target) => {
                e.u8(0);
                enc_pending_steering(&mut e, target);
            }
            AcceptedRouteTarget::Steering(target) => {
                e.u8(1);
                enc_steering_target(&mut e, target);
            }
            AcceptedRouteTarget::NextTurn(reason) => {
                e.u8(2);
                enc_next_turn_reason(&mut e, *reason);
            }
            AcceptedRouteTarget::ProjectionLost(proof) => {
                e.u8(3);
                match proof.prior_target() {
                    AcceptedRouteLostTarget::AwaitingSteering(target) => {
                        e.u8(0);
                        enc_pending_steering(&mut e, target);
                    }
                    AcceptedRouteLostTarget::Steering(target) => {
                        e.u8(1);
                        enc_steering_target(&mut e, target);
                    }
                    AcceptedRouteLostTarget::AwaitingTerminal(target) => {
                        e.u8(2);
                        enc_steering_target(&mut e, target);
                    }
                }
                let abandonment = proof.abandonment();
                enc_binding_rev(&mut e, abandonment.expected_binding_revision());
                enc_input_gate_rev(&mut e, abandonment.expected_gate_revision());
                enc_route_generation(&mut e, abandonment.expected_route().generation());
                enc_route_revision(&mut e, abandonment.expected_route().revision());
                match abandonment.kind() {
                    AcceptedRouteAbandonmentKind::Generic => e.u8(0),
                    AcceptedRouteAbandonmentKind::ExactRejectedInput {
                        input_id,
                        expected_input_revision,
                    } => {
                        e.u8(1);
                        enc_accepted(&mut e, input_id);
                        enc_accepted_rev(&mut e, expected_input_revision);
                    }
                }
                enc_binding_rev(&mut e, proof.retirement_binding_revision());
                enc_snapshot(&mut e, proof.snapshot_id());
                enc_external(&mut e, proof.cas_thread_id().as_str());
            }
            AcceptedRouteTarget::AwaitingTerminal(target) => {
                e.u8(4);
                enc_steering_target(&mut e, target);
            }
        }
        match (value.first_ordinal(), value.last_ordinal()) {
            (None, None) => e.u8(0),
            (Some(first), Some(last)) => {
                e.u8(1);
                enc_accepted_ord(&mut e, first);
                enc_accepted_ord(&mut e, last);
            }
            _ => unreachable!("accepted-route record constructor enforces paired interval bounds"),
        }
        e.u64(value.input_count());
        e.u64(value.ready_retryable_count());
        e.u64(value.delivering_count());
        e.u64(value.next_turn_count());
        e.u64(value.terminal_count());
        e.u64(value.live_logical_utf8_bytes());
        e.u64(value.delivering_logical_utf8_bytes());
        Ok(e.finish())
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(bytes);
        let thread = dec_thread(&mut d)?;
        let generation = dec_route_generation(&mut d)?;
        let revision = dec_route_revision(&mut d)?;
        let target = match d.u8()? {
            0 => AcceptedRouteTarget::AwaitingSteering(dec_pending_steering(&mut d)?),
            1 => AcceptedRouteTarget::Steering(dec_steering_target(&mut d)?),
            2 => AcceptedRouteTarget::NextTurn(dec_next_turn_reason(&mut d)?),
            3 => {
                let prior = match d.u8()? {
                    0 => AcceptedRouteLostTarget::AwaitingSteering(dec_pending_steering(&mut d)?),
                    1 => AcceptedRouteLostTarget::Steering(dec_steering_target(&mut d)?),
                    2 => AcceptedRouteLostTarget::AwaitingTerminal(dec_steering_target(&mut d)?),
                    tag => {
                        return Err(CodecError::InvalidTag {
                            kind: "accepted-route lost target",
                            tag,
                        });
                    }
                };
                let expected_binding_revision = dec_binding_rev(&mut d)?;
                let expected_gate_revision = dec_input_gate_rev(&mut d)?;
                let expected_route = AcceptedRouteHeadProof::new(
                    dec_route_generation(&mut d)?,
                    dec_route_revision(&mut d)?,
                );
                let kind = match d.u8()? {
                    0 => AcceptedRouteAbandonmentKind::Generic,
                    1 => AcceptedRouteAbandonmentKind::ExactRejectedInput {
                        input_id: dec_accepted(&mut d)?,
                        expected_input_revision: dec_accepted_rev(&mut d)?,
                    },
                    tag => {
                        return Err(CodecError::InvalidTag {
                            kind: "accepted-route abandonment",
                            tag,
                        });
                    }
                };
                AcceptedRouteTarget::ProjectionLost(AcceptedRouteProjectionLostProof::new(
                    prior,
                    AcceptedRouteAbandonmentProof::new(
                        expected_binding_revision,
                        expected_gate_revision,
                        expected_route,
                        kind,
                    ),
                    dec_binding_rev(&mut d)?,
                    dec_snapshot(&mut d)?,
                    dec_cas_thread(&mut d)?,
                ))
            }
            4 => AcceptedRouteTarget::AwaitingTerminal(dec_steering_target(&mut d)?),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "accepted-route target",
                    tag,
                });
            }
        };
        let (first, last) = match d.u8()? {
            0 => (None, None),
            1 => (
                Some(dec_accepted_ord(&mut d)?),
                Some(dec_accepted_ord(&mut d)?),
            ),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "accepted-route interval",
                    tag,
                });
            }
        };
        let value = AcceptedRouteGenerationRecord::new(
            thread,
            generation,
            revision,
            target,
            first,
            last,
            d.u64()?,
            d.u64()?,
            d.u64()?,
            d.u64()?,
            d.u64()?,
            d.u64()?,
            d.u64()?,
        )
        .map_err(|source| invalid("accepted-route generation", source))?;
        d.finish()?;
        Ok(value)
    }
}

pub(crate) struct AcceptedNextSourcesFamily;
pub(crate) type AcceptedNextSourcesCodec = ExactCodec<AcceptedNextSourcesFamily>;

impl Family for AcceptedNextSourcesFamily {
    type Key = ThreadRouteKey;
    type Value = AcceptedNextSourceRecord;
    const NAME: &'static str = "accepted-next-sources";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        ThreadRouteKey::decode(bytes)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_thread(&mut e, value.thread_id());
        enc_route_generation(&mut e, value.generation());
        enc_route_revision(&mut e, value.generation_revision());
        enc_accepted_ord(&mut e, value.first_ordinal());
        enc_accepted_ord(&mut e, value.last_ordinal());
        Ok(e.finish())
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(bytes);
        let value = AcceptedNextSourceRecord::new(
            dec_thread(&mut d)?,
            dec_route_generation(&mut d)?,
            dec_route_revision(&mut d)?,
            dec_accepted_ord(&mut d)?,
            dec_accepted_ord(&mut d)?,
        );
        d.finish()?;
        Ok(value)
    }
}

pub(crate) struct AcceptedReadySourcesFamily;
pub(crate) type AcceptedReadySourcesCodec = ExactCodec<AcceptedReadySourcesFamily>;

impl Family for AcceptedReadySourcesFamily {
    type Key = ThreadRouteKey;
    type Value = AcceptedReadySourceRecord;
    const NAME: &'static str = "accepted-ready-sources";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }

    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        ThreadRouteKey::decode(bytes)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_thread(&mut e, value.thread_id());
        enc_input_gate_rev(&mut e, value.gate_revision());
        enc_route_generation(&mut e, value.generation());
        enc_route_revision(&mut e, value.generation_revision());
        enc_accepted_ord(&mut e, value.first_ordinal());
        enc_accepted_ord(&mut e, value.last_ordinal());
        Ok(e.finish())
    }

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(bytes);
        let value = AcceptedReadySourceRecord::new(
            dec_thread(&mut d)?,
            dec_input_gate_rev(&mut d)?,
            dec_route_generation(&mut d)?,
            dec_route_revision(&mut d)?,
            dec_accepted_ord(&mut d)?,
            dec_accepted_ord(&mut d)?,
        );
        d.finish()?;
        Ok(value)
    }
}

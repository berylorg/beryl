use super::*;

pub(crate) struct AcceptedRouteGenerationHeadsFamily;
pub(crate) type AcceptedRouteGenerationHeadsCodec = ExactCodec<AcceptedRouteGenerationHeadsFamily>;

impl Family for AcceptedRouteGenerationHeadsFamily {
    type Key = SyndicThreadId;
    type Value = AcceptedRouteGenerationHeadRecord;
    const NAME: &'static str = "accepted-route-generation-heads";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.as_bytes().to_vec())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        key16(bytes, "accepted-route head key", SyndicThreadId::from_bytes)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_thread(&mut e, value.thread_id());
        enc_route_generation(&mut e, value.proof().generation());
        enc_route_revision(&mut e, value.proof().revision());
        Ok(e.finish())
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(bytes);
        let value = AcceptedRouteGenerationHeadRecord::new(
            dec_thread(&mut d)?,
            AcceptedRouteHeadProof::new(dec_route_generation(&mut d)?, dec_route_revision(&mut d)?),
        );
        d.finish()?;
        Ok(value)
    }
}

pub(crate) struct AcceptedRouteLeavesFamily;
pub(crate) type AcceptedRouteLeavesCodec = ExactCodec<AcceptedRouteLeavesFamily>;

impl Family for AcceptedRouteLeavesFamily {
    type Key = SyndicAcceptedInputId;
    type Value = AcceptedRouteLeafRecord;
    const NAME: &'static str = "accepted-route-leaves";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(4);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.as_bytes().to_vec())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        key16(
            bytes,
            "accepted-route leaf key",
            SyndicAcceptedInputId::from_bytes,
        )
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut e = Encoder::new();
        enc_accepted(&mut e, value.input_id());
        enc_thread(&mut e, value.thread_id());
        enc_route_generation(&mut e, value.generation());
        enc_accepted_ord(&mut e, value.ordinal());
        enc_accepted_rev(&mut e, value.revision());
        match value.state() {
            AcceptedRouteLeafState::Routed => e.u8(0),
            AcceptedRouteLeafState::NextTurn(reason) => {
                e.u8(1);
                enc_next_turn_reason(&mut e, reason);
            }
        }
        enc_accepted_lifecycle(&mut e, value.lifecycle());
        match value.last_transition() {
            None => e.u8(0),
            Some(proof) => {
                e.u8(1);
                enc_input_gate_rev(&mut e, proof.expected_gate_revision());
                enc_route_generation(&mut e, proof.expected_route().generation());
                enc_route_revision(&mut e, proof.expected_route().revision());
                enc_accepted_rev(&mut e, proof.expected_input_revision());
                enc_leaf_transition_kind(&mut e, proof.kind());
            }
        }
        match value.promotion() {
            None => e.u8(0),
            Some(proof) => {
                e.u8(1);
                enc_input_gate_rev(&mut e, proof.expected_gate_revision());
                enc_route_generation(&mut e, proof.expected_route().generation());
                enc_route_revision(&mut e, proof.expected_route().revision());
                enc_accepted_rev(&mut e, proof.expected_input_revision());
                enc_turn(&mut e, proof.successor_turn_id());
                enc_item(&mut e, proof.successor_item_id());
                enc_timestamp(&mut e, proof.promoted_at());
            }
        }
        Ok(e.finish())
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut d = Decoder::new(bytes);
        let input_id = dec_accepted(&mut d)?;
        let thread_id = dec_thread(&mut d)?;
        let generation = dec_route_generation(&mut d)?;
        let ordinal = dec_accepted_ord(&mut d)?;
        let revision = dec_accepted_rev(&mut d)?;
        let state = match d.u8()? {
            0 => AcceptedRouteLeafState::Routed,
            1 => AcceptedRouteLeafState::NextTurn(dec_next_turn_reason(&mut d)?),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "accepted-route leaf state",
                    tag,
                });
            }
        };
        let lifecycle = dec_accepted_lifecycle(&mut d)?;
        let transition = match d.u8()? {
            0 => None,
            1 => {
                let expected_gate_revision = dec_input_gate_rev(&mut d)?;
                let expected_route = AcceptedRouteHeadProof::new(
                    dec_route_generation(&mut d)?,
                    dec_route_revision(&mut d)?,
                );
                let expected_input_revision = dec_accepted_rev(&mut d)?;
                let kind = dec_leaf_transition_kind(&mut d)?;
                Some(AcceptedRouteLeafTransitionProof::new(
                    expected_gate_revision,
                    expected_route,
                    expected_input_revision,
                    kind,
                ))
            }
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "accepted-route leaf transition presence",
                    tag,
                });
            }
        };
        let promotion = match d.u8()? {
            0 => None,
            1 => Some(AcceptedInputPromotionProof::new(
                dec_input_gate_rev(&mut d)?,
                AcceptedRouteHeadProof::new(
                    dec_route_generation(&mut d)?,
                    dec_route_revision(&mut d)?,
                ),
                dec_accepted_rev(&mut d)?,
                dec_turn(&mut d)?,
                dec_item(&mut d)?,
                dec_timestamp(&mut d)?,
            )),
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "accepted-route leaf promotion presence",
                    tag,
                });
            }
        };
        d.finish()?;
        let leaf = AcceptedRouteLeafRecord::new(
            input_id, thread_id, generation, ordinal, revision, state, lifecycle,
        );
        let leaf = match transition {
            Some(proof) => leaf.with_transition_proof(proof),
            None => leaf,
        };
        Ok(match promotion {
            Some(proof) => leaf.with_promotion_proof(proof),
            None => leaf,
        })
    }
}

fn enc_leaf_transition_kind(e: &mut Encoder, value: AcceptedRouteLeafTransitionKind) {
    e.u8(match value {
        AcceptedRouteLeafTransitionKind::Begin => 0,
        AcceptedRouteLeafTransitionKind::Retry => 1,
        AcceptedRouteLeafTransitionKind::Complete => 2,
        AcceptedRouteLeafTransitionKind::SteeringRejected => 3,
        AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection => 5,
    });
}

fn dec_leaf_transition_kind(
    d: &mut Decoder<'_>,
) -> Result<AcceptedRouteLeafTransitionKind, CodecError> {
    match d.u8()? {
        0 => Ok(AcceptedRouteLeafTransitionKind::Begin),
        1 => Ok(AcceptedRouteLeafTransitionKind::Retry),
        2 => Ok(AcceptedRouteLeafTransitionKind::Complete),
        3 => Ok(AcceptedRouteLeafTransitionKind::SteeringRejected),
        // Tag 4 was the invalid worker-capacity reclassification and stays retired.
        5 => Ok(AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection),
        tag => Err(CodecError::InvalidTag {
            kind: "accepted-route leaf transition",
            tag,
        }),
    }
}

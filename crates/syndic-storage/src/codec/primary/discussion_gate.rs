use super::*;

pub(super) fn encode_discussion_handoff_gate(
    value: &DiscussionHandoffGateRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut encoder = Encoder::new();
    enc_thread(&mut encoder, value.thread_id());
    encoder.u64(value.revision().get());
    match value.state() {
        DiscussionHandoffGateState::Open => encoder.u8(0),
        DiscussionHandoffGateState::Pending {
            intent_id,
            job_id,
            resolving_turn_id,
        } => {
            encoder.u8(1);
            encoder.fixed16(intent_id.as_bytes());
            encoder.fixed16(job_id.as_bytes());
            enc_turn(&mut encoder, resolving_turn_id);
        }
    }
    Ok(encoder.finish())
}

pub(super) fn decode_discussion_handoff_gate(
    bytes: &[u8],
) -> Result<DiscussionHandoffGateRecord, CodecError> {
    let mut decoder = Decoder::new(bytes);
    let thread = dec_thread(&mut decoder)?;
    let revision = DiscussionHandoffGateRevision::new(decoder.u64()?)
        .map_err(|source| invalid("discussion handoff gate revision", source))?;
    let state = match decoder.u8()? {
        0 => DiscussionHandoffGateState::Open,
        1 => DiscussionHandoffGateState::Pending {
            intent_id: ResolutionIntentId::from_bytes(decoder.fixed16()?),
            job_id: JobId::from_bytes(decoder.fixed16()?),
            resolving_turn_id: dec_turn(&mut decoder)?,
        },
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "discussion handoff gate state",
                tag,
            });
        }
    };
    decoder.finish()?;
    Ok(DiscussionHandoffGateRecord::new(thread, revision, state))
}

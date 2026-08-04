use super::*;

pub(super) fn encode_source_event(value: &SourceEventRecord) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_turn(&mut e, value.turn_id());
    enc_source_seq(&mut e, value.sequence());
    match value.source() {
        Some(source) => {
            e.u8(1);
            enc_cas_turn_source(&mut e, source)
        }
        None => e.u8(0),
    }
    encode_source_event_payload(&mut e, value.payload());
    Ok(e.finish())
}

pub(super) fn decode_source_event(bytes: &[u8]) -> Result<SourceEventRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let turn = dec_turn(&mut d)?;
    let sequence = dec_source_seq(&mut d)?;
    let source = dec_opt(&mut d, "event CAS source", dec_cas_turn_source)?;
    let value =
        SourceEventRecord::new(turn, sequence, source, decode_source_event_payload(&mut d)?)
            .map_err(|source| invalid("source event", source))?;
    d.finish()?;
    Ok(value)
}

fn encode_source_event_payload(e: &mut Encoder, payload: &SourceEventPayload) {
    match payload {
        SourceEventPayload::TurnActivated => e.u8(0),
        SourceEventPayload::ItemFrame { item_id, frame } => {
            e.u8(1);
            enc_item(e, *item_id);
            enc_sealed_provider_frame_reference(e, frame);
        }
        SourceEventPayload::TurnEnded(status) => {
            e.u8(2);
            enc_turn_end_status(e, *status);
        }
        SourceEventPayload::ProviderObservationIssue(issue) => {
            e.u8(3);
            encode_provider_observation_issue(e, issue);
        }
    }
}

fn decode_source_event_payload(d: &mut Decoder<'_>) -> Result<SourceEventPayload, CodecError> {
    match d.u8()? {
        0 => Ok(SourceEventPayload::TurnActivated),
        1 => Ok(SourceEventPayload::ItemFrame {
            item_id: dec_item(d)?,
            frame: Box::new(dec_sealed_provider_frame_reference(d)?),
        }),
        2 => Ok(SourceEventPayload::TurnEnded(dec_turn_end_status(d)?)),
        3 => Ok(SourceEventPayload::ProviderObservationIssue(Box::new(
            decode_provider_observation_issue(d)?,
        ))),
        tag => Err(CodecError::InvalidTag {
            kind: "source-event payload",
            tag,
        }),
    }
}

fn encode_provider_observation_issue(e: &mut Encoder, issue: &ProviderObservationIssue) {
    let reference = issue.observation();
    enc_provider_observation_id(e, reference.identity());
    enc_provider_observation_begin(e, reference.begin());
    e.u64(reference.revision());
    e.u64(reference.chunk_count());
    e.u64(reference.canonical_bytes());
    e.fixed32(reference.digest().as_bytes());
    enc_cas_turn_source(e, issue.source());
    enc_external(e, issue.item_id().as_str());
    enc_provider_item_kind(e, issue.item_kind());
    enc_provider_frame_observation_summary(e, issue.lifecycle());
    enc_provider_observation_issue_reason(e, issue.reason());
}

fn decode_provider_observation_issue(
    d: &mut Decoder<'_>,
) -> Result<ProviderObservationIssue, CodecError> {
    let observation = SealedProviderObservationReference::from_stored_parts(
        dec_provider_observation_id(d)?,
        dec_provider_observation_begin(d)?,
        d.u64()?,
        d.u64()?,
        d.u64()?,
        ProviderObservationDigest::from_bytes(d.fixed32()?),
    )
    .map_err(|source| invalid("sealed provider-observation reference", source))?;
    ProviderObservationIssue::from_stored_parts(
        observation,
        dec_cas_turn_source(d)?,
        dec_cas_item(d)?,
        dec_provider_item_kind(d)?,
        dec_provider_frame_observation_summary(d)?,
        dec_provider_observation_issue_reason(d)?,
    )
    .map_err(|source| invalid("provider-observation issue", source))
}

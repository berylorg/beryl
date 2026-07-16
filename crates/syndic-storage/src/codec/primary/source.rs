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
        SourceEventPayload::ItemStarted {
            item,
            assistant_phase,
        } => {
            e.u8(1);
            encode_item_descriptor(e, item);
            enc_opt(e, *assistant_phase, enc_assistant_phase);
        }
        SourceEventPayload::ItemDelta {
            item_id,
            cas_item_id,
            expected_kind,
            text,
        } => {
            e.u8(2);
            enc_item(e, *item_id);
            enc_external(e, cas_item_id.as_str());
            enc_provider_item_kind(e, *expected_kind);
            e.text(text.as_str());
        }
        SourceEventPayload::ItemCompleted {
            item,
            assistant_phase,
        } => {
            e.u8(3);
            encode_item_descriptor(e, item);
            enc_opt(e, *assistant_phase, enc_assistant_phase);
        }
        SourceEventPayload::TurnEnded(status) => {
            e.u8(4);
            enc_turn_end_status(e, *status);
        }
    }
}

fn encode_item_descriptor(e: &mut Encoder, item: &SourceItemDescriptor) {
    enc_item(e, item.item_id());
    enc_external(e, item.cas_item_id().as_str());
    enc_provider_item_kind(e, item.kind());
    enc_provider_item_disposition(e, item.disposition());
}

fn decode_source_event_payload(d: &mut Decoder<'_>) -> Result<SourceEventPayload, CodecError> {
    match d.u8()? {
        0 => Ok(SourceEventPayload::TurnActivated),
        1 => Ok(SourceEventPayload::ItemStarted {
            item: decode_item_descriptor(d)?,
            assistant_phase: dec_opt(d, "source assistant phase", dec_assistant_phase)?,
        }),
        2 => Ok(SourceEventPayload::ItemDelta {
            item_id: dec_item(d)?,
            cas_item_id: dec_cas_item(d)?,
            expected_kind: dec_provider_item_kind(d)?,
            text: SourceEventText::new(d.text("source-event text")?)
                .map_err(|source| invalid("source-event text", source))?,
        }),
        3 => Ok(SourceEventPayload::ItemCompleted {
            item: decode_item_descriptor(d)?,
            assistant_phase: dec_opt(d, "source assistant phase", dec_assistant_phase)?,
        }),
        4 => Ok(SourceEventPayload::TurnEnded(dec_turn_end_status(d)?)),
        tag => Err(CodecError::InvalidTag {
            kind: "source-event payload",
            tag,
        }),
    }
}

fn decode_item_descriptor(d: &mut Decoder<'_>) -> Result<SourceItemDescriptor, CodecError> {
    SourceItemDescriptor::new(
        dec_item(d)?,
        dec_cas_item(d)?,
        dec_provider_item_kind(d)?,
        dec_provider_item_disposition(d)?,
    )
    .map_err(|source| invalid("source item descriptor", source))
}

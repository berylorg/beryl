use super::*;

pub(in crate::codec::primary) fn encode_canonical_item(
    value: &CanonicalItemRecord,
) -> Result<Vec<u8>, CodecError> {
    let mut e = Encoder::new();
    enc_item(&mut e, value.id());
    enc_turn(&mut e, value.turn_id());
    enc_item_ord(&mut e, value.ordinal());
    enc_projection_rev(&mut e, value.revision());
    enc_opt(&mut e, value.source_event(), enc_source_seq);
    e.u64(value.source_event_count());
    match value.cas_source() {
        Some(source) => {
            e.u8(1);
            enc_cas_item_source(&mut e, source)
        }
        None => e.u8(0),
    }
    enc_provider_item_kind(&mut e, value.provider_kind());
    enc_provider_item_lifecycle(&mut e, value.provider_lifecycle());
    enc_opt(&mut e, value.assistant_phase(), enc_assistant_phase);
    enc_opt(
        &mut e,
        value.provider(),
        enc_sealed_provider_frame_reference,
    );
    enc_opt(
        &mut e,
        value.narrative_completion(),
        enc_provider_narrative_completion_disposition,
    );
    encode_canonical_presentation(&mut e, value.presentation());
    Ok(e.finish())
}

pub(in crate::codec::primary) fn decode_canonical_item(
    bytes: &[u8],
) -> Result<CanonicalItemRecord, CodecError> {
    let mut d = Decoder::new(bytes);
    let id = dec_item(&mut d)?;
    let turn = dec_turn(&mut d)?;
    let ordinal = dec_item_ord(&mut d)?;
    let revision = dec_projection_rev(&mut d)?;
    let event = dec_opt(&mut d, "item source event", dec_source_seq)?;
    let event_count = d.u64()?;
    let cas = dec_opt(&mut d, "item CAS source", dec_cas_item_source)?;
    let stored_kind = dec_provider_item_kind(&mut d)?;
    let stored_lifecycle = dec_provider_item_lifecycle(&mut d)?;
    let phase = dec_opt(&mut d, "canonical assistant phase", dec_assistant_phase)?;
    let provider = dec_opt(
        &mut d,
        "canonical provider frame",
        dec_sealed_provider_frame_reference,
    )?;
    let narrative_completion = dec_opt(
        &mut d,
        "canonical provider narrative completion",
        dec_provider_narrative_completion_disposition,
    )?;
    let presentation = decode_canonical_presentation(&mut d)?;
    let value = match provider {
        Some(provider) => CanonicalItemRecord::with_provider_state(
            id,
            turn,
            ordinal,
            revision,
            event.ok_or_else(|| {
                invalid(
                    "canonical item",
                    SyndicRecordError::InvalidProviderItemLifecycle,
                )
            })?,
            event_count,
            cas.ok_or_else(|| {
                invalid(
                    "canonical item",
                    SyndicRecordError::InvalidProviderItemLifecycle,
                )
            })?,
            phase,
            provider,
            narrative_completion,
            presentation,
        )
        .map_err(|source| invalid("canonical item", source))?,
        None => decode_local_canonical_item(
            id,
            turn,
            ordinal,
            revision,
            event,
            event_count,
            cas,
            phase,
            narrative_completion,
            presentation,
        )?,
    };
    if value.provider_kind() != stored_kind || value.provider_lifecycle() != stored_lifecycle {
        return Err(invalid(
            "canonical item",
            SyndicRecordError::InvalidProviderItemLifecycle,
        ));
    }
    d.finish()?;
    Ok(value)
}

fn encode_canonical_presentation(e: &mut Encoder, value: &CanonicalItemPresentation) {
    match value {
        CanonicalItemPresentation::UserInput {
            content,
            asset_reference_set,
        } => {
            e.u8(0);
            enc_content_ref(e, *content);
            enc_opt(
                e,
                asset_reference_set.as_deref().copied(),
                enc_sealed_asset_reference_set_proof,
            );
        }
        CanonicalItemPresentation::Narrative => e.u8(1),
        CanonicalItemPresentation::Operational => e.u8(2),
        CanonicalItemPresentation::Activity => e.u8(3),
        CanonicalItemPresentation::GeneratedMedia { resource_id } => {
            e.u8(4);
            enc_resource(e, *resource_id);
        }
    }
}

fn decode_canonical_presentation(
    d: &mut Decoder<'_>,
) -> Result<CanonicalItemPresentation, CodecError> {
    match d.u8()? {
        0 => Ok(CanonicalItemPresentation::user_input(
            dec_content_ref(d)?,
            dec_opt(
                d,
                "canonical user asset reference set",
                dec_sealed_asset_reference_set_proof,
            )?,
        )),
        1 => Ok(CanonicalItemPresentation::Narrative),
        2 => Ok(CanonicalItemPresentation::Operational),
        3 => Ok(CanonicalItemPresentation::Activity),
        4 => Ok(CanonicalItemPresentation::GeneratedMedia {
            resource_id: dec_resource(d)?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "canonical item presentation",
            tag,
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_local_canonical_item(
    id: SyndicItemId,
    turn: SyndicTurnId,
    ordinal: TurnItemOrdinal,
    revision: ProjectionRevision,
    event: Option<SourceEventSequence>,
    event_count: u64,
    cas: Option<CasItemSource>,
    phase: Option<AssistantMessagePhase>,
    narrative_completion: Option<ProviderNarrativeCompletionDisposition>,
    presentation: CanonicalItemPresentation,
) -> Result<CanonicalItemRecord, CodecError> {
    if event.is_some()
        || event_count != 0
        || cas.is_some()
        || phase.is_some()
        || narrative_completion.is_some()
    {
        return Err(invalid(
            "canonical item",
            SyndicRecordError::InvalidProviderItemLifecycle,
        ));
    }
    let CanonicalItemPresentation::UserInput {
        content,
        asset_reference_set,
    } = presentation
    else {
        return Err(invalid(
            "canonical item",
            SyndicRecordError::InvalidProviderItemDisposition,
        ));
    };
    Ok(CanonicalItemRecord::local_user_input(
        id,
        turn,
        ordinal,
        revision,
        content,
        asset_reference_set.map(|proof| *proof),
    ))
}

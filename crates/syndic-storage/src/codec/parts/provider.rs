use beryl_model::SyndicContentDigest;

use super::*;

pub(crate) fn enc_provider_frame_ordinal(
    encoder: &mut Encoder,
    value: crate::ProviderFrameOrdinalV1,
) {
    encoder.u64(value.get());
}

pub(crate) fn dec_provider_frame_ordinal(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderFrameOrdinalV1, CodecError> {
    crate::ProviderFrameOrdinalV1::new(decoder.u64()?)
        .map_err(|source| invalid("provider frame ordinal", source))
}

pub(crate) fn enc_provider_narrative_generation(
    encoder: &mut Encoder,
    value: crate::ProviderNarrativeGeneration,
) {
    encoder.u64(value.get());
}

pub(crate) fn dec_provider_narrative_generation(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderNarrativeGeneration, CodecError> {
    crate::ProviderNarrativeGeneration::new(decoder.u64()?)
        .map_err(|source| invalid("provider narrative generation", source))
}

pub(crate) fn enc_provider_lifecycle_timestamp(
    encoder: &mut Encoder,
    value: crate::ProviderLifecycleTimestampMsV1,
) {
    encoder.u64(value.get());
}

pub(crate) fn dec_provider_lifecycle_timestamp(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderLifecycleTimestampMsV1, CodecError> {
    Ok(crate::ProviderLifecycleTimestampMsV1::new(decoder.u64()?))
}

pub(crate) fn enc_provider_frame_history_support(
    encoder: &mut Encoder,
    value: crate::ProviderFrameHistorySupportV1,
) {
    match value {
        crate::ProviderFrameHistorySupportV1::Supported => encoder.u8(0),
        crate::ProviderFrameHistorySupportV1::Unsupported(reason) => {
            encoder.u8(1);
            enc_unsupported_history_reason(encoder, reason);
        }
    }
}

pub(crate) fn dec_provider_frame_history_support(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderFrameHistorySupportV1, CodecError> {
    match decoder.u8()? {
        0 => Ok(crate::ProviderFrameHistorySupportV1::Supported),
        1 => Ok(crate::ProviderFrameHistorySupportV1::Unsupported(
            dec_unsupported_history_reason(decoder)?,
        )),
        tag => Err(CodecError::InvalidTag {
            kind: "provider frame history support",
            tag,
        }),
    }
}

pub(crate) fn enc_provider_item_stream_state(
    encoder: &mut Encoder,
    value: &crate::ProviderItemStreamStateV1,
) {
    enc_external(encoder, value.item_id().as_str());
    enc_provider_item_kind(encoder, value.kind());
    encoder.u64(value.next_ordinal().get());
    enc_opt(encoder, value.started_at(), |encoder, timestamp| {
        enc_provider_lifecycle_timestamp(encoder, timestamp);
    });
    enc_bool(encoder, value.is_complete());
    enc_provider_frame_history_support(encoder, value.history_support());
}

pub(crate) fn dec_provider_item_stream_state(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderItemStreamStateV1, CodecError> {
    crate::ProviderItemStreamStateV1::new(
        dec_cas_item(decoder)?,
        dec_provider_item_kind(decoder)?,
        decoder.u64()?,
        dec_opt(decoder, "provider stream start", |decoder| {
            dec_provider_lifecycle_timestamp(decoder)
        })?,
        dec_bool(decoder, "provider stream completion")?,
        dec_provider_frame_history_support(decoder)?,
    )
    .map_err(|source| invalid("provider-item stream state", source))
}

pub(crate) fn enc_provider_frame_observation_summary(
    encoder: &mut Encoder,
    value: crate::ProviderFrameObservationSummaryV1,
) {
    match value {
        crate::ProviderFrameObservationSummaryV1::Started(timestamp) => {
            encoder.u8(0);
            enc_provider_lifecycle_timestamp(encoder, timestamp);
        }
        crate::ProviderFrameObservationSummaryV1::Delta => encoder.u8(1),
        crate::ProviderFrameObservationSummaryV1::Completed(timestamp) => {
            encoder.u8(2);
            enc_provider_lifecycle_timestamp(encoder, timestamp);
        }
    }
}

pub(crate) fn dec_provider_frame_observation_summary(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderFrameObservationSummaryV1, CodecError> {
    match decoder.u8()? {
        0 => dec_provider_lifecycle_timestamp(decoder)
            .map(crate::ProviderFrameObservationSummaryV1::Started),
        1 => Ok(crate::ProviderFrameObservationSummaryV1::Delta),
        2 => dec_provider_lifecycle_timestamp(decoder)
            .map(crate::ProviderFrameObservationSummaryV1::Completed),
        tag => Err(CodecError::InvalidTag {
            kind: "provider frame observation",
            tag,
        }),
    }
}

pub(crate) fn enc_provider_frame_reference(
    encoder: &mut Encoder,
    value: &crate::ProviderFrameReferenceV1,
) {
    enc_external(encoder, value.item_id().as_str());
    enc_provider_item_kind(encoder, value.item_kind());
    enc_provider_frame_ordinal(encoder, value.ordinal());
    encoder.u64(value.encoded_start());
    encoder.u64(value.encoded_end());
    encoder.fixed32(&value.encoded_digest());
    encoder.u64(value.logical_utf8_bytes());
    encoder.u64(value.text_span_count());
}

pub(crate) fn dec_provider_frame_reference(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderFrameReferenceV1, CodecError> {
    crate::ProviderFrameReferenceV1::new(
        dec_cas_item(decoder)?,
        dec_provider_item_kind(decoder)?,
        dec_provider_frame_ordinal(decoder)?,
        decoder.u64()?,
        decoder.u64()?,
        decoder.fixed32()?,
        decoder.u64()?,
        decoder.u64()?,
    )
    .map_err(|source| invalid("provider frame reference", source))
}

pub(crate) fn enc_provider_narrative_reference(
    encoder: &mut Encoder,
    value: crate::ProviderNarrativeReference,
) {
    enc_content(encoder, value.content_id());
    enc_provider_narrative_generation(encoder, value.generation());
    encoder.u64(value.span_count());
    encoder.u64(value.logical_utf8_bytes());
    encoder.fixed32(&value.chain_digest());
}

pub(crate) fn dec_provider_narrative_reference(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderNarrativeReference, CodecError> {
    crate::ProviderNarrativeReference::new(
        dec_content(decoder)?,
        dec_provider_narrative_generation(decoder)?,
        decoder.u64()?,
        decoder.u64()?,
        decoder.fixed32()?,
    )
    .map_err(|source| invalid("provider narrative reference", source))
}

pub(crate) fn enc_provider_narrative_completion_disposition(
    encoder: &mut Encoder,
    value: crate::ProviderNarrativeCompletionDisposition,
) {
    match value {
        crate::ProviderNarrativeCompletionDisposition::Equal => encoder.u8(0),
        crate::ProviderNarrativeCompletionDisposition::Mismatch { utf8_byte_offset } => {
            encoder.u8(1);
            encoder.u64(utf8_byte_offset);
        }
    }
}

pub(crate) fn dec_provider_narrative_completion_disposition(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderNarrativeCompletionDisposition, CodecError> {
    match decoder.u8()? {
        0 => Ok(crate::ProviderNarrativeCompletionDisposition::Equal),
        1 => Ok(crate::ProviderNarrativeCompletionDisposition::Mismatch {
            utf8_byte_offset: decoder.u64()?,
        }),
        tag => Err(CodecError::InvalidTag {
            kind: "provider narrative completion disposition",
            tag,
        }),
    }
}

pub(crate) fn enc_sealed_provider_frame_reference(
    encoder: &mut Encoder,
    value: &crate::SealedProviderFrameReference,
) {
    enc_content_ref(encoder, value.content());
    enc_provider_frame_reference(encoder, value.frame());
    enc_provider_frame_observation_summary(encoder, value.observation());
    enc_provider_item_stream_state(encoder, value.stream_state());
    enc_opt(encoder, value.narrative(), |encoder, narrative| {
        enc_provider_narrative_reference(encoder, narrative);
    });
}

pub(crate) fn dec_sealed_provider_frame_reference(
    decoder: &mut Decoder<'_>,
) -> Result<crate::SealedProviderFrameReference, CodecError> {
    crate::SealedProviderFrameReference::new(
        dec_content_ref(decoder)?,
        dec_provider_frame_reference(decoder)?,
        dec_provider_frame_observation_summary(decoder)?,
        dec_provider_item_stream_state(decoder)?,
        dec_opt(decoder, "provider narrative", |decoder| {
            dec_provider_narrative_reference(decoder)
        })?,
    )
    .map_err(|source| invalid("sealed provider frame reference", source))
}

pub(crate) fn enc_provider_item_build_revision(
    encoder: &mut Encoder,
    value: crate::ProviderItemBuildRevision,
) {
    encoder.u64(value.get());
}

pub(crate) fn dec_provider_item_build_revision(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderItemBuildRevision, CodecError> {
    crate::ProviderItemBuildRevision::new(decoder.u64()?)
        .map_err(|source| invalid("provider-item build revision", source))
}

pub(crate) fn enc_provider_item_build_lifecycle(
    encoder: &mut Encoder,
    value: crate::ProviderItemBuildLifecycle,
) {
    encoder.u8(match value {
        crate::ProviderItemBuildLifecycle::Staging => 0,
        crate::ProviderItemBuildLifecycle::Sealed => 1,
    });
}

pub(crate) fn dec_provider_item_build_lifecycle(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderItemBuildLifecycle, CodecError> {
    match decoder.u8()? {
        0 => Ok(crate::ProviderItemBuildLifecycle::Staging),
        1 => Ok(crate::ProviderItemBuildLifecycle::Sealed),
        tag => Err(CodecError::InvalidTag {
            kind: "provider-item build lifecycle",
            tag,
        }),
    }
}

fn enc_provider_frame_text_span(encoder: &mut Encoder, value: crate::ProviderFrameTextSpanV1) {
    enc_provider_frame_ordinal(encoder, value.frame_ordinal());
    encoder.u64(value.logical_start());
    encoder.u64(value.logical_end());
    encoder.u64(value.source_start());
    encoder.u64(value.source_end());
    encoder.fixed32(&value.source_digest());
    encoder.u8(match value.role() {
        crate::ProviderLogicalTextRoleV1::Narrative => 0,
        crate::ProviderLogicalTextRoleV1::Operational => 1,
        crate::ProviderLogicalTextRoleV1::Activity => 2,
    });
}

fn dec_provider_frame_text_span(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderFrameTextSpanV1, CodecError> {
    let ordinal = dec_provider_frame_ordinal(decoder)?;
    let logical_start = decoder.u64()?;
    let logical_end = decoder.u64()?;
    let source_start = decoder.u64()?;
    let source_end = decoder.u64()?;
    let source_digest = decoder.fixed32()?;
    let role = match decoder.u8()? {
        0 => crate::ProviderLogicalTextRoleV1::Narrative,
        1 => crate::ProviderLogicalTextRoleV1::Operational,
        2 => crate::ProviderLogicalTextRoleV1::Activity,
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider logical text role",
                tag,
            });
        }
    };
    crate::ProviderFrameTextSpanV1::new(
        ordinal,
        logical_start,
        logical_end,
        source_start,
        source_end,
        source_digest,
        role,
    )
    .map_err(|source| invalid("provider frame text span", source))
}

fn enc_provider_narrative_completion_check(
    encoder: &mut Encoder,
    value: crate::ProviderNarrativeCompletionCheck,
) {
    enc_opt(encoder, value.source(), enc_provider_frame_text_span);
    match value.state() {
        crate::ProviderNarrativeCompletionState::Pending(frontier) => {
            encoder.u8(0);
            encoder.u64(frontier.compared_utf8_bytes());
            encoder.u64(frontier.verified_span_count());
            encoder.fixed32(&frontier.verified_chain_digest());
        }
        crate::ProviderNarrativeCompletionState::Equal => encoder.u8(1),
        crate::ProviderNarrativeCompletionState::Mismatch { utf8_byte_offset } => {
            encoder.u8(2);
            encoder.u64(utf8_byte_offset);
        }
    }
}

fn dec_provider_narrative_completion_check(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderNarrativeCompletionCheck, CodecError> {
    let source = dec_opt(decoder, "provider completion source", |decoder| {
        dec_provider_frame_text_span(decoder)
    })?;
    let state = match decoder.u8()? {
        0 => crate::ProviderNarrativeCompletionState::Pending(
            crate::ProviderNarrativeComparisonFrontier::from_stored_parts(
                decoder.u64()?,
                decoder.u64()?,
                decoder.fixed32()?,
            ),
        ),
        1 => crate::ProviderNarrativeCompletionState::Equal,
        2 => crate::ProviderNarrativeCompletionState::Mismatch {
            utf8_byte_offset: decoder.u64()?,
        },
        tag => {
            return Err(CodecError::InvalidTag {
                kind: "provider narrative completion state",
                tag,
            });
        }
    };
    Ok(crate::ProviderNarrativeCompletionCheck::new(source, state))
}

pub(crate) fn enc_provider_item_build_record(
    encoder: &mut Encoder,
    value: &crate::ProviderItemBuildRecord,
) {
    enc_item(encoder, value.item_id());
    enc_turn(encoder, value.turn_id());
    enc_cas_item_source(encoder, value.source());
    enc_source_seq(encoder, value.source_event());
    enc_provider_item_build_revision(encoder, value.revision());
    enc_opt(encoder, value.prior(), |encoder, prior| {
        enc_sealed_provider_frame_reference(encoder, prior);
    });
    enc_sealed_provider_frame_reference(encoder, value.target());
    encoder.u64(value.staged_chunk_count());
    encoder.u64(value.staged_encoded_bytes());
    encoder.fixed32(value.staged_chain_digest().as_bytes());
    enc_opt(encoder, value.staged_narrative(), |encoder, narrative| {
        enc_provider_narrative_reference(encoder, narrative);
    });
    enc_opt(encoder, value.completion_check(), |encoder, check| {
        enc_provider_narrative_completion_check(encoder, check);
    });
    enc_provider_item_build_lifecycle(encoder, value.lifecycle());
}

pub(crate) fn dec_provider_item_build_record(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderItemBuildRecord, CodecError> {
    crate::ProviderItemBuildRecord::new(
        dec_item(decoder)?,
        dec_turn(decoder)?,
        dec_cas_item_source(decoder)?,
        dec_source_seq(decoder)?,
        dec_provider_item_build_revision(decoder)?,
        dec_opt(decoder, "prior provider frame", |decoder| {
            dec_sealed_provider_frame_reference(decoder)
        })?,
        dec_sealed_provider_frame_reference(decoder)?,
        decoder.u64()?,
        decoder.u64()?,
        SyndicContentDigest::from_bytes(decoder.fixed32()?),
        dec_opt(decoder, "staged provider narrative", |decoder| {
            dec_provider_narrative_reference(decoder)
        })?,
        dec_opt(decoder, "provider narrative completion check", |decoder| {
            dec_provider_narrative_completion_check(decoder)
        })?,
        dec_provider_item_build_lifecycle(decoder)?,
    )
    .map_err(|source| invalid("provider-item build record", source))
}

pub(crate) fn enc_provider_narrative_span_record(
    encoder: &mut Encoder,
    value: &crate::ProviderNarrativeSpanRecord,
) {
    enc_content(encoder, value.content_id());
    enc_provider_narrative_generation(encoder, value.generation());
    encoder.u64(value.logical_start());
    encoder.u64(value.logical_end());
    enc_provider_frame_ordinal(encoder, value.frame_ordinal());
    encoder.fixed32(&value.frame_encoded_digest());
    encoder.u64(value.source_start());
    encoder.u64(value.source_end());
    encoder.fixed32(&value.source_digest());
    encoder.fixed32(&value.resulting_chain_digest());
}

pub(crate) fn dec_provider_narrative_span_record(
    decoder: &mut Decoder<'_>,
) -> Result<crate::ProviderNarrativeSpanRecord, CodecError> {
    crate::ProviderNarrativeSpanRecord::from_stored_parts(
        dec_content(decoder)?,
        dec_provider_narrative_generation(decoder)?,
        decoder.u64()?,
        decoder.u64()?,
        dec_provider_frame_ordinal(decoder)?,
        decoder.fixed32()?,
        decoder.u64()?,
        decoder.u64()?,
        decoder.fixed32()?,
        decoder.fixed32()?,
    )
    .map_err(|source| invalid("provider narrative span", source))
}

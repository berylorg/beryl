use crate::{
    ActivityChildHandoffFact, ActivityChildHandoffMembership, ActivityCompactFact,
    ActivityItemSource, ActivityQueryEntryRecord, ActivityQuerySource, ActivityQuerySourceRecord,
    ImageLabelOriginSpanRecord,
};

use super::super::{CodecError, ExactCodec, Family, SMALL_MAX, keys::*, parts::*};

pub(crate) struct ImageLabelOriginSpansFamily;
pub(crate) type ImageLabelOriginSpansCodec = ExactCodec<ImageLabelOriginSpansFamily>;

impl Family for ImageLabelOriginSpansFamily {
    type Key = ImageLabelOriginSpanKey;
    type Value = ImageLabelOriginSpanRecord;
    const NAME: &'static str = "image-label-origin-spans";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 24;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        ImageLabelOriginSpanKey::decode(bytes)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_thread(&mut encoder, value.thread_id());
        enc_image_label(&mut encoder, value.start_label());
        enc_image_label(&mut encoder, value.end_label());
        enc_image_label_origin_owner(&mut encoder, value.admitted_owner());
        enc_sealed_asset_reference_set_proof(&mut encoder, value.asset_reference_set());
        Ok(encoder.finish())
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let value = ImageLabelOriginSpanRecord::new(
            dec_thread(&mut decoder)?,
            dec_image_label(&mut decoder)?,
            dec_image_label(&mut decoder)?,
            dec_image_label_origin_owner(&mut decoder)?,
            dec_sealed_asset_reference_set_proof(&mut decoder)?,
        )
        .map_err(|source| super::super::invalid("image-label origin span", source))?;
        decoder.finish()?;
        Ok(value)
    }
}

pub(crate) struct ActivityQueryEntriesFamily;
pub(crate) type ActivityQueryEntriesCodec = ExactCodec<ActivityQueryEntriesFamily>;

impl Family for ActivityQueryEntriesFamily {
    type Key = ActivityQueryEntryKey;
    type Value = ActivityQueryEntryRecord;
    const NAME: &'static str = "activity-query-entries";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 49;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        ActivityQueryEntryKey::decode(bytes)
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_thread(&mut encoder, value.thread_id());
        enc_activity_work_period(&mut encoder, value.work_period());
        encoder.u8(u8::from(value.order().running()));
        enc_timestamp(&mut encoder, value.order().updated_at());
        enc_item(&mut encoder, value.item_id());
        enc_thread(&mut encoder, value.source().thread_id());
        enc_turn(&mut encoder, value.source().turn_id());
        enc_cas_item_source(&mut encoder, value.source().cas_item());
        enc_source_seq(&mut encoder, value.source_event());
        enc_provider_item_kind(&mut encoder, value.provider_kind());
        enc_provider_item_lifecycle(&mut encoder, value.provider_lifecycle());
        enc_opt(
            &mut encoder,
            value.compact_fact().cloned(),
            |encoder, fact| match fact {
                ActivityCompactFact::ChildHandoff(fact) => {
                    encoder.u8(0);
                    enc_thread(encoder, fact.observed_child_thread_id());
                    enc_projection_source_range(encoder, fact.final_answer_range());
                }
            },
        );
        Ok(encoder.finish())
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let thread = dec_thread(&mut decoder)?;
        let work_period = dec_activity_work_period(&mut decoder)?;
        let running = dec_bool(&mut decoder, "activity entry running flag")?;
        let updated_at = dec_timestamp(&mut decoder)?;
        let item = dec_item(&mut decoder)?;
        let item_source = ActivityItemSource::new(
            dec_thread(&mut decoder)?,
            dec_turn(&mut decoder)?,
            item,
            dec_cas_item_source(&mut decoder)?,
        );
        let source_event = dec_source_seq(&mut decoder)?;
        let kind = dec_provider_item_kind(&mut decoder)?;
        let lifecycle = dec_provider_item_lifecycle(&mut decoder)?;
        let fact = dec_opt(
            &mut decoder,
            "activity compact fact",
            |decoder| match decoder.u8()? {
                0 => Ok(ActivityCompactFact::ChildHandoff(
                    ActivityChildHandoffFact::new(
                        dec_thread(decoder)?,
                        dec_projection_source_range(decoder, "activity handoff narrative range")?,
                    ),
                )),
                tag => Err(CodecError::InvalidTag {
                    kind: "activity compact fact",
                    tag,
                }),
            },
        )?;
        let value = ActivityQueryEntryRecord::new(
            thread,
            work_period,
            crate::ActivityQueryOrder::new(running, updated_at, item),
            item_source,
            source_event,
            kind,
            lifecycle,
            fact,
        )
        .map_err(|source| super::super::invalid("activity-query entry", source))?;
        decoder.finish()?;
        Ok(value)
    }
}

pub(crate) fn activity_entry_stored_bytes(
    key: &ActivityQueryEntryKey,
    value: &ActivityQueryEntryRecord,
) -> Result<u64, CodecError> {
    let key_bytes = ActivityQueryEntriesFamily::encode_key(key)?.len();
    let value_bytes = ActivityQueryEntriesFamily::encode_value(value)?
        .len()
        .checked_add(beryl_home_store::RECORD_VERSION_BYTES)
        .ok_or(CodecError::InvalidLength("activity stored bytes"))?;
    u64::try_from(
        key_bytes
            .checked_add(value_bytes)
            .ok_or(CodecError::InvalidLength("activity stored bytes"))?,
    )
    .map_err(|_| CodecError::InvalidLength("activity stored bytes"))
}

pub(crate) struct ActivityQuerySourcesFamily;
pub(crate) type ActivityQuerySourcesCodec = ExactCodec<ActivityQuerySourcesFamily>;

impl Family for ActivityQuerySourcesFamily {
    type Key = ActivityQuerySourceKey;
    type Value = ActivityQuerySourceRecord;
    const NAME: &'static str = "activity-query-sources";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 56;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.encode())
    }

    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        ActivityQuerySourceKey::decode(bytes)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_thread(&mut encoder, value.thread_id());
        enc_activity_work_period(&mut encoder, value.work_period());
        enc_thread(&mut encoder, value.source().thread_id());
        enc_turn(&mut encoder, value.source().turn_id());
        enc_opt(&mut encoder, value.activity_start(), enc_source_seq);
        encoder.u64(value.source_frontier());
        encoder.u8(u8::from(value.active()));
        enc_opt(&mut encoder, value.child_handoff(), |encoder, handoff| {
            enc_item(encoder, handoff.item_id());
            enc_projection_source_range(encoder, handoff.final_answer_range());
        });
        Ok(encoder.finish())
    }

    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let thread = dec_thread(&mut decoder)?;
        let work_period = dec_activity_work_period(&mut decoder)?;
        let source = ActivityQuerySource::new(dec_thread(&mut decoder)?, dec_turn(&mut decoder)?);
        let activity_start = dec_opt(&mut decoder, "activity source start", dec_source_seq)?;
        let source_frontier = decoder.u64()?;
        let active = dec_bool(&mut decoder, "activity source active flag")?;
        let handoff = dec_opt(
            &mut decoder,
            "activity child handoff membership",
            |decoder| {
                Ok(ActivityChildHandoffMembership::new(
                    dec_item(decoder)?,
                    dec_projection_source_range(decoder, "activity handoff membership range")?,
                ))
            },
        )?;
        decoder.finish()?;
        Ok(ActivityQuerySourceRecord::new(
            thread,
            work_period,
            source,
            activity_start,
            source_frontier,
            active,
            handoff,
        ))
    }
}

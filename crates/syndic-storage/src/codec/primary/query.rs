use beryl_model::SyndicThreadId;

use crate::{ActivityQueryHeadRecord, ActivityQuerySource, ProjectionLifecycle, SyndicRecordError};

use super::super::{CodecError, ExactCodec, Family, SMALL_MAX, parts::*};

pub(crate) struct ActivityQueryHeadsFamily;
pub(crate) type ActivityQueryHeadsCodec = ExactCodec<ActivityQueryHeadsFamily>;

impl Family for ActivityQueryHeadsFamily {
    type Key = SyndicThreadId;
    type Value = ActivityQueryHeadRecord;
    const NAME: &'static str = "activity-query-heads";
    const RECORD_VERSION: beryl_home_store::RecordVersion = beryl_home_store::RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = SMALL_MAX;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, CodecError> {
        Ok(key.as_bytes().to_vec())
    }
    fn decode_key(bytes: &[u8]) -> Result<Self::Key, CodecError> {
        let bytes: [u8; 16] = bytes
            .try_into()
            .map_err(|_| CodecError::InvalidLength("activity-query head key"))?;
        Ok(SyndicThreadId::from_bytes(bytes))
    }
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, CodecError> {
        let mut encoder = Encoder::new();
        enc_thread(&mut encoder, value.thread_id());
        enc_activity_work_period(&mut encoder, value.work_period());
        enc_opt(&mut encoder, value.source(), |encoder, source| {
            enc_thread(encoder, source.thread_id());
            enc_turn(encoder, source.turn_id());
        });
        encoder.u8(u8::from(value.source_active()));
        encoder.u64(value.source_frontier());
        enc_activity_query_revision(&mut encoder, value.revision());
        encoder.u64(value.source_count());
        encoder.u64(value.logical_row_count());
        encoder.u64(value.running_row_count());
        encoder.u64(value.completed_row_count());
        encoder.u64(value.completed_stored_bytes());
        enc_opt(
            &mut encoder,
            value.completed_retention_cutoff(),
            |encoder, order| {
                encoder.u8(u8::from(order.running()));
                enc_timestamp(encoder, order.updated_at());
                enc_item(encoder, order.item_id());
            },
        );
        encoder.u8(match value.lifecycle() {
            ProjectionLifecycle::Current => 0,
            ProjectionLifecycle::Stale => 1,
        });
        Ok(encoder.finish())
    }
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let thread = dec_thread(&mut decoder)?;
        let period = dec_activity_work_period(&mut decoder)?;
        let source = dec_opt(&mut decoder, "activity source", |decoder| {
            Ok(ActivityQuerySource::new(
                dec_thread(decoder)?,
                dec_turn(decoder)?,
            ))
        })?;
        let source_active = dec_bool(&mut decoder, "activity source active flag")?;
        let source_frontier = decoder.u64()?;
        let revision = dec_activity_query_revision(&mut decoder)?;
        let source_count = decoder.u64()?;
        let logical_count = decoder.u64()?;
        let running_count = decoder.u64()?;
        let completed_count = decoder.u64()?;
        let completed_bytes = decoder.u64()?;
        let retention = dec_opt(&mut decoder, "activity retention cutoff", |decoder| {
            let running = dec_bool(decoder, "activity cutoff running flag")?;
            let updated_at = dec_timestamp(decoder)?;
            let item = dec_item(decoder)?;
            Ok(crate::ActivityQueryOrder::new(running, updated_at, item))
        })?;
        let lifecycle = match decoder.u8()? {
            0 => ProjectionLifecycle::Current,
            1 => ProjectionLifecycle::Stale,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "activity-query lifecycle",
                    tag,
                });
            }
        };
        let value = ActivityQueryHeadRecord::new(
            thread,
            period,
            source,
            source_active,
            source_frontier,
            revision,
            source_count,
            logical_count,
            running_count,
            completed_count,
            completed_bytes,
            retention,
            lifecycle,
        )
        .map_err(|source: SyndicRecordError| {
            super::super::invalid("activity-query head", source)
        })?;
        decoder.finish()?;
        Ok(value)
    }
}

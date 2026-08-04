use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};

use crate::{ActivityQueryOrder, ActivityWorkPeriod, ImageLabelOrdinal, SyndicTimestamp};

use super::{
    CodecError, Decoder, Encoder, ScanKey, dec_image_label, dec_item, dec_thread, dec_turn,
    enc_image_label, enc_item, enc_thread, enc_turn,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ImageLabelOriginSpanKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) end_label: ImageLabelOrdinal,
}

impl ImageLabelOriginSpanKey {
    pub(crate) fn encode(self) -> Vec<u8> {
        let mut encoder = Encoder::new();
        enc_thread(&mut encoder, self.thread);
        enc_image_label(&mut encoder, self.end_label);
        encoder.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut decoder)?,
            end_label: dec_image_label(&mut decoder)?,
        };
        decoder.finish()?;
        Ok(key)
    }
}

impl ScanKey for ImageLabelOriginSpanKey {
    fn first() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([0; 16]),
            end_label: ImageLabelOrdinal::FIRST,
        }
    }
    fn last() -> Self {
        Self {
            thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            end_label: ImageLabelOrdinal::new(u64::MAX).expect("maximum is nonzero"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ActivityQueryEntryKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) work_period: ActivityWorkPeriod,
    pub(crate) order: ActivityQueryOrder,
}

impl ActivityQueryEntryKey {
    pub(crate) fn first_for_period(
        thread: SyndicThreadId,
        work_period: ActivityWorkPeriod,
    ) -> Self {
        Self {
            thread,
            work_period,
            order: ActivityQueryOrder::new(
                true,
                SyndicTimestamp::from_unix_millis(u64::MAX),
                SyndicItemId::from_bytes([0; 16]),
            ),
        }
    }

    pub(crate) fn first_completed_for_period(
        thread: SyndicThreadId,
        work_period: ActivityWorkPeriod,
    ) -> Self {
        Self {
            thread,
            work_period,
            order: ActivityQueryOrder::new(
                false,
                SyndicTimestamp::from_unix_millis(u64::MAX),
                SyndicItemId::from_bytes([0; 16]),
            ),
        }
    }

    pub(crate) fn last_for_period(thread: SyndicThreadId, work_period: ActivityWorkPeriod) -> Self {
        Self {
            thread,
            work_period,
            order: ActivityQueryOrder::new(
                false,
                SyndicTimestamp::from_unix_millis(0),
                SyndicItemId::from_bytes([u8::MAX; 16]),
            ),
        }
    }

    pub(crate) fn encode(self) -> Vec<u8> {
        let mut encoder = Encoder::new();
        enc_thread(&mut encoder, self.thread);
        encoder.u64(self.work_period.get());
        encoder.u8(if self.order.running() { 0 } else { 1 });
        encoder.u64(u64::MAX - self.order.updated_at().unix_millis());
        enc_item(&mut encoder, self.order.item_id());
        encoder.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let thread = dec_thread(&mut decoder)?;
        let work_period = ActivityWorkPeriod::new(decoder.u64()?)
            .map_err(|source| super::super::invalid("activity work period", source))?;
        let running = match decoder.u8()? {
            0 => true,
            1 => false,
            tag => {
                return Err(CodecError::InvalidTag {
                    kind: "activity running order",
                    tag,
                });
            }
        };
        let updated_at = SyndicTimestamp::from_unix_millis(u64::MAX - decoder.u64()?);
        let item_id = dec_item(&mut decoder)?;
        decoder.finish()?;
        Ok(Self {
            thread,
            work_period,
            order: ActivityQueryOrder::new(running, updated_at, item_id),
        })
    }
}

impl ScanKey for ActivityQueryEntryKey {
    fn first() -> Self {
        Self::first_for_period(
            SyndicThreadId::from_bytes([0; 16]),
            ActivityWorkPeriod::FIRST,
        )
    }
    fn last() -> Self {
        Self::last_for_period(
            SyndicThreadId::from_bytes([u8::MAX; 16]),
            ActivityWorkPeriod::new(u64::MAX).expect("maximum is nonzero"),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ActivityQuerySourceKey {
    pub(crate) thread: SyndicThreadId,
    pub(crate) work_period: ActivityWorkPeriod,
    pub(crate) source_thread: SyndicThreadId,
    pub(crate) source_turn: SyndicTurnId,
}

impl ActivityQuerySourceKey {
    pub(crate) fn first_for_period(
        thread: SyndicThreadId,
        work_period: ActivityWorkPeriod,
    ) -> Self {
        Self {
            thread,
            work_period,
            source_thread: SyndicThreadId::from_bytes([0; 16]),
            source_turn: SyndicTurnId::from_bytes([0; 16]),
        }
    }

    pub(crate) fn last_for_period(thread: SyndicThreadId, work_period: ActivityWorkPeriod) -> Self {
        Self {
            thread,
            work_period,
            source_thread: SyndicThreadId::from_bytes([u8::MAX; 16]),
            source_turn: SyndicTurnId::from_bytes([u8::MAX; 16]),
        }
    }

    pub(crate) fn encode(self) -> Vec<u8> {
        let mut encoder = Encoder::new();
        enc_thread(&mut encoder, self.thread);
        encoder.u64(self.work_period.get());
        enc_thread(&mut encoder, self.source_thread);
        enc_turn(&mut encoder, self.source_turn);
        encoder.finish()
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut decoder = Decoder::new(bytes);
        let key = Self {
            thread: dec_thread(&mut decoder)?,
            work_period: ActivityWorkPeriod::new(decoder.u64()?)
                .map_err(|source| super::super::invalid("activity work period", source))?,
            source_thread: dec_thread(&mut decoder)?,
            source_turn: dec_turn(&mut decoder)?,
        };
        decoder.finish()?;
        Ok(key)
    }
}

impl ScanKey for ActivityQuerySourceKey {
    fn first() -> Self {
        Self::first_for_period(
            SyndicThreadId::from_bytes([0; 16]),
            ActivityWorkPeriod::FIRST,
        )
    }

    fn last() -> Self {
        Self::last_for_period(
            SyndicThreadId::from_bytes([u8::MAX; 16]),
            ActivityWorkPeriod::new(u64::MAX).expect("maximum is nonzero"),
        )
    }
}

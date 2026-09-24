use beryl_home_store::CursorReadLimits;
use beryl_state::{HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES, HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES};
use std::num::NonZeroUsize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandoffScanConfiguration {
    pub handoff_recovery_page_items: usize,
    pub handoff_recovery_page_encoded_bytes: usize,
    pub handoff_job_record_encoded_bytes: usize,
    pub handoff_reconcile_slots: usize,
    pub handoff_ready_job_items: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandoffScanLimits {
    page: CursorReadLimits,
    record_bytes: NonZeroUsize,
    reconcile_slots: NonZeroUsize,
    ready_items: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum HandoffScanLimitError {
    #[error("handoff limit {field} must be positive")]
    Zero { field: &'static str },
    #[error("handoff record byte limit {actual} is below the State envelope {minimum}")]
    RecordTooSmall { actual: usize, minimum: usize },
    #[error(
        "handoff page byte limit {actual} cannot hold its configured record envelope {minimum}"
    )]
    PageTooSmall { actual: usize, minimum: usize },
    #[error("handoff configured record and key bytes overflow")]
    RecordEnvelopeOverflow,
}

impl TryFrom<HandoffScanConfiguration> for HandoffScanLimits {
    type Error = HandoffScanLimitError;
    fn try_from(config: HandoffScanConfiguration) -> Result<Self, Self::Error> {
        let positive =
            |value, field| NonZeroUsize::new(value).ok_or(HandoffScanLimitError::Zero { field });
        let items = positive(
            config.handoff_recovery_page_items,
            "handoff_recovery_page_items",
        )?;
        let bytes = positive(
            config.handoff_recovery_page_encoded_bytes,
            "handoff_recovery_page_encoded_bytes",
        )?;
        let record_bytes = positive(
            config.handoff_job_record_encoded_bytes,
            "handoff_job_record_encoded_bytes",
        )?;
        let reconcile_slots = positive(config.handoff_reconcile_slots, "handoff_reconcile_slots")?;
        let ready_items = positive(config.handoff_ready_job_items, "handoff_ready_job_items")?;
        if record_bytes.get() < HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES {
            return Err(HandoffScanLimitError::RecordTooSmall {
                actual: record_bytes.get(),
                minimum: HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES,
            });
        }
        let minimum = record_bytes
            .get()
            .checked_add(
                HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES - HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES,
            )
            .ok_or(HandoffScanLimitError::RecordEnvelopeOverflow)?;
        if bytes.get() < minimum {
            return Err(HandoffScanLimitError::PageTooSmall {
                actual: bytes.get(),
                minimum,
            });
        }
        Ok(Self {
            page: CursorReadLimits::new(items.get(), bytes.get())
                .expect("validated positive page limits"),
            record_bytes,
            reconcile_slots,
            ready_items,
        })
    }
}

impl HandoffScanLimits {
    pub fn page(self) -> CursorReadLimits {
        self.page
    }
    pub fn job_record_encoded_bytes(self) -> NonZeroUsize {
        self.record_bytes
    }
    pub fn reconcile_slots(self) -> NonZeroUsize {
        self.reconcile_slots
    }
    pub fn ready_job_items(self) -> NonZeroUsize {
        self.ready_items
    }
}

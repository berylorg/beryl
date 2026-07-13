use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};
use beryl_model::SyndicThreadId;

use super::{
    ThreadMetadataDomain, ThreadMetadataValidationError, codec::ThreadMetadataRecordCodec,
};

const VALIDATION_PAGE_ITEMS: usize = 128;
const VALIDATION_PAGE_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn validate(
    reader: &DomainReader<'_, ThreadMetadataDomain>,
) -> Result<(), ThreadMetadataValidationError> {
    let mut after: Option<SyndicThreadId> = None;
    loop {
        let end = SyndicThreadId::from_bytes([u8::MAX; 16]);
        let range = match after {
            Some(after) => CursorRange::after(after, end),
            None => CursorRange::closed(SyndicThreadId::from_bytes([0; 16]), end),
        };
        let page = reader.cursor::<ThreadMetadataRecordCodec>(
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(VALIDATION_PAGE_ITEMS, VALIDATION_PAGE_BYTES)
                .expect("validation limits are nonzero"),
        )?;
        for record in page.records() {
            if record.key() != &record.value().thread_id {
                return Err(ThreadMetadataValidationError::Invariant(
                    "thread metadata key does not match its record identity",
                ));
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|record| *record.key());
        if after.is_none() {
            return Err(ThreadMetadataValidationError::Invariant(
                "bounded metadata cursor reported more without a record",
            ));
        }
    }
}

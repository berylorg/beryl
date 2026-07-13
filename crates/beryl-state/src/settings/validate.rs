use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};

use super::{SettingKey, SettingsDomain, SettingsValidationError, codec::SettingRecordCodec};

const VALIDATION_PAGE_ITEMS: usize = 128;
const VALIDATION_PAGE_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn validate(
    reader: &DomainReader<'_, SettingsDomain>,
) -> Result<(), SettingsValidationError> {
    let mut after = None;
    loop {
        let range = match after {
            Some(after) => CursorRange::after(after, SettingKey::LAST),
            None => CursorRange::closed(SettingKey::FIRST, SettingKey::LAST),
        };
        let page = reader.cursor::<SettingRecordCodec>(
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(VALIDATION_PAGE_ITEMS, VALIDATION_PAGE_BYTES)
                .expect("settings validation limits are nonzero"),
        )?;
        for record in page.records() {
            let value = record.value();
            if record.key() != &value.key {
                return Err(SettingsValidationError::Invariant(
                    "setting key does not match its record identity",
                ));
            }
            if value.schema_version != value.key.schema_version() {
                return Err(SettingsValidationError::Invariant(
                    "setting record carries an unsupported setting schema",
                ));
            }
            if value.key != value.value.key() {
                return Err(SettingsValidationError::Invariant(
                    "setting record key does not match its scalar type",
                ));
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|record| *record.key());
        if after.is_none() {
            return Err(SettingsValidationError::Invariant(
                "bounded settings cursor reported more without a record",
            ));
        }
    }
}

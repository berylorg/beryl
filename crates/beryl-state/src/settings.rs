use std::{error::Error, fmt};

use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError, DomainCallbackSource,
    DomainHandle, DomainRegistrationError, DomainSchemaVersion, HomeStore, KeyspaceSchemaVersion,
    MutationContribution, PointReadLimit, ReadError, RecordFamily, StorageDomain,
};

use crate::{RecordRevision, StatePage};

mod codec;
mod mutation;
mod validate;
mod value;

use codec::SettingRecordCodec;
pub use mutation::{
    ApplySettings, ApplySettingsError, ExpectedSettingRevision, SettingUpdate,
    SettingsMutationError,
};
pub use value::{SettingKey, SettingSchemaVersion, SettingValue, SettingValueError};

pub(crate) const SETTINGS_RECORD_LIMIT: usize = 64 * 1024;
const SETTINGS_FAMILIES: &[RecordFamily<SettingsDomain>] =
    &[RecordFamily::new::<SettingRecordCodec>(
        KeyspaceSchemaVersion::new(1),
    )];

pub(crate) struct SettingsDomain;

impl StorageDomain for SettingsDomain {
    const NAME: &'static str = "beryl-settings";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = SETTINGS_FAMILIES;
    type ValidationError = SettingsValidationError;

    fn validate(
        reader: &beryl_home_store::DomainReader<'_, Self>,
    ) -> Result<(), Self::ValidationError> {
        validate::validate(reader)
    }
}

/// One durable scalar preference with its exact setting and record revisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingRecord {
    pub(crate) key: SettingKey,
    pub(crate) schema_version: SettingSchemaVersion,
    pub(crate) value: SettingValue,
    pub(crate) revision: RecordRevision,
}

impl SettingRecord {
    pub(crate) fn initial(key: SettingKey, value: SettingValue) -> Self {
        debug_assert_eq!(key, value.key());
        Self {
            key,
            schema_version: key.schema_version(),
            value,
            revision: RecordRevision::INITIAL,
        }
    }

    #[must_use]
    pub const fn key(&self) -> SettingKey {
        self.key
    }

    #[must_use]
    pub const fn schema_version(&self) -> SettingSchemaVersion {
        self.schema_version
    }

    #[must_use]
    pub const fn value(&self) -> &SettingValue {
        &self.value
    }

    #[must_use]
    pub const fn revision(&self) -> RecordRevision {
        self.revision
    }
}

/// Opaque typed access to Beryl-owned scalar settings.
#[derive(Clone, Copy)]
pub struct SettingsState {
    handle: DomainHandle<SettingsDomain>,
}

impl SettingsState {
    pub(crate) fn register(store: &mut HomeStore) -> Result<Self, DomainRegistrationError> {
        store
            .register_domain::<SettingsDomain>()
            .map(|handle| Self { handle })
    }

    pub(crate) fn reacquire(
        store: &HomeStore,
    ) -> Result<Self, beryl_home_store::DomainHandleError> {
        store
            .domain_handle::<SettingsDomain>()
            .map(|handle| Self { handle })
    }

    pub fn revision(&self, store: &HomeStore) -> Result<beryl_model::DomainRevision, ReadError> {
        store.domain_revision(self.handle)
    }

    /// Returns this domain's revision from a still-current successful command.
    pub fn committed_revision(
        &self,
        store: &HomeStore,
        receipt: &beryl_home_store::CommitReceipt,
    ) -> Result<Option<beryl_model::DomainRevision>, beryl_home_store::CommitReceiptError> {
        store.receipt_domain_revision(receipt, self.handle)
    }

    pub fn setting(
        &self,
        store: &HomeStore,
        key: SettingKey,
    ) -> Result<Option<SettingRecord>, ReadError> {
        store.read_point::<SettingsDomain, SettingRecordCodec>(self.handle, &key, point_limit())
    }

    pub fn list(
        &self,
        store: &HomeStore,
        after: Option<SettingKey>,
        limits: CursorReadLimits,
    ) -> Result<StatePage<SettingRecord>, ReadError> {
        let start = after.unwrap_or(SettingKey::FIRST);
        let range = if after.is_some() {
            CursorRange::after(start, SettingKey::LAST)
        } else {
            CursorRange::closed(start, SettingKey::LAST)
        };
        let page = store.read_cursor::<SettingsDomain, SettingRecordCodec>(
            self.handle,
            &range,
            CursorDirection::Forward,
            limits,
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let has_more = page.has_more();
        Ok(StatePage {
            records: page
                .into_records()
                .into_iter()
                .map(|record| record.into_parts().1)
                .collect(),
            stored_bytes,
            decoded_bytes,
            has_more,
        })
    }

    /// Seals one atomic settings-window Apply against the exact domain revision.
    #[must_use]
    pub fn apply(
        &self,
        expected_revision: beryl_model::DomainRevision,
        command: ApplySettings,
    ) -> MutationContribution {
        self.handle.contribution(expected_revision, command)
    }
}

fn point_limit() -> PointReadLimit {
    PointReadLimit::new(SETTINGS_RECORD_LIMIT + 4).expect("settings point limit is nonzero")
}

#[derive(Debug)]
pub(crate) enum SettingsValidationError {
    Read(ReadError),
    Invariant(&'static str),
}

impl fmt::Display for SettingsValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for SettingsValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Invariant(_) => None,
        }
    }
}

impl DomainCallbackError for SettingsValidationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for SettingsValidationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

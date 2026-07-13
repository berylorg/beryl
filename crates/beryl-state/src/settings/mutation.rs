use std::{collections::BTreeSet, error::Error, fmt};

use beryl_home_store::{
    DomainMutation, DomainReader, MutationBuildError, MutationBuilder, PointReadLimit, ReadError,
};

use crate::{RecordRevision, ValueError};

use super::{
    SETTINGS_RECORD_LIMIT, SettingKey, SettingRecord, SettingValue, SettingsDomain,
    codec::SettingRecordCodec,
};

/// Exact record state expected by one staged setting update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedSettingRevision {
    Absent,
    Exact(RecordRevision),
}

/// One typed scalar update staged for an atomic settings Apply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingUpdate {
    key: SettingKey,
    expected_revision: ExpectedSettingRevision,
    value: SettingValue,
}

impl SettingUpdate {
    #[must_use]
    pub const fn new(
        key: SettingKey,
        expected_revision: ExpectedSettingRevision,
        value: SettingValue,
    ) -> Self {
        Self {
            key,
            expected_revision,
            value,
        }
    }

    #[must_use]
    pub const fn key(&self) -> SettingKey {
        self.key
    }

    #[must_use]
    pub const fn expected_revision(&self) -> ExpectedSettingRevision {
        self.expected_revision
    }

    #[must_use]
    pub const fn value(&self) -> &SettingValue {
        &self.value
    }
}

/// One nonempty, duplicate-free set of setting updates committed atomically.
pub struct ApplySettings {
    updates: Vec<SettingUpdate>,
}

impl ApplySettings {
    pub fn new(updates: Vec<SettingUpdate>) -> Result<Self, ApplySettingsError> {
        if updates.is_empty() {
            return Err(ApplySettingsError::Empty);
        }
        let mut keys = BTreeSet::new();
        for update in &updates {
            let value_key = update.value.key();
            if update.key != value_key {
                return Err(ApplySettingsError::KeyValueMismatch {
                    key: update.key,
                    value_key,
                });
            }
            if !keys.insert(update.key) {
                return Err(ApplySettingsError::Duplicate { key: update.key });
            }
        }
        Ok(Self { updates })
    }

    #[must_use]
    pub fn updates(&self) -> &[SettingUpdate] {
        &self.updates
    }
}

/// Invalid storage shape for one settings-window Apply request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplySettingsError {
    Empty,
    Duplicate {
        key: SettingKey,
    },
    KeyValueMismatch {
        key: SettingKey,
        value_key: SettingKey,
    },
}

impl fmt::Display for ApplySettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("settings Apply must contain at least one update"),
            Self::Duplicate { key } => write!(
                formatter,
                "settings Apply contains duplicate key `{}`",
                key.stable_id()
            ),
            Self::KeyValueMismatch { key, value_key } => write!(
                formatter,
                "settings Apply key `{}` does not accept scalar `{}`",
                key.stable_id(),
                value_key.stable_id()
            ),
        }
    }
}

impl Error for ApplySettingsError {}

impl DomainMutation<SettingsDomain> for ApplySettings {
    type Error = SettingsMutationError;

    fn validate(&self, reader: &DomainReader<'_, SettingsDomain>) -> Result<(), Self::Error> {
        for update in &self.updates {
            validate_update(reader, update)?;
        }
        Ok(())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SettingsDomain>,
        mutations: &mut MutationBuilder<'_, SettingsDomain>,
    ) -> Result<(), Self::Error> {
        for update in &self.updates {
            let record = match update.expected_revision {
                ExpectedSettingRevision::Absent => {
                    SettingRecord::initial(update.key, update.value.clone())
                }
                ExpectedSettingRevision::Exact(_) => {
                    let mut record = required(reader, update.key)?;
                    record.value = update.value.clone();
                    record.revision = record.revision.checked_next()?;
                    record
                }
            };
            mutations.put::<SettingRecordCodec>(&update.key, &record)?;
        }
        Ok(())
    }
}

fn validate_update(
    reader: &DomainReader<'_, SettingsDomain>,
    update: &SettingUpdate,
) -> Result<(), SettingsMutationError> {
    let current = read(reader, update.key)?;
    match (update.expected_revision, current) {
        (ExpectedSettingRevision::Absent, None) => Ok(()),
        (ExpectedSettingRevision::Absent, Some(_)) => {
            Err(SettingsMutationError::SettingExists { key: update.key })
        }
        (ExpectedSettingRevision::Exact(_), None) => {
            Err(SettingsMutationError::SettingMissing { key: update.key })
        }
        (ExpectedSettingRevision::Exact(expected), Some(record)) if expected != record.revision => {
            Err(SettingsMutationError::RecordRevisionConflict {
                key: update.key,
                expected,
                current: record.revision,
            })
        }
        (ExpectedSettingRevision::Exact(_), Some(_)) => Ok(()),
    }
}

fn read(
    reader: &DomainReader<'_, SettingsDomain>,
    key: SettingKey,
) -> Result<Option<SettingRecord>, SettingsMutationError> {
    reader
        .point::<SettingRecordCodec>(&key, point_limit())
        .map_err(Into::into)
}

fn required(
    reader: &DomainReader<'_, SettingsDomain>,
    key: SettingKey,
) -> Result<SettingRecord, SettingsMutationError> {
    read(reader, key)?.ok_or(SettingsMutationError::SettingMissing { key })
}

fn point_limit() -> PointReadLimit {
    PointReadLimit::new(SETTINGS_RECORD_LIMIT + 4).expect("settings point limit is nonzero")
}

/// Why a revision-checked settings Apply contribution was rejected.
#[derive(Debug)]
pub enum SettingsMutationError {
    Read(ReadError),
    Build(MutationBuildError),
    Value(ValueError),
    SettingExists {
        key: SettingKey,
    },
    SettingMissing {
        key: SettingKey,
    },
    RecordRevisionConflict {
        key: SettingKey,
        expected: RecordRevision,
        current: RecordRevision,
    },
}

impl fmt::Display for SettingsMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::SettingExists { key } => {
                write!(formatter, "setting `{}` already exists", key.stable_id())
            }
            Self::SettingMissing { key } => {
                write!(formatter, "setting `{}` is missing", key.stable_id())
            }
            Self::RecordRevisionConflict {
                key,
                expected,
                current,
            } => write!(
                formatter,
                "setting `{}` record revision conflict: expected {}, current {}",
                key.stable_id(),
                expected.get(),
                current.get()
            ),
        }
    }
}

impl Error for SettingsMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Build(source) => Some(source),
            Self::Value(source) => Some(source),
            _ => None,
        }
    }
}

impl From<ReadError> for SettingsMutationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl From<MutationBuildError> for SettingsMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl From<ValueError> for SettingsMutationError {
    fn from(source: ValueError) -> Self {
        Self::Value(source)
    }
}

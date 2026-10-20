use std::{sync::Arc, time::Duration};

use beryl_home_store::HomeGeneration;
use beryl_model::{AdmittedHostPath, BerylHomeId, DomainRevision};
pub use beryl_state::SettingKey;
use beryl_state::{InstalledThemeId, RecordRevision, SettingRecord, SettingValue};

pub const SETTINGS_KEYS: [SettingKey; 5] = [
    SettingKey::ActiveThemeId,
    SettingKey::ContextCompactionTimeout,
    SettingKey::DraftAutosaveInterval,
    SettingKey::DeveloperInstructions,
    SettingKey::EndTurnSound,
];
pub const DEVELOPER_INSTRUCTIONS_DRAFT_BYTES: usize = 64 * 1024;

pub(super) fn index(key: SettingKey) -> usize {
    SETTINGS_KEYS
        .iter()
        .position(|item| *item == key)
        .expect("closed settings key")
}
pub(super) fn mask(key: SettingKey) -> u8 {
    1 << index(key)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsSourceIdentity {
    pub home_id: BerylHomeId,
    pub home_generation: HomeGeneration,
    pub service_generation: u64,
    pub settings_revision: DomainRevision,
    pub publication: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsDraftBinding {
    pub source: SettingsSourceIdentity,
    pub window_instance: u64,
    pub draft_revision: u64,
    pub route_identity: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsIssue {
    InvalidValue,
    TooLong { maximum: usize, actual: usize },
    RejectedSaved,
    Excluded,
    Unresolved,
    CommandRefused,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsRowAvailability {
    Available,
    RejectedSaved,
    Excluded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppliedSettingOrigin {
    Saved,
    AbsentDefault,
    RejectedSaved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsRow {
    pub key: SettingKey,
    pub value: String,
    pub availability: SettingsRowAvailability,
    pub issue: Option<SettingsIssue>,
}

impl SettingsRow {
    pub fn is_available(&self) -> bool {
        self.availability == SettingsRowAvailability::Available
    }
}

#[derive(Clone, Debug)]
pub struct SettingsBaseline {
    pub(super) identity: SettingsSourceIdentity,
    pub(super) records: [Option<SettingsRecordFacts>; 5],
    pub(super) rows: [SettingsRow; 5],
    pub(super) applied: AppliedSettingsSnapshot,
}

impl SettingsBaseline {
    pub fn identity(&self) -> SettingsSourceIdentity {
        self.identity
    }
    pub fn row(&self, key: SettingKey) -> &SettingsRow {
        &self.rows[index(key)]
    }
    pub fn rows(&self) -> &[SettingsRow; 5] {
        &self.rows
    }
    pub fn applied(&self) -> &AppliedSettingsSnapshot {
        &self.applied
    }
    pub fn record_revision(&self, key: SettingKey) -> Option<RecordRevision> {
        self.records[index(key)]
            .as_ref()
            .map(|record| record.revision)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SettingsRecordFacts {
    pub revision: RecordRevision,
    pub schema: beryl_state::SettingSchemaVersion,
    pub value: SettingValue,
}

impl SettingsRecordFacts {
    pub fn from_record(record: SettingRecord) -> Self {
        Self {
            revision: record.revision(),
            schema: record.schema_version(),
            value: record.value().clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AppliedSettingsSnapshot {
    pub identity: SettingsSourceIdentity,
    pub(super) excluded: u8,
    pub(super) active_theme: Option<InstalledThemeId>,
    pub(super) timeout: Duration,
    pub(super) autosave: Duration,
    pub(super) instructions: Arc<str>,
    pub(super) sound: Option<AdmittedHostPath>,
    pub(super) origins: [AppliedSettingOrigin; 5],
    pub(super) home_binding: Option<beryl_home_store::HomeGenerationIdentity>,
}

impl AppliedSettingsSnapshot {
    pub fn matches_home(&self, home: &beryl_home_store::HomeStore) -> bool {
        self.identity.home_id == home.home_id()
            && self.home_binding.is_some()
            && home.generation_identity().ok() == self.home_binding
    }
    pub fn origin(&self, key: SettingKey) -> AppliedSettingOrigin {
        self.origins[index(key)]
    }
    pub fn is_available(&self, key: SettingKey) -> bool {
        self.excluded & mask(key) == 0
    }
    fn require(&self, key: SettingKey) -> Result<(), SettingsServiceError> {
        if self.is_available(key) {
            Ok(())
        } else {
            Err(SettingsServiceError::Unavailable(key))
        }
    }
    pub fn active_theme(&self) -> Result<Option<&InstalledThemeId>, SettingsServiceError> {
        self.require(SettingKey::ActiveThemeId)?;
        Ok(self.active_theme.as_ref())
    }
    pub fn compaction_timeout(&self) -> Result<Duration, SettingsServiceError> {
        self.require(SettingKey::ContextCompactionTimeout)?;
        Ok(self.timeout)
    }
    pub fn draft_autosave_interval(&self) -> Result<Duration, SettingsServiceError> {
        self.require(SettingKey::DraftAutosaveInterval)?;
        Ok(self.autosave)
    }
    pub fn developer_instructions(&self) -> Result<&str, SettingsServiceError> {
        self.require(SettingKey::DeveloperInstructions)?;
        Ok(&self.instructions)
    }
    pub fn end_turn_sound(&self) -> Result<Option<&AdmittedHostPath>, SettingsServiceError> {
        self.require(SettingKey::EndTurnSound)?;
        Ok(self.sound.as_ref())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsServiceError {
    #[error("Settings source is unavailable")]
    Retired,
    #[error("Settings operation belongs to another source or draft")]
    Stale,
    #[error("Settings source belongs to another home")]
    ForeignHome,
    #[error("Settings command custody is unavailable")]
    Custody,
    #[error("Settings command is already in flight")]
    Busy,
    #[error("Settings key {0:?} is unavailable")]
    Unavailable(SettingKey),
    #[error("Settings draft is invalid for {key:?}: {issue:?}")]
    Invalid {
        key: SettingKey,
        issue: SettingsIssue,
    },
    #[error("Settings draft has no modified values")]
    Clean,
    #[error("Settings command was cancelled before admission")]
    Cancelled,
    #[error("Settings prepared theme is unavailable or obsolete")]
    Theme,
    #[error("Settings source changed during preparation")]
    Changed,
    #[error(transparent)]
    Read(#[from] beryl_home_store::ReadError),
    #[error(transparent)]
    CommandBuild(#[from] beryl_home_store::CommandBuildError),
    #[error(transparent)]
    Reconciliation(#[from] beryl_home_store::ReconciliationFailure),
}

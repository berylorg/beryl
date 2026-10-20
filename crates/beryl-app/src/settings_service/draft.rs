use std::sync::Arc;

use beryl_model::{AdmittedHostPath, PathFlavor};
use beryl_state::{InstalledThemeId, SettingKey, SettingValue};

use super::{SettingsApplyObservation, model::*};

#[derive(Clone, Debug)]
pub enum SettingsDraftValue {
    ActiveThemeId(String),
    ContextCompactionTimeout(String),
    DraftAutosaveInterval(String),
    DeveloperInstructions(String),
    EndTurnSound(String),
}

impl SettingsDraftValue {
    pub fn key(&self) -> SettingKey {
        match self {
            Self::ActiveThemeId(_) => SettingKey::ActiveThemeId,
            Self::ContextCompactionTimeout(_) => SettingKey::ContextCompactionTimeout,
            Self::DraftAutosaveInterval(_) => SettingKey::DraftAutosaveInterval,
            Self::DeveloperInstructions(_) => SettingKey::DeveloperInstructions,
            Self::EndTurnSound(_) => SettingKey::EndTurnSound,
        }
    }
    fn text(self) -> String {
        match self {
            Self::ActiveThemeId(v)
            | Self::ContextCompactionTimeout(v)
            | Self::DraftAutosaveInterval(v)
            | Self::DeveloperInstructions(v)
            | Self::EndTurnSound(v) => v,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SettingsDraft {
    pub(super) baseline: Arc<SettingsBaseline>,
    pub(super) binding: SettingsDraftBinding,
    pub(super) values: [Option<String>; 5],
}

impl SettingsDraft {
    pub fn new(baseline: Arc<SettingsBaseline>, window_instance: u64, route_identity: u64) -> Self {
        Self {
            binding: SettingsDraftBinding {
                source: baseline.identity(),
                window_instance,
                draft_revision: 1,
                route_identity,
            },
            baseline,
            values: std::array::from_fn(|_| None),
        }
    }
    pub fn binding(&self) -> SettingsDraftBinding {
        self.binding
    }
    pub fn baseline(&self) -> &Arc<SettingsBaseline> {
        &self.baseline
    }
    pub fn discard_to_baseline(
        &mut self,
        baseline: Arc<SettingsBaseline>,
    ) -> Result<(), SettingsServiceError> {
        self.advance()?;
        self.binding.source = baseline.identity();
        self.baseline = baseline;
        self.values = std::array::from_fn(|_| None);
        Ok(())
    }
    pub fn issue(&self, key: SettingKey) -> Option<SettingsIssue> {
        if !self.baseline.row(key).is_available() {
            return self.baseline.row(key).issue;
        }
        self.is_modified(key)
            .then(|| validate_value(key, self.value(key)).err())
            .flatten()
    }
    pub fn can_apply(&self) -> bool {
        !self.modified().is_empty() && self.issues().is_empty()
    }
    pub fn value(&self, key: SettingKey) -> &str {
        self.values[index(key)]
            .as_deref()
            .unwrap_or(&self.baseline.row(key).value)
    }
    pub fn modified(&self) -> Vec<SettingKey> {
        SETTINGS_KEYS
            .into_iter()
            .filter(|key| self.values[index(*key)].is_some())
            .collect()
    }
    pub fn is_modified(&self, key: SettingKey) -> bool {
        self.values[index(key)].is_some()
    }
    fn advance(&mut self) -> Result<(), SettingsServiceError> {
        self.binding.draft_revision = self
            .binding
            .draft_revision
            .checked_add(1)
            .ok_or(SettingsServiceError::Stale)?;
        Ok(())
    }
    pub fn set(&mut self, value: SettingsDraftValue) -> Result<bool, SettingsServiceError> {
        let key = value.key();
        if !self.baseline.row(key).is_available() {
            return Err(SettingsServiceError::Unavailable(key));
        }
        let value = value.text();
        let maximum = match key {
            SettingKey::ActiveThemeId => 256,
            SettingKey::ContextCompactionTimeout | SettingKey::DraftAutosaveInterval => 20,
            SettingKey::DeveloperInstructions => DEVELOPER_INSTRUCTIONS_DRAFT_BYTES,
            SettingKey::EndTurnSound => 32 * 1024,
        };
        if value.len() > maximum {
            return Err(SettingsServiceError::Invalid {
                key,
                issue: SettingsIssue::TooLong {
                    maximum,
                    actual: value.len(),
                },
            });
        }
        if self.value(key) == value {
            return Ok(false);
        }
        self.advance()?;
        self.values[index(key)] = (value != self.baseline.row(key).value).then_some(value);
        Ok(true)
    }
    pub fn reset(&mut self, key: SettingKey) -> Result<bool, SettingsServiceError> {
        if !self.baseline.row(key).is_available() {
            return Err(SettingsServiceError::Unavailable(key));
        }
        if self.values[index(key)].is_none() {
            return Ok(false);
        }
        self.advance()?;
        self.values[index(key)] = None;
        Ok(true)
    }
    pub fn set_route(&mut self, route_identity: u64) -> Result<(), SettingsServiceError> {
        if route_identity != self.binding.route_identity {
            self.advance()?;
            self.binding.route_identity = route_identity;
        }
        Ok(())
    }
    pub fn issues(&self) -> Vec<(SettingKey, SettingsIssue)> {
        self.modified()
            .into_iter()
            .filter_map(|key| {
                if !self.baseline.row(key).is_available() {
                    Some((key, SettingsIssue::Excluded))
                } else {
                    validate_value(key, self.value(key))
                        .err()
                        .map(|issue| (key, issue))
                }
            })
            .collect()
    }
    pub fn accept_committed(
        &mut self,
        observation: &SettingsApplyObservation,
    ) -> Result<bool, SettingsServiceError> {
        let SettingsApplyObservation::Committed {
            binding, baseline, ..
        } = observation
        else {
            return Ok(false);
        };
        if *binding != self.binding {
            return Err(SettingsServiceError::Stale);
        }
        self.advance()?;
        self.binding.source = baseline.identity();
        self.baseline = Arc::clone(baseline);
        self.values = std::array::from_fn(|_| None);
        Ok(true)
    }
    pub fn mark_scoped_unresolved(
        &mut self,
        observation: &SettingsApplyObservation,
    ) -> Result<bool, SettingsServiceError> {
        let SettingsApplyObservation::ScopedUnresolved {
            binding, baseline, ..
        } = observation
        else {
            return Ok(false);
        };
        if *binding != self.binding {
            return Err(SettingsServiceError::Stale);
        }
        let mut retained = (*self.baseline).clone();
        for key in self.modified() {
            if baseline.row(key).availability == SettingsRowAvailability::Excluded {
                retained.rows[index(key)].availability = SettingsRowAvailability::Excluded;
                retained.rows[index(key)].issue = Some(SettingsIssue::Excluded);
            }
        }
        self.baseline = Arc::new(retained);
        Ok(true)
    }
}

pub(super) fn validate_value(key: SettingKey, text: &str) -> Result<SettingValue, SettingsIssue> {
    let invalid = || SettingsIssue::InvalidValue;
    match key {
        SettingKey::ActiveThemeId => {
            InstalledThemeId::new(text).map_err(|_| invalid())?;
            SettingValue::active_theme_id(text).map_err(|_| invalid())
        }
        SettingKey::ContextCompactionTimeout => {
            let seconds: u64 = text.parse().map_err(|_| invalid())?;
            if !(1..=86_400).contains(&seconds) {
                return Err(invalid());
            }
            Ok(SettingValue::context_compaction_timeout_millis(
                seconds * 1_000,
            ))
        }
        SettingKey::DraftAutosaveInterval => {
            let seconds: u64 = text.parse().map_err(|_| invalid())?;
            if !(5..=300).contains(&seconds) {
                return Err(invalid());
            }
            Ok(SettingValue::draft_autosave_interval_seconds(seconds))
        }
        SettingKey::DeveloperInstructions => {
            SettingValue::developer_instructions(text).map_err(|_| SettingsIssue::TooLong {
                maximum: 60 * 1024,
                actual: text.len(),
            })
        }
        SettingKey::EndTurnSound => {
            let path = if text.is_empty() {
                None
            } else {
                let flavor = if cfg!(windows) {
                    PathFlavor::Windows
                } else {
                    PathFlavor::Posix
                };
                Some(AdmittedHostPath::from_admitted(flavor, text).map_err(|_| invalid())?)
            };
            Ok(SettingValue::end_turn_sound(path))
        }
    }
}

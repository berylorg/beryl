use std::{fmt, time::Duration};

use beryl_home_store::HomeStore;
use beryl_state::SettingKey;
use crate::settings_service::{AppliedSettingOrigin, AppliedSettingsReader};

use super::ContextCompactionError;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone)]
pub struct ContextCompactionTimeoutPolicy(TimeoutPolicy);

#[derive(Clone)]
enum TimeoutPolicy {
    Fixed(Duration),
    AppliedSettings(AppliedSettingsReader),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextCompactionTimeoutSource {
    Fixed,
    Applied,
    AbsentDefault,
    RejectedSavedValue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedContextCompactionTimeout {
    duration: Duration,
    source: ContextCompactionTimeoutSource,
}

impl ResolvedContextCompactionTimeout {
    pub const fn duration(self) -> Duration {
        self.duration
    }

    pub const fn source(self) -> ContextCompactionTimeoutSource {
        self.source
    }

    pub(super) const fn fixed(duration: Duration) -> Self {
        Self {
            duration,
            source: ContextCompactionTimeoutSource::Fixed,
        }
    }
}

impl ContextCompactionTimeoutPolicy {
    pub const fn fixed(timeout: Duration) -> Self {
        Self(TimeoutPolicy::Fixed(timeout))
    }

    pub fn applied_settings(settings: AppliedSettingsReader) -> Self {
        Self(TimeoutPolicy::AppliedSettings(settings))
    }

    pub fn validate(&self) -> Result<(), ContextCompactionError> {
        if let TimeoutPolicy::Fixed(timeout) = self.0 {
            super::coordinator::model::validate_completion_timeout(timeout)?;
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        home: &HomeStore,
    ) -> Result<ResolvedContextCompactionTimeout, ContextCompactionError> {
        match &self.0 {
            TimeoutPolicy::Fixed(timeout) => {
                self.validate()?;
                Ok(ResolvedContextCompactionTimeout::fixed(*timeout))
            }
            TimeoutPolicy::AppliedSettings(settings) => {
                let snapshot = settings.snapshot().map_err(|_| ContextCompactionError::Unavailable)?;
                if !snapshot.matches_home(home) {
                    return Err(ContextCompactionError::Unavailable);
                }
                let duration = snapshot.compaction_timeout().map_err(|_| ContextCompactionError::Unavailable)?;
                let source = match snapshot.origin(SettingKey::ContextCompactionTimeout) {
                    AppliedSettingOrigin::Saved => ContextCompactionTimeoutSource::Applied,
                    AppliedSettingOrigin::AbsentDefault => ContextCompactionTimeoutSource::AbsentDefault,
                    AppliedSettingOrigin::RejectedSaved => ContextCompactionTimeoutSource::RejectedSavedValue,
                };
                Ok(ResolvedContextCompactionTimeout {
                    duration,
                    source,
                })
            }
        }
    }
}

impl Default for ContextCompactionTimeoutPolicy {
    fn default() -> Self {
        Self::fixed(DEFAULT_TIMEOUT)
    }
}

impl fmt::Debug for ContextCompactionTimeoutPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TimeoutPolicy::Fixed(timeout) => formatter.debug_tuple("Fixed").field(timeout).finish(),
            TimeoutPolicy::AppliedSettings(_) => formatter.write_str("AppliedSettings"),
        }
    }
}

impl PartialEq for ContextCompactionTimeoutPolicy {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (TimeoutPolicy::Fixed(left), TimeoutPolicy::Fixed(right)) => left == right,
            (TimeoutPolicy::AppliedSettings(left), TimeoutPolicy::AppliedSettings(right)) => {
                left.same_source(right)
            }
            _ => false,
        }
    }
}

impl Eq for ContextCompactionTimeoutPolicy {}

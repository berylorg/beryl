use std::{fmt, sync::Arc, time::Duration};

use beryl_home_store::HomeStore;
use beryl_state::{SettingKey, SettingsState};

use super::ContextCompactionError;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone)]
pub struct ContextCompactionTimeoutPolicy(TimeoutPolicy);

#[derive(Clone)]
enum TimeoutPolicy {
    Fixed(Duration),
    AppliedSettings(Arc<SettingsState>),
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

    pub fn applied_settings(settings: SettingsState) -> Self {
        Self(TimeoutPolicy::AppliedSettings(Arc::new(settings)))
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
                let record = settings.setting(home, SettingKey::ContextCompactionTimeout)?;
                let Some(record) = record else {
                    return Ok(ResolvedContextCompactionTimeout {
                        duration: DEFAULT_TIMEOUT,
                        source: ContextCompactionTimeoutSource::AbsentDefault,
                    });
                };
                let millis = record.value().as_context_compaction_timeout_millis();
                Ok(match millis {
                    Some(millis)
                        if (1_000..=86_400_000).contains(&millis) && millis % 1_000 == 0 =>
                    {
                        ResolvedContextCompactionTimeout {
                            duration: Duration::from_millis(millis),
                            source: ContextCompactionTimeoutSource::Applied,
                        }
                    }
                    _ => ResolvedContextCompactionTimeout {
                        duration: DEFAULT_TIMEOUT,
                        source: ContextCompactionTimeoutSource::RejectedSavedValue,
                    },
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
                Arc::ptr_eq(left, right)
            }
            _ => false,
        }
    }
}

impl Eq for ContextCompactionTimeoutPolicy {}

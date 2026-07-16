use std::{error::Error, fmt, time::Duration};

use beryl_state::{RecordRevision, SettingKey, SettingRecord};

/// Default draft-autosave interval in seconds.
pub const DEFAULT_AUTOSAVE_SECONDS: u64 = 30;
/// Minimum configurable draft-autosave interval in seconds.
pub const MIN_AUTOSAVE_SECONDS: u64 = 5;
/// Maximum configurable draft-autosave interval in seconds.
pub const MAX_AUTOSAVE_SECONDS: u64 = 300;

/// Validated, non-disableable draft-autosave interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftAutosaveInterval(Duration);

impl DraftAutosaveInterval {
    /// The product default.
    pub const DEFAULT: Self = Self(Duration::from_secs(DEFAULT_AUTOSAVE_SECONDS));

    pub fn from_seconds(seconds: u64) -> Result<Self, DraftAutosaveIntervalError> {
        if !(MIN_AUTOSAVE_SECONDS..=MAX_AUTOSAVE_SECONDS).contains(&seconds) {
            return Err(DraftAutosaveIntervalError {
                kind: DraftAutosaveIntervalErrorKind::OutOfRange(seconds),
            });
        }
        Ok(Self(Duration::from_secs(seconds)))
    }

    /// Returns the validated duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

impl Default for DraftAutosaveInterval {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Why a draft-autosave interval was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftAutosaveIntervalError {
    kind: DraftAutosaveIntervalErrorKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DraftAutosaveIntervalErrorKind {
    OutOfRange(u64),
    WrongSetting,
}

impl fmt::Display for DraftAutosaveIntervalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "draft autosave interval must be {MIN_AUTOSAVE_SECONDS}..={MAX_AUTOSAVE_SECONDS} seconds, got {}",
            match self.kind {
                DraftAutosaveIntervalErrorKind::OutOfRange(seconds) => seconds,
                DraftAutosaveIntervalErrorKind::WrongSetting => {
                    return formatter.write_str(
                        "draft autosave publication must contain the draft-autosave setting",
                    );
                }
            }
        )
    }
}

impl Error for DraftAutosaveIntervalError {}

/// Exact committed setting publication applied to the autosave timer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftAutosavePublication {
    interval: DraftAutosaveInterval,
    revision: Option<RecordRevision>,
}

impl DraftAutosavePublication {
    /// Uses the 30-second default only when the durable setting is absent.
    #[must_use]
    pub const fn absent_default() -> Self {
        Self {
            interval: DraftAutosaveInterval::DEFAULT,
            revision: None,
        }
    }

    pub fn from_record(record: &SettingRecord) -> Result<Self, DraftAutosaveIntervalError> {
        if record.key() != SettingKey::DraftAutosaveInterval {
            return Err(DraftAutosaveIntervalError {
                kind: DraftAutosaveIntervalErrorKind::WrongSetting,
            });
        }
        let seconds = record.value().as_draft_autosave_interval_seconds().ok_or(
            DraftAutosaveIntervalError {
                kind: DraftAutosaveIntervalErrorKind::WrongSetting,
            },
        )?;
        Ok(Self {
            interval: DraftAutosaveInterval::from_seconds(seconds)?,
            revision: Some(record.revision()),
        })
    }

    #[must_use]
    pub const fn interval(self) -> DraftAutosaveInterval {
        self.interval
    }

    #[must_use]
    pub const fn revision(self) -> Option<RecordRevision> {
        self.revision
    }
}

/// Whether an exact setting publication changed the active timer authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftAutosavePublicationAction {
    Applied,
    Stale,
}

/// Caller-observed monotonic process time used only for deterministic scheduling.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DraftPersistenceTime(Duration);

impl DraftPersistenceTime {
    #[must_use]
    pub const fn from_duration(value: Duration) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }

    pub(crate) fn elapsed_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }
}

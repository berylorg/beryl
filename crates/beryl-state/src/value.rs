use std::{error::Error, fmt, num::NonZeroU64};

use beryl_model::Availability;

/// Why a bounded Beryl-state value was rejected before persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueError {
    /// Unknown availability cannot claim an observation time.
    UnknownAvailabilityObserved,
    /// An actual availability observation requires its time.
    AvailabilityObservationMissing,
    /// Zero is reserved as the absence of a record revision.
    ZeroRecordRevision,
    /// A monotonic package-owned revision was exhausted.
    RevisionExhausted,
}

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownAvailabilityObserved => {
                formatter.write_str("unknown availability cannot have an observation time")
            }
            Self::AvailabilityObservationMissing => {
                formatter.write_str("observed availability requires an observation time")
            }
            Self::ZeroRecordRevision => formatter.write_str("record revision must be nonzero"),
            Self::RevisionExhausted => formatter.write_str("record revision is exhausted"),
        }
    }
}

impl Error for ValueError {}

/// Package-local monotonic revision of one Beryl-state record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecordRevision(NonZeroU64);

impl RecordRevision {
    /// Initial revision assigned when a record is first created.
    pub const INITIAL: Self = Self(NonZeroU64::MIN);

    /// Constructs an exact persisted record revision.
    pub fn new(value: u64) -> Result<Self, ValueError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(ValueError::ZeroRecordRevision)
    }

    /// Returns the integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub(crate) fn checked_next(self) -> Result<Self, ValueError> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(ValueError::RevisionExhausted)
    }
}

/// Caller-supplied milliseconds since the Unix epoch.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UnixMillis(u64);

impl UnixMillis {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Bounded current availability plus optional observation time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AvailabilitySnapshot {
    availability: Availability,
    observed_at: Option<UnixMillis>,
}

impl AvailabilitySnapshot {
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            availability: Availability::Unknown,
            observed_at: None,
        }
    }

    pub fn observed(
        availability: Availability,
        observed_at: UnixMillis,
    ) -> Result<Self, ValueError> {
        if availability == Availability::Unknown {
            return Err(ValueError::UnknownAvailabilityObserved);
        }
        Ok(Self {
            availability,
            observed_at: Some(observed_at),
        })
    }

    pub(crate) fn from_parts(
        availability: Availability,
        observed_at: Option<UnixMillis>,
    ) -> Result<Self, ValueError> {
        match (availability, observed_at) {
            (Availability::Unknown, None) => Ok(Self::unknown()),
            (Availability::Unknown, Some(_)) => Err(ValueError::UnknownAvailabilityObserved),
            (_, None) => Err(ValueError::AvailabilityObservationMissing),
            (_, Some(observed_at)) => Self::observed(availability, observed_at),
        }
    }

    #[must_use]
    pub const fn availability(self) -> Availability {
        self.availability
    }

    #[must_use]
    pub const fn observed_at(self) -> Option<UnixMillis> {
        self.observed_at
    }
}

use std::{error::Error, fmt, num::NonZeroU64};

use serde::{Deserialize, Serialize};

/// Why a typed monotonic revision could not be constructed or advanced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevisionError {
    /// Zero is reserved as the absence of a revision.
    Zero,
    /// The revision cannot advance beyond its maximum value.
    Exhausted,
}

impl fmt::Display for RevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("revision must be non-zero"),
            Self::Exhausted => formatter.write_str("revision is exhausted"),
        }
    }
}

impl Error for RevisionError {}

macro_rules! typed_revision {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// Constructs a revision, rejecting the reserved zero value.
            pub fn new(value: u64) -> Result<Self, RevisionError> {
                NonZeroU64::new(value).map(Self).ok_or(RevisionError::Zero)
            }

            /// Constructs a revision from an already validated non-zero value.
            #[must_use]
            pub const fn from_nonzero(value: NonZeroU64) -> Self {
                Self(value)
            }

            /// Returns the integer representation owned by the consuming domain.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0.get()
            }

            /// Returns the next monotonic revision.
            pub fn checked_next(self) -> Result<Self, RevisionError> {
                self.0
                    .get()
                    .checked_add(1)
                    .and_then(NonZeroU64::new)
                    .map(Self)
                    .ok_or(RevisionError::Exhausted)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

typed_revision!(
    /// Revision of the complete physical Beryl-home generation.
    HomeRevision
);
typed_revision!(
    /// Revision of one registered logical storage domain.
    DomainRevision
);
typed_revision!(
    /// Revision of one Syndic thread record.
    ThreadRevision
);
typed_revision!(
    /// Revision of one current Syndic draft.
    DraftRevision
);
typed_revision!(
    /// Revision of one chunked Syndic content frontier.
    ContentRevision
);
typed_revision!(
    /// Revision of one execution-projection binding.
    BindingRevision
);
typed_revision!(
    /// Revision of one durable accepted-input record.
    AcceptedInputRevision
);
typed_revision!(
    /// Revision of one thread's durable input-admission gate.
    InputGateRevision
);
typed_revision!(
    /// Revision of one Syndic transcript or resource projection frontier.
    ProjectionRevision
);
typed_revision!(
    /// Revision of one durable main-window thread claim.
    ClaimRevision
);
typed_revision!(
    /// Revision of the active durable session generation.
    SessionRevision
);
typed_revision!(
    /// Revision of one durable orchestration job.
    JobRevision
);

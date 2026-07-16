use std::num::NonZeroU64;

use super::SyndicValueError;

macro_rules! one_based_value {
    ($(#[$meta:meta])* $name:ident, $kind:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// First valid value in this ordered domain.
            pub const FIRST: Self = Self(NonZeroU64::MIN);

            /// Constructs an exact admitted value, rejecting reserved zero.
            pub fn new(value: u64) -> Result<Self, SyndicValueError> {
                NonZeroU64::new(value)
                    .map(Self)
                    .ok_or(SyndicValueError::ZeroOrdinal { kind: $kind })
            }

            /// Returns the integer representation.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0.get()
            }

            /// Advances one step without wrapping.
            pub fn checked_next(self) -> Result<Self, SyndicValueError> {
                self.get()
                    .checked_add(1)
                    .and_then(NonZeroU64::new)
                    .map(Self)
                    .ok_or(SyndicValueError::OrdinalExhausted { kind: $kind })
            }
        }
    };
}

one_based_value!(
    /// Stable per-thread admission order of one accepted input fragment.
    AcceptedInputOrdinal,
    "accepted-input ordinal"
);
one_based_value!(
    /// Final nonzero per-thread label allocated to one durable image marker.
    ImageLabelOrdinal,
    "image-label ordinal"
);
one_based_value!(
    /// Exact order of one resolved image marker in an admitted input.
    InputMarkerOrdinal,
    "input-marker ordinal"
);
one_based_value!(
    /// Monotonic source order of one normalized live event within a turn.
    SourceEventSequence,
    "source-event sequence"
);
one_based_value!(
    /// Stable sortable position of one record in a Syndic transcript view.
    TranscriptPosition,
    "transcript position"
);
one_based_value!(
    /// Immutable identity of one rebuildable transcript-view generation.
    TranscriptGeneration,
    "transcript generation"
);
one_based_value!(
    /// Immutable identity of one canonical item's projection generation.
    ItemProjectionGeneration,
    "item-projection generation"
);
one_based_value!(
    /// Immutable depth of one submitted turn, with roots at one.
    TurnDepth,
    "turn depth"
);
one_based_value!(
    /// Mutable lifecycle/frontier revision of one submitted turn.
    TurnStateRevision,
    "turn-state revision"
);
one_based_value!(
    /// Immutable revision of one discussion-context envelope.
    ContextEnvelopeRevision,
    "context-envelope revision"
);
one_based_value!(
    /// Stable order of one canonical item within a turn.
    TurnItemOrdinal,
    "turn-item ordinal"
);
one_based_value!(
    /// Stable order of one admitted source event affecting a canonical item.
    ItemSourceEventOrdinal,
    "item source-event ordinal"
);
one_based_value!(
    /// Stable order of one render-significant piece within encoded canonical content.
    ContentPieceOrdinal,
    "content piece ordinal"
);
one_based_value!(
    /// Exact order of one atom in canonical composer content.
    ComposerAtomOrdinal,
    "composer atom ordinal"
);
one_based_value!(
    /// Stable order of one projection within a canonical item.
    ProjectionOrdinal,
    "projection ordinal"
);
one_based_value!(
    /// Stable order of one resource within a projection.
    ResourceOrdinal,
    "resource ordinal"
);

/// Caller-supplied milliseconds since the Unix epoch.
///
/// This pure value does not observe a clock or choose a timestamp.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SyndicTimestamp(u64);

impl SyndicTimestamp {
    #[must_use]
    pub const fn from_unix_millis(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn unix_millis(self) -> u64 {
        self.0
    }
}

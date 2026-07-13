use std::{error::Error, fmt, num::NonZeroU64};

use beryl_model::{Availability, JobId, ThreadRevision};

/// Maximum UTF-8 bytes in one accepted generated thread title.
pub const GENERATED_TITLE_MAX_BYTES: usize = 512;

/// Why a bounded Beryl-state value was rejected before persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueError {
    /// Text is empty.
    Empty { kind: &'static str },
    /// Text exceeds its schema budget.
    TooLong {
        kind: &'static str,
        maximum: usize,
        actual: usize,
    },
    /// Text has leading or trailing whitespace.
    SurroundingWhitespace { kind: &'static str },
    /// Text contains a control character.
    ControlCharacter { kind: &'static str, index: usize },
    /// Unknown availability cannot claim an observation time.
    UnknownAvailabilityObserved,
    /// An actual availability observation requires its time.
    AvailabilityObservationMissing,
    /// A present model context window must be positive.
    ZeroModelContextWindow,
    /// Zero is reserved as the absence of a record revision.
    ZeroRecordRevision,
    /// A monotonic package-owned revision was exhausted.
    RevisionExhausted,
}

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { kind } => write!(formatter, "{kind} must not be empty"),
            Self::TooLong {
                kind,
                maximum,
                actual,
            } => write!(
                formatter,
                "{kind} must not exceed {maximum} UTF-8 bytes, got {actual}"
            ),
            Self::SurroundingWhitespace { kind } => {
                write!(formatter, "{kind} must not have surrounding whitespace")
            }
            Self::ControlCharacter { kind, index } => {
                write!(
                    formatter,
                    "{kind} contains a control character at byte {index}"
                )
            }
            Self::UnknownAvailabilityObserved => {
                formatter.write_str("unknown availability cannot have an observation time")
            }
            Self::AvailabilityObservationMissing => {
                formatter.write_str("observed availability requires an observation time")
            }
            Self::ZeroModelContextWindow => {
                formatter.write_str("model context window must be positive when present")
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

/// Validated immutable generated-title metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedTitle {
    text: Box<str>,
    source_thread_revision: ThreadRevision,
    generated_at: UnixMillis,
}

impl GeneratedTitle {
    pub fn new(
        text: impl AsRef<str>,
        source_thread_revision: ThreadRevision,
        generated_at: UnixMillis,
    ) -> Result<Self, ValueError> {
        let text = text.as_ref();
        validate_text("generated title", text, GENERATED_TITLE_MAX_BYTES)?;
        Ok(Self {
            text: text.into(),
            source_thread_revision,
            generated_at,
        })
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub const fn source_thread_revision(&self) -> ThreadRevision {
        self.source_thread_revision
    }

    #[must_use]
    pub const fn generated_at(&self) -> UnixMillis {
        self.generated_at
    }
}

/// Exact recent-activity fact from one Syndic thread revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadActivitySummary {
    source_thread_revision: ThreadRevision,
    last_activity_at: UnixMillis,
}

impl ThreadActivitySummary {
    #[must_use]
    pub const fn new(source_thread_revision: ThreadRevision, last_activity_at: UnixMillis) -> Self {
        Self {
            source_thread_revision,
            last_activity_at,
        }
    }

    #[must_use]
    pub const fn source_thread_revision(self) -> ThreadRevision {
        self.source_thread_revision
    }

    #[must_use]
    pub const fn last_activity_at(self) -> UnixMillis {
        self.last_activity_at
    }
}

/// Exact nonnegative token counters from one CAS usage notification.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenUsageBreakdown {
    cached_input_tokens: u64,
    input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
}

impl TokenUsageBreakdown {
    #[must_use]
    pub const fn new(
        cached_input_tokens: u64,
        input_tokens: u64,
        output_tokens: u64,
        reasoning_output_tokens: u64,
        total_tokens: u64,
    ) -> Self {
        Self {
            cached_input_tokens,
            input_tokens,
            output_tokens,
            reasoning_output_tokens,
            total_tokens,
        }
    }

    #[must_use]
    pub const fn cached_input_tokens(self) -> u64 {
        self.cached_input_tokens
    }

    #[must_use]
    pub const fn input_tokens(self) -> u64 {
        self.input_tokens
    }

    #[must_use]
    pub const fn output_tokens(self) -> u64 {
        self.output_tokens
    }

    #[must_use]
    pub const fn reasoning_output_tokens(self) -> u64 {
        self.reasoning_output_tokens
    }

    #[must_use]
    pub const fn total_tokens(self) -> u64 {
        self.total_tokens
    }
}

/// Durable exact token-usage presentation snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenUsageSnapshot {
    last: TokenUsageBreakdown,
    total: TokenUsageBreakdown,
    model_context_window: Option<NonZeroU64>,
    source_thread_revision: ThreadRevision,
    observed_at: UnixMillis,
}

impl TokenUsageSnapshot {
    pub fn new(
        last: TokenUsageBreakdown,
        total: TokenUsageBreakdown,
        model_context_window: Option<u64>,
        source_thread_revision: ThreadRevision,
        observed_at: UnixMillis,
    ) -> Result<Self, ValueError> {
        let model_context_window = model_context_window
            .map(|value| NonZeroU64::new(value).ok_or(ValueError::ZeroModelContextWindow))
            .transpose()?;
        Ok(Self {
            last,
            total,
            model_context_window,
            source_thread_revision,
            observed_at,
        })
    }

    #[must_use]
    pub const fn last(self) -> TokenUsageBreakdown {
        self.last
    }

    #[must_use]
    pub const fn total(self) -> TokenUsageBreakdown {
        self.total
    }

    #[must_use]
    pub const fn model_context_window(self) -> Option<u64> {
        match self.model_context_window {
            Some(value) => Some(value.get()),
            None => None,
        }
    }

    #[must_use]
    pub const fn source_thread_revision(self) -> ThreadRevision {
        self.source_thread_revision
    }

    #[must_use]
    pub const fn observed_at(self) -> UnixMillis {
        self.observed_at
    }
}

/// Beryl-owned automatic archive presentation state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadArchiveState {
    Ordinary,
    BranchDiscussionOpen,
    BranchDiscussionArchived {
        handoff_job_id: JobId,
        archived_at: UnixMillis,
    },
}

impl ThreadArchiveState {
    #[must_use]
    pub const fn is_archived(self) -> bool {
        matches!(self, Self::BranchDiscussionArchived { .. })
    }
}

fn validate_text(kind: &'static str, value: &str, maximum: usize) -> Result<(), ValueError> {
    if value.is_empty() {
        return Err(ValueError::Empty { kind });
    }
    if value.len() > maximum {
        return Err(ValueError::TooLong {
            kind,
            maximum,
            actual: value.len(),
        });
    }
    if value.trim() != value {
        return Err(ValueError::SurroundingWhitespace { kind });
    }
    if let Some((index, _)) = value.char_indices().find(|(_, value)| value.is_control()) {
        return Err(ValueError::ControlCharacter { kind, index });
    }
    Ok(())
}

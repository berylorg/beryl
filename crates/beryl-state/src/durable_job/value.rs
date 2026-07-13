use std::{error::Error, fmt, num::NonZeroU64};

use beryl_model::{
    CasThreadId, CasTurnId, DynamicToolCallId, SyndicAcceptedInputId, SyndicDraftId, SyndicTurnId,
};

/// Maximum UTF-8 bytes retained for one admitted branch-resolution payload.
pub const RESOLUTION_TEXT_MAX_BYTES: usize = 64 * 1024;

/// Maximum UTF-8 bytes retained as diagnostic evidence for one job failure.
pub const HANDOFF_FAILURE_DETAIL_MAX_BYTES: usize = 2 * 1024;

/// Why a typed durable-job value was rejected before persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DurableJobValueError {
    Empty {
        kind: &'static str,
    },
    TooLong {
        kind: &'static str,
        maximum: usize,
        actual: usize,
    },
    SurroundingWhitespace {
        kind: &'static str,
    },
    ControlCharacter {
        kind: &'static str,
        index: usize,
    },
    ZeroAttemptOrdinal,
    AttemptOrdinalExhausted,
}

impl fmt::Display for DurableJobValueError {
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
            Self::ZeroAttemptOrdinal => {
                formatter.write_str("resolution attempt ordinal must be nonzero")
            }
            Self::AttemptOrdinalExhausted => {
                formatter.write_str("resolution attempt ordinal is exhausted")
            }
        }
    }
}

impl Error for DurableJobValueError {}

/// One-based durable order of resolution attempts for one discussion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResolutionAttemptOrdinal(NonZeroU64);

impl ResolutionAttemptOrdinal {
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    pub fn new(value: u64) -> Result<Self, DurableJobValueError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(DurableJobValueError::ZeroAttemptOrdinal)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub(crate) fn checked_next(self) -> Result<Self, DurableJobValueError> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(DurableJobValueError::AttemptOrdinalExhausted)
    }
}

/// Exact parent accepted-input queue position reserved by an admitted job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ParentQueueOrdinal(u64);

impl ParentQueueOrdinal {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Exact bounded model-produced resolution retained unchanged after admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionText(Box<str>);

impl ResolutionText {
    pub fn new(value: impl AsRef<str>) -> Result<Self, DurableJobValueError> {
        let value = value.as_ref();
        validate_resolution_text(value)?;
        Ok(Self(value.into()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Immutable identity of the draft or submitted turn that owns discussion context.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DiscussionContextOwnerId {
    Draft(SyndicDraftId),
    SubmittedTurn(SyndicTurnId),
}

/// Exact digest from the immutable discussion context envelope.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DiscussionContextDigest([u8; 32]);

impl DiscussionContextDigest {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Full CAS correlation that makes one dynamic-tool request idempotent.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ResolutionRequestIdentity {
    cas_thread_id: CasThreadId,
    cas_turn_id: CasTurnId,
    tool_call_id: DynamicToolCallId,
}

impl ResolutionRequestIdentity {
    #[must_use]
    pub const fn new(
        cas_thread_id: CasThreadId,
        cas_turn_id: CasTurnId,
        tool_call_id: DynamicToolCallId,
    ) -> Self {
        Self {
            cas_thread_id,
            cas_turn_id,
            tool_call_id,
        }
    }

    #[must_use]
    pub const fn cas_thread_id(&self) -> &CasThreadId {
        &self.cas_thread_id
    }

    #[must_use]
    pub const fn cas_turn_id(&self) -> &CasTurnId {
        &self.cas_turn_id
    }

    #[must_use]
    pub const fn tool_call_id(&self) -> &DynamicToolCallId {
        &self.tool_call_id
    }
}

/// Exact Syndic identities created by normal parent accepted-input admission.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ParentHandoffIdentity {
    accepted_input_id: SyndicAcceptedInputId,
    turn_id: SyndicTurnId,
}

impl ParentHandoffIdentity {
    #[must_use]
    pub const fn new(accepted_input_id: SyndicAcceptedInputId, turn_id: SyndicTurnId) -> Self {
        Self {
            accepted_input_id,
            turn_id,
        }
    }

    #[must_use]
    pub const fn accepted_input_id(self) -> SyndicAcceptedInputId {
        self.accepted_input_id
    }

    #[must_use]
    pub const fn turn_id(self) -> SyndicTurnId {
        self.turn_id
    }
}

/// Exact parent CAS identities durably correlated after CAS accepts the turn.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ParentCasIdentity {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
}

impl ParentCasIdentity {
    #[must_use]
    pub const fn new(thread_id: CasThreadId, turn_id: CasTurnId) -> Self {
        Self { thread_id, turn_id }
    }

    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }
}

/// Typed reason retained with a retryable or terminal handoff failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HandoffFailureKind {
    RuntimeUnavailable,
    RootUnavailable,
    CasUnavailable,
    TransientDeliveryFailure,
    CasRejectedBeforeAcceptance,
    InvariantViolation,
    ParentMissing,
    UnrecoverablePostAppend,
    ParentInterrupted,
    ParentIncomplete,
    ParentTerminalFailure,
}

impl HandoffFailureKind {
    #[must_use]
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::RuntimeUnavailable
                | Self::RootUnavailable
                | Self::CasUnavailable
                | Self::TransientDeliveryFailure
                | Self::CasRejectedBeforeAcceptance
        )
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !self.is_retryable()
    }
}

/// Bounded structured evidence for the current durable failure state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandoffFailureEvidence {
    kind: HandoffFailureKind,
    detail: Option<Box<str>>,
}

impl HandoffFailureEvidence {
    pub fn new(
        kind: HandoffFailureKind,
        detail: Option<&str>,
    ) -> Result<Self, DurableJobValueError> {
        let detail = detail.map(validate_failure_detail).transpose()?;
        Ok(Self { kind, detail })
    }

    #[must_use]
    pub const fn kind(&self) -> HandoffFailureKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

fn validate_resolution_text(value: &str) -> Result<(), DurableJobValueError> {
    if value.is_empty() {
        return Err(DurableJobValueError::Empty {
            kind: "branch resolution text",
        });
    }
    if value.len() > RESOLUTION_TEXT_MAX_BYTES {
        return Err(DurableJobValueError::TooLong {
            kind: "branch resolution text",
            maximum: RESOLUTION_TEXT_MAX_BYTES,
            actual: value.len(),
        });
    }
    if let Some(index) = value.as_bytes().iter().position(|byte| *byte == 0) {
        return Err(DurableJobValueError::ControlCharacter {
            kind: "branch resolution text",
            index,
        });
    }
    Ok(())
}

fn validate_failure_detail(value: &str) -> Result<Box<str>, DurableJobValueError> {
    const KIND: &str = "handoff failure detail";
    if value.is_empty() {
        return Err(DurableJobValueError::Empty { kind: KIND });
    }
    if value.len() > HANDOFF_FAILURE_DETAIL_MAX_BYTES {
        return Err(DurableJobValueError::TooLong {
            kind: KIND,
            maximum: HANDOFF_FAILURE_DETAIL_MAX_BYTES,
            actual: value.len(),
        });
    }
    if value.trim() != value {
        return Err(DurableJobValueError::SurroundingWhitespace { kind: KIND });
    }
    if let Some((index, _)) = value
        .char_indices()
        .find(|(_, character)| character.is_control())
    {
        return Err(DurableJobValueError::ControlCharacter { kind: KIND, index });
    }
    Ok(value.into())
}

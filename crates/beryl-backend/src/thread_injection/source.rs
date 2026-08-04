use std::{fmt, str};

use beryl_model::{
    RecoveryItemSequenceDigest, RecoveryItemSequenceError, RecoveryItemSequenceRole,
};
use beryl_stream::PageLease;
use thiserror::Error;

use super::THREAD_INJECTION_MAX_PAGE_BYTES;

/// Opaque immutable identity of one replayable recovery source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThreadInjectionSourceIdentity([u8; 32]);

impl ThreadInjectionSourceIdentity {
    /// Creates a storage-neutral source identity from its exact bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact opaque identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Opaque immutable revision of one replayable recovery source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThreadInjectionSourceRevision(u64);

impl ThreadInjectionSourceRevision {
    /// Creates a storage-neutral exact source revision.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the exact opaque revision value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Closed canonical role/content pair supported by recovery injection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ThreadInjectionRole {
    /// One user-role message containing one `input_text` value.
    UserInputText,
    /// One assistant-role message containing one `output_text` value.
    AssistantOutputText,
}

impl ThreadInjectionRole {
    pub(super) const fn sequence_role(self) -> RecoveryItemSequenceRole {
        match self {
            Self::UserInputText => RecoveryItemSequenceRole::UserInputText,
            Self::AssistantOutputText => RecoveryItemSequenceRole::AssistantOutputText,
        }
    }
}

/// One nonempty bounded valid-UTF-8 event from a sequential recovery source.
pub struct ThreadInjectionSourcePage {
    source_identity: ThreadInjectionSourceIdentity,
    source_revision: ThreadInjectionSourceRevision,
    item_ordinal: u64,
    role: ThreadInjectionRole,
    declared_item_utf8_bytes: u64,
    item_offset: u64,
    page: PageLease,
    item_terminal: bool,
    sequence_terminal: bool,
}

impl ThreadInjectionSourcePage {
    /// Builds one source event from an already-filled lease, rejecting a page that cannot make
    /// bounded UTF-8 progress.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_identity: ThreadInjectionSourceIdentity,
        source_revision: ThreadInjectionSourceRevision,
        item_ordinal: u64,
        role: ThreadInjectionRole,
        declared_item_utf8_bytes: u64,
        item_offset: u64,
        page: PageLease,
        item_terminal: bool,
        sequence_terminal: bool,
    ) -> Result<Self, ThreadInjectionSourceError> {
        if page.is_empty() {
            return Err(ThreadInjectionSourceError::EmptyPage);
        }
        if page.len() > THREAD_INJECTION_MAX_PAGE_BYTES {
            return Err(ThreadInjectionSourceError::PageTooLarge {
                maximum: THREAD_INJECTION_MAX_PAGE_BYTES,
                actual: page.len(),
            });
        }
        str::from_utf8(page.as_slice()).map_err(|_| ThreadInjectionSourceError::InvalidSource)?;
        Ok(Self {
            source_identity,
            source_revision,
            item_ordinal,
            role,
            declared_item_utf8_bytes,
            item_offset,
            page,
            item_terminal,
            sequence_terminal,
        })
    }

    #[must_use]
    pub const fn source_identity(&self) -> ThreadInjectionSourceIdentity {
        self.source_identity
    }

    #[must_use]
    pub const fn source_revision(&self) -> ThreadInjectionSourceRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn item_ordinal(&self) -> u64 {
        self.item_ordinal
    }

    #[must_use]
    pub const fn role(&self) -> ThreadInjectionRole {
        self.role
    }

    #[must_use]
    pub const fn declared_item_utf8_bytes(&self) -> u64 {
        self.declared_item_utf8_bytes
    }

    #[must_use]
    pub const fn item_offset(&self) -> u64 {
        self.item_offset
    }

    #[must_use]
    pub fn text(&self) -> &str {
        str::from_utf8(self.page.as_slice())
            .expect("source page UTF-8 was validated at construction")
    }

    #[must_use]
    pub const fn item_terminal(&self) -> bool {
        self.item_terminal
    }

    #[must_use]
    pub const fn sequence_terminal(&self) -> bool {
        self.sequence_terminal
    }
}

impl fmt::Debug for ThreadInjectionSourcePage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ThreadInjectionSourcePage")
            .field("source_identity", &self.source_identity)
            .field("source_revision", &self.source_revision)
            .field("item_ordinal", &self.item_ordinal)
            .field("role", &self.role)
            .field("declared_item_utf8_bytes", &self.declared_item_utf8_bytes)
            .field("item_offset", &self.item_offset)
            .field("page_utf8_bytes", &self.page.len())
            .field("item_terminal", &self.item_terminal)
            .field("sequence_terminal", &self.sequence_terminal)
            .finish()
    }
}

/// Mutable sequential source for one preflighted recovery injection.
///
/// Implementations are transferable and safe to own from a serialized session
/// worker. Each call advances exactly once and must honor the nonzero requested
/// byte ceiling. `None` is exact EOF and is valid only after the final page
/// declared both item and sequence terminal state.
pub trait ThreadInjectionSource: Send + Sync {
    fn next_page(
        &mut self,
        max_utf8_bytes: usize,
    ) -> Result<Option<ThreadInjectionSourcePage>, ThreadInjectionSourceError>;
}

/// Typed source or structural disagreement during recovery replay.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ThreadInjectionSourceError {
    #[error("thread-injection source read was cancelled")]
    Cancelled,
    #[error("thread-injection source is unavailable")]
    Unavailable,
    #[error("thread-injection source revision changed during replay")]
    RevisionDrift,
    #[error("thread-injection source read failed")]
    ReadFailed,
    #[error("thread-injection source rejected its compact proof or durable content")]
    InvalidSource,
    #[error("thread-injection source was asked for a zero-byte page")]
    ZeroPageRequest,
    #[error("thread-injection source ended before item {expected_item_ordinal}")]
    PrematureEof { expected_item_ordinal: u64 },
    #[error("thread-injection source returned a page after its final terminal page")]
    PageAfterSequenceTerminal,
    #[error("thread-injection page identity did not match preflight")]
    SourceIdentityMismatch {
        expected: ThreadInjectionSourceIdentity,
        actual: ThreadInjectionSourceIdentity,
    },
    #[error("thread-injection page revision did not match preflight")]
    SourceRevisionMismatch {
        expected: ThreadInjectionSourceRevision,
        actual: ThreadInjectionSourceRevision,
    },
    #[error("thread-injection page ordinal {actual} did not match expected ordinal {expected}")]
    ItemOrdinalMismatch { expected: u64, actual: u64 },
    #[error("thread-injection item {item_ordinal} declared zero UTF-8 bytes")]
    EmptyItem { item_ordinal: u64 },
    #[error("thread-injection source returned an empty page")]
    EmptyPage,
    #[error("thread-injection page had {actual} bytes, exceeding requested maximum {maximum}")]
    PageTooLarge { maximum: usize, actual: usize },
    #[error("thread-injection item {item_ordinal} changed role between pages")]
    ItemRoleMismatch { item_ordinal: u64 },
    #[error(
        "thread-injection item {item_ordinal} changed declared length from {expected} to {actual}"
    )]
    ItemLengthMismatch {
        item_ordinal: u64,
        expected: u64,
        actual: u64,
    },
    #[error(
        "thread-injection item {item_ordinal} page offset {actual} did not match expected offset {expected}"
    )]
    ItemOffsetMismatch {
        item_ordinal: u64,
        expected: u64,
        actual: u64,
    },
    #[error("thread-injection item {item_ordinal} page end overflowed")]
    ItemEndOverflow { item_ordinal: u64 },
    #[error(
        "thread-injection item {item_ordinal} page ended at {actual}, beyond declared length {declared}"
    )]
    ItemPastDeclaredEnd {
        item_ordinal: u64,
        declared: u64,
        actual: u64,
    },
    #[error("thread-injection item {item_ordinal} terminal flag disagreed with its exact end")]
    ItemTerminalMismatch { item_ordinal: u64 },
    #[error("thread-injection item {item_ordinal} sequence-terminal flag was not exact")]
    SequenceTerminalMismatch { item_ordinal: u64 },
    #[error("thread-injection sequence structure disagreed with preflight: {source}")]
    Sequence {
        #[source]
        source: RecoveryItemSequenceError,
    },
    #[error("thread-injection sequence digest did not match preflight")]
    SequenceDigestMismatch {
        expected: RecoveryItemSequenceDigest,
        actual: RecoveryItemSequenceDigest,
    },
}

impl From<RecoveryItemSequenceError> for ThreadInjectionSourceError {
    fn from(source: RecoveryItemSequenceError) -> Self {
        Self::Sequence { source }
    }
}

use beryl_model::{CasThreadId, RecoveryItemSequenceDigest};
use thiserror::Error;

use crate::{JsonRpcError, LoadedThreadSession, ManagedBackendError};

mod source;
mod wire;

pub use source::{
    ThreadInjectionRole, ThreadInjectionSource, ThreadInjectionSourceError,
    ThreadInjectionSourceIdentity, ThreadInjectionSourcePage, ThreadInjectionSourceRevision,
};
pub(crate) use wire::{
    ThreadInjectItemsParams, ThreadInjectionSourceFailureSlot, ThreadInjectionWriteFailure,
    write_injection_source_json,
};

/// Maximum number of messages in one normalized thread-injection sequence.
pub const THREAD_INJECTION_MAX_ITEMS: u64 = 262_144;

/// Maximum canonical UTF-8 text bytes in one thread-injection sequence.
pub const THREAD_INJECTION_MAX_TEXT_BYTES: u64 = 262_144;

/// Maximum valid UTF-8 bytes requested in one source page.
pub const THREAD_INJECTION_MAX_PAGE_BYTES: usize = 64 * 1024;

/// Compact proof frozen before one recovery sequence is encoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadInjectionPreflight {
    source_identity: ThreadInjectionSourceIdentity,
    source_revision: ThreadInjectionSourceRevision,
    item_count: u64,
    canonical_utf8_bytes: u64,
    sequence_digest: RecoveryItemSequenceDigest,
}

/// Pre-dispatch rejection while constructing a compact injection preflight.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ThreadInjectionPreflightError {
    /// A recovery injection must contain at least one item.
    #[error("thread-injection preflight must contain at least one item")]
    Empty,
    /// The exact item count exceeded the supported semantic limit.
    #[error("thread-injection preflight has {actual} items; the maximum is {maximum}")]
    TooManyItems { actual: u64, maximum: u64 },
    /// Nonempty recovery items require a nonzero canonical byte total.
    #[error("thread-injection preflight must contain canonical UTF-8 bytes")]
    EmptyCanonicalUtf8,
    /// The exact canonical byte total exceeded the supported semantic limit.
    #[error(
        "thread-injection preflight has {actual} canonical UTF-8 bytes; the maximum is {maximum}"
    )]
    TooManyCanonicalUtf8Bytes { actual: u64, maximum: u64 },
}

impl ThreadInjectionPreflight {
    /// Validates exact nonzero totals while retaining compact replay authority.
    pub fn new(
        source_identity: ThreadInjectionSourceIdentity,
        source_revision: ThreadInjectionSourceRevision,
        item_count: u64,
        canonical_utf8_bytes: u64,
        sequence_digest: RecoveryItemSequenceDigest,
    ) -> Result<Self, ThreadInjectionPreflightError> {
        if item_count == 0 {
            return Err(ThreadInjectionPreflightError::Empty);
        }
        if item_count > THREAD_INJECTION_MAX_ITEMS {
            return Err(ThreadInjectionPreflightError::TooManyItems {
                actual: item_count,
                maximum: THREAD_INJECTION_MAX_ITEMS,
            });
        }
        if canonical_utf8_bytes == 0 {
            return Err(ThreadInjectionPreflightError::EmptyCanonicalUtf8);
        }
        if canonical_utf8_bytes > THREAD_INJECTION_MAX_TEXT_BYTES {
            return Err(ThreadInjectionPreflightError::TooManyCanonicalUtf8Bytes {
                actual: canonical_utf8_bytes,
                maximum: THREAD_INJECTION_MAX_TEXT_BYTES,
            });
        }
        Ok(Self {
            source_identity,
            source_revision,
            item_count,
            canonical_utf8_bytes,
            sequence_digest,
        })
    }

    /// Returns the immutable source identity frozen by preflight.
    #[must_use]
    pub const fn source_identity(self) -> ThreadInjectionSourceIdentity {
        self.source_identity
    }

    /// Returns the immutable source revision frozen by preflight.
    #[must_use]
    pub const fn source_revision(self) -> ThreadInjectionSourceRevision {
        self.source_revision
    }

    /// Returns the exact nonzero item total.
    #[must_use]
    pub const fn item_count(self) -> u64 {
        self.item_count
    }

    /// Returns the exact nonzero canonical UTF-8 byte total.
    #[must_use]
    pub const fn canonical_utf8_bytes(self) -> u64 {
        self.canonical_utf8_bytes
    }

    /// Returns the exact shared V1 recovery-sequence digest.
    #[must_use]
    pub const fn sequence_digest(self) -> RecoveryItemSequenceDigest {
        self.sequence_digest
    }
}

/// Normalized structured rejection for one thread-injection request.
///
/// The raw JSON-RPC error data is deliberately not exposed. Callers receive
/// the stable numeric code, human-readable message, and whether CAS supplied
/// additional private diagnostic data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadInjectionRejection {
    code: i64,
    message: Box<str>,
    message_was_truncated: bool,
    data_was_present: bool,
}

/// Terminal outcome of one consumed fresh-thread injection attempt.
///
/// Every variant consumes the fresh-idle capability. Callers must never retry
/// an unsuccessful attempt against the same CAS thread.
#[derive(Debug)]
pub enum ThreadInjectionOutcome {
    /// CAS returned the exact successful response after source revalidation.
    Succeeded { thread: LoadedThreadSession },
    /// CAS returned one matching structured JSON-RPC rejection.
    Rejected {
        thread_id: CasThreadId,
        rejection: ThreadInjectionRejection,
    },
    /// Local evidence proves no request or source byte was offered for dispatch.
    ///
    /// The target is still consumed because an injection attempt never returns fresh-thread
    /// authority to its caller.
    ProvenNotDispatched {
        thread_id: CasThreadId,
        error: Box<ManagedBackendError>,
    },
    /// A concrete transport failure prevented exact request completion.
    TransportLost {
        thread_id: CasThreadId,
        error: Box<ManagedBackendError>,
    },
    /// Source disagreement or another failure prevented exact completion proof.
    CompletionUnknown {
        thread_id: CasThreadId,
        error: Box<ManagedBackendError>,
    },
}

impl ThreadInjectionRejection {
    /// Returns the exact JSON-RPC error code supplied by CAS.
    #[must_use]
    pub const fn code(&self) -> i64 {
        self.code
    }

    /// Returns the bounded diagnostic projection supplied by backend ingress.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Reports whether the decoded diagnostic continued beyond the retained projection.
    #[must_use]
    pub const fn message_was_truncated(&self) -> bool {
        self.message_was_truncated
    }

    /// Reports whether CAS supplied additional raw diagnostic data.
    #[must_use]
    pub const fn data_was_present(&self) -> bool {
        self.data_was_present
    }

    pub(crate) fn from_json_rpc(error: JsonRpcError) -> Self {
        Self {
            code: error.code(),
            message: error.message().into(),
            message_was_truncated: error.message_was_truncated(),
            data_was_present: error.data_was_present(),
        }
    }
}

/// Exact stable Codex App Server method for normalized item injection.
pub(crate) const THREAD_INJECT_ITEMS_METHOD: &str = "thread/inject_items";

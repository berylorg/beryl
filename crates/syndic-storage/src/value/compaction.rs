use std::num::NonZeroU64;

use beryl_model::{SyndicItemId, SyndicThreadId, SyndicTurnId};

use super::SyndicValueError;

macro_rules! compaction_nonce {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; 16]);

        impl $name {
            #[must_use]
            pub const fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(bytes)
            }

            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }
        }
    };
}

compaction_nonce!(
    /// Caller-owned natural identity of one durable context-compaction operation.
    CompactionOperationNonce
);
compaction_nonce!(
    /// Caller-owned identity of the sole compact-start request attempt.
    CompactionAttemptNonce
);

/// SHA-256 commitment to one canonical V1 compaction settlement receipt.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompactionSettlementReceiptCommitment([u8; 32]);

impl CompactionSettlementReceiptCommitment {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Durable compaction identity, unique within one Syndic thread.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompactionOperationId {
    thread_id: SyndicThreadId,
    nonce: CompactionOperationNonce,
}

impl CompactionOperationId {
    #[must_use]
    pub const fn new(thread_id: SyndicThreadId, nonce: CompactionOperationNonce) -> Self {
        Self { thread_id, nonce }
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    #[must_use]
    pub const fn nonce(self) -> CompactionOperationNonce {
        self.nonce
    }

    /// Returns the required parentless provider-operation turn identity.
    #[must_use]
    pub const fn provider_turn_id(self) -> SyndicTurnId {
        SyndicTurnId::from_bytes(*self.nonce.as_bytes())
    }
}

macro_rules! nonzero_compaction_value {
    ($(#[$meta:meta])* $name:ident, $kind:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(NonZeroU64);

        impl $name {
            pub const FIRST: Self = Self(NonZeroU64::MIN);

            pub fn new(value: u64) -> Result<Self, SyndicValueError> {
                NonZeroU64::new(value).map(Self).ok_or(SyndicValueError::ZeroOrdinal {
                    kind: $kind,
                })
            }

            #[must_use]
            pub const fn get(self) -> u64 {
                self.0.get()
            }

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

nonzero_compaction_value!(
    /// Monotonic revision of one retained compaction-operation record.
    CompactionOperationRevision,
    "compaction-operation revision"
);
nonzero_compaction_value!(
    /// Exact order of one normalized provider observation for a compaction operation.
    CompactionProviderSequence,
    "compaction provider sequence"
);

/// Pinned thread status retained in exact provider order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompactionThreadStatus {
    Active,
    Idle,
    SystemError,
}

/// Request-loop disposition retained independently from provider observations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompactionRequestDisposition {
    Accepted,
    RejectedBeforeCore,
    ProvenLocalNondispatch,
    CompletionUnknown,
}

/// Lifecycle of the one context-compaction marker item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompactionMarkerLifecycle {
    Started,
    Completed,
}

/// Exact durable marker frontier selected by the operation record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionMarkerObservation {
    sequence: CompactionProviderSequence,
    item_id: SyndicItemId,
    lifecycle: CompactionMarkerLifecycle,
}

impl CompactionMarkerObservation {
    #[must_use]
    pub const fn new(
        sequence: CompactionProviderSequence,
        item_id: SyndicItemId,
        lifecycle: CompactionMarkerLifecycle,
    ) -> Self {
        Self {
            sequence,
            item_id,
            lifecycle,
        }
    }

    #[must_use]
    pub const fn sequence(&self) -> CompactionProviderSequence {
        self.sequence
    }

    #[must_use]
    pub const fn item_id(&self) -> SyndicItemId {
        self.item_id
    }

    #[must_use]
    pub const fn lifecycle(&self) -> CompactionMarkerLifecycle {
        self.lifecycle
    }
}

/// Why live compaction authority was conservatively consumed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompactionAbandonmentReason {
    ProviderRejectedBeforeCore,
    CompletionUnknown,
    TargetAuthorityLost,
    StartupProcessGenerationLost,
    ProviderProtocolConflict,
}

/// Closed durable result of consuming one compaction operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactionSettlement {
    CancelledBeforeDispatch,
    LocalNondispatch,
    Abandoned(CompactionAbandonmentReason),
    ManualSuccess,
    ManualFailure,
    LifecycleUserWorkWon,
    LifecycleContinuation {
        turn_id: SyndicTurnId,
        item_id: SyndicItemId,
        content_id: beryl_model::SyndicContentId,
    },
}

use std::num::NonZeroU64;

use beryl_model::SyndicThreadId;

use super::SyndicValueError;

macro_rules! stop_nonce {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; 16]);

        impl $name {
            /// Constructs one caller-owned nonce from its exact random bytes.
            #[must_use]
            pub const fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(bytes)
            }

            /// Returns the exact caller-owned nonce bytes.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }
        }
    };
}

stop_nonce!(
    /// Caller-owned natural identity of one durable stop operation.
    StopOperationNonce
);
stop_nonce!(
    /// Caller-owned natural identity of one dispatch attempt within a stop operation.
    StopAttemptNonce
);

/// Durable stop identity, unique within one Syndic thread.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StopOperationId {
    thread_id: SyndicThreadId,
    nonce: StopOperationNonce,
}

impl StopOperationId {
    /// Combines the owning thread with the caller-owned operation nonce.
    #[must_use]
    pub const fn new(thread_id: SyndicThreadId, nonce: StopOperationNonce) -> Self {
        Self { thread_id, nonce }
    }

    /// Returns the owning Syndic thread.
    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }

    /// Returns the caller-owned operation nonce.
    #[must_use]
    pub const fn nonce(self) -> StopOperationNonce {
        self.nonce
    }
}

/// Monotonic revision of one retained stop-operation record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StopOperationRevision(NonZeroU64);

impl StopOperationRevision {
    /// First valid stop-operation revision.
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    /// Constructs an exact revision, rejecting reserved zero.
    pub fn new(value: u64) -> Result<Self, SyndicValueError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(SyndicValueError::ZeroOrdinal {
                kind: "stop-operation revision",
            })
    }

    /// Returns the integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// Advances one revision without wrapping.
    pub fn checked_next(self) -> Result<Self, SyndicValueError> {
        self.get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(SyndicValueError::OrdinalExhausted {
                kind: "stop-operation revision",
            })
    }
}

/// Closed durable reason for joining one exact stop operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StopCause {
    /// Deliberate control of the currently selected provider operation.
    SelectedOperationControl,
    /// Diagnostic control of the currently selected provider operation.
    DiagnosticControl,
    /// Healthy-home ownership released while closing a window.
    HealthyHomeWindowClose,
    /// Beryl-owned interruption required by an approval decision.
    InterruptingApproval,
}

impl StopCause {
    /// All causes in the fixed persisted slot order.
    pub const ALL: [Self; 4] = [
        Self::SelectedOperationControl,
        Self::DiagnosticControl,
        Self::HealthyHomeWindowClose,
        Self::InterruptingApproval,
    ];

    #[must_use]
    const fn bit(self) -> u8 {
        match self {
            Self::SelectedOperationControl => 1 << 0,
            Self::DiagnosticControl => 1 << 1,
            Self::HealthyHomeWindowClose => 1 << 2,
            Self::InterruptingApproval => 1 << 3,
        }
    }
}

/// Why persisted stop-cause bits are not one canonical nonempty closed set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum StopCauseSetError {
    /// A stop operation must retain at least one durable cause.
    #[error("stop cause set must not be empty")]
    Empty,
    /// Only the low four cause bits are defined.
    #[error("stop cause set contains unknown bits in {bits:#010b}")]
    UnknownBits { bits: u8 },
}

/// Nonempty canonical set of the four closed durable stop causes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StopCauseSet(u8);

impl StopCauseSet {
    /// All currently defined cause bits.
    pub const ALL_BITS: u8 = 0b0000_1111;

    /// Constructs a singleton set.
    #[must_use]
    pub const fn from_cause(cause: StopCause) -> Self {
        Self(cause.bit())
    }

    /// Validates one canonical persisted bit representation.
    pub const fn from_bits(bits: u8) -> Result<Self, StopCauseSetError> {
        if bits == 0 {
            return Err(StopCauseSetError::Empty);
        }
        if bits & !Self::ALL_BITS != 0 {
            return Err(StopCauseSetError::UnknownBits { bits });
        }
        Ok(Self(bits))
    }

    /// Returns the canonical persisted bit representation.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Reports whether this set contains one exact cause.
    #[must_use]
    pub const fn contains(self, cause: StopCause) -> bool {
        self.0 & cause.bit() != 0
    }

    /// Monotonically adds one exact cause.
    #[must_use]
    pub const fn with(self, cause: StopCause) -> Self {
        Self(self.0 | cause.bit())
    }

    /// Monotonically joins two nonempty cause sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl From<StopCause> for StopCauseSet {
    fn from(cause: StopCause) -> Self {
        Self::from_cause(cause)
    }
}

/// Fixed first-publication revision slots for the closed stop-cause set.
///
/// Each present cause retains the exact record revision that first published it. Multiple causes
/// admitted together share [`StopOperationRevision::FIRST`]; a later join owns one distinct
/// successor revision. Absence is represented by an empty typed slot and is encoded as zero.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StopCauseFirstRevisions {
    selected_operation_control: Option<StopOperationRevision>,
    diagnostic_control: Option<StopOperationRevision>,
    healthy_home_window_close: Option<StopOperationRevision>,
    interrupting_approval: Option<StopOperationRevision>,
}

/// Why fixed stop-cause first-publication slots are not valid admission provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum StopCauseFirstRevisionsError {
    /// At least one cause must have been published at admission revision one.
    #[error("stop cause-first revisions require at least one cause at revision one")]
    MissingAdmissionCause,
}

impl StopCauseFirstRevisions {
    /// Constructs the four fixed slots in [`StopCause::ALL`] order.
    pub fn new(
        selected_operation_control: Option<StopOperationRevision>,
        diagnostic_control: Option<StopOperationRevision>,
        healthy_home_window_close: Option<StopOperationRevision>,
        interrupting_approval: Option<StopOperationRevision>,
    ) -> Result<Self, StopCauseFirstRevisionsError> {
        let revisions = Self {
            selected_operation_control,
            diagnostic_control,
            healthy_home_window_close,
            interrupting_approval,
        };
        if StopCause::ALL
            .into_iter()
            .any(|cause| revisions.first_revision(cause) == Some(StopOperationRevision::FIRST))
        {
            Ok(revisions)
        } else {
            Err(StopCauseFirstRevisionsError::MissingAdmissionCause)
        }
    }

    /// Constructs admission provenance, assigning every initial cause to revision one.
    #[must_use]
    pub const fn for_admission(causes: StopCauseSet) -> Self {
        Self {
            selected_operation_control: if causes.contains(StopCause::SelectedOperationControl) {
                Some(StopOperationRevision::FIRST)
            } else {
                None
            },
            diagnostic_control: if causes.contains(StopCause::DiagnosticControl) {
                Some(StopOperationRevision::FIRST)
            } else {
                None
            },
            healthy_home_window_close: if causes.contains(StopCause::HealthyHomeWindowClose) {
                Some(StopOperationRevision::FIRST)
            } else {
                None
            },
            interrupting_approval: if causes.contains(StopCause::InterruptingApproval) {
                Some(StopOperationRevision::FIRST)
            } else {
                None
            },
        }
    }

    /// Returns the immutable first-publication revision for one cause.
    #[must_use]
    pub const fn first_revision(self, cause: StopCause) -> Option<StopOperationRevision> {
        match cause {
            StopCause::SelectedOperationControl => self.selected_operation_control,
            StopCause::DiagnosticControl => self.diagnostic_control,
            StopCause::HealthyHomeWindowClose => self.healthy_home_window_close,
            StopCause::InterruptingApproval => self.interrupting_approval,
        }
    }

    /// Derives the nonempty aggregate convenience view from the persisted slots.
    #[must_use]
    pub fn causes(self) -> StopCauseSet {
        let mut bits = 0;
        for cause in StopCause::ALL {
            if self.first_revision(cause).is_some() {
                bits |= cause.bit();
            }
        }
        StopCauseSet::from_bits(bits)
            .expect("validated stop cause-first revisions always contain an admission cause")
    }

    /// Derives exactly the causes first published by admission.
    #[must_use]
    pub fn admission_causes(self) -> StopCauseSet {
        let mut bits = 0;
        for cause in StopCause::ALL {
            if self.first_revision(cause) == Some(StopOperationRevision::FIRST) {
                bits |= cause.bit();
            }
        }
        StopCauseSet::from_bits(bits)
            .expect("validated stop cause-first revisions always contain an admission cause")
    }

    pub(crate) fn publish(
        mut self,
        cause: StopCause,
        revision: StopOperationRevision,
    ) -> Option<Self> {
        let slot = match cause {
            StopCause::SelectedOperationControl => &mut self.selected_operation_control,
            StopCause::DiagnosticControl => &mut self.diagnostic_control,
            StopCause::HealthyHomeWindowClose => &mut self.healthy_home_window_close,
            StopCause::InterruptingApproval => &mut self.interrupting_approval,
        };
        if slot.is_some() {
            return None;
        }
        *slot = Some(revision);
        Some(self)
    }
}

/// Immutable provenance for the sole durable stop-dispatch claim.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StopDispatchClaimWitness {
    source_revision: StopOperationRevision,
    attempt: StopAttemptNonce,
}

impl StopDispatchClaimWitness {
    /// Captures the exact live source revision and caller-owned attempt identity.
    #[must_use]
    pub const fn new(source_revision: StopOperationRevision, attempt: StopAttemptNonce) -> Self {
        Self {
            source_revision,
            attempt,
        }
    }

    /// Returns the live record revision consumed by this claim.
    #[must_use]
    pub const fn source_revision(self) -> StopOperationRevision {
        self.source_revision
    }

    /// Returns the caller-owned identity of the sole dispatch attempt.
    #[must_use]
    pub const fn attempt(self) -> StopAttemptNonce {
        self.attempt
    }
}

/// Closed provenance for conservatively abandoning one live stop operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StopAbandonmentReason {
    /// The provider proved rejection before core interruption without preserving target authority.
    ProviderRejectedBeforeCoreInterrupt,
    /// The live connection or exact target authority was lost.
    TargetAuthorityLost,
    /// Startup cannot recover the old managed-process generation.
    StartupProcessGenerationLost,
}

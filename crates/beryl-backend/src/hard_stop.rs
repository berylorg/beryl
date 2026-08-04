use std::fmt;

use beryl_model::{CasLoadedSessionGeneration, CasThreadId, CasTurnId, RuntimeId};

use crate::{ManagedBackendError, exact_interruption::ExactForegroundTurnAuthorizationCore};

/// Opaque local correlation for one durable stop operation.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct StopOperationCorrelation([u8; 16]);

/// Opaque local correlation for one claimed dispatch attempt.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct StopAttemptCorrelation([u8; 16]);

/// Durable state of the sole claimed stop attempt when approval ownership is reported.
///
/// This is correlation evidence, not a dispatch capability. Only an exact foreground
/// authorization can consume the corresponding attempt on the authenticated session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopAttemptDisposition {
    /// The attempt is durably claimed and no request byte has crossed the transport.
    ClaimedNotDispatched(StopAttemptCorrelation),
    /// At least one request byte may already have crossed the transport.
    PossiblyDispatched(StopAttemptCorrelation),
}

/// Exact foreground CAS turn selected by the caller's target election.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactForegroundTurn {
    runtime_id: RuntimeId,
    loaded_session_generation: CasLoadedSessionGeneration,
    thread_id: CasThreadId,
    turn_id: CasTurnId,
}

/// Caller-issued proof that its target-operation election excludes a successor.
///
/// The witness is deliberately non-cloneable. Calling [`Self::issue`] attests that the
/// caller currently owns the outer election which prevents a successor turn or compaction
/// start across the request cut. Raw target identities do not establish this fact, and this
/// crate cannot reconstruct it.
#[derive(Debug, Eq, PartialEq)]
pub struct CallerNoSuccessorFence {
    _private: (),
}

/// Non-cloneable authorization for one exact foreground turn operation.
#[derive(Debug)]
pub struct ExactForegroundTurnAuthorization {
    core: ExactForegroundTurnAuthorizationCore,
    operation: StopOperationCorrelation,
    attempt: StopAttemptCorrelation,
}

/// Returned exact local identity of one attempted foreground operation.
#[derive(Debug)]
pub struct ExactForegroundTurnRequest {
    target: ExactForegroundTurn,
    operation: StopOperationCorrelation,
    attempt: StopAttemptCorrelation,
    _fence: CallerNoSuccessorFence,
}

/// Closed disposition of one exact `turn/interrupt` request.
#[derive(Debug)]
pub enum TurnInterruptDisposition {
    /// CAS returned the matching empty response; lifecycle remains separately observed.
    RequestAccepted,
    /// Pinned CAS proved that the handler did not enqueue the core interrupt.
    RejectedBeforeCoreInterrupt,
    /// Local byte-level evidence proves that no request byte crossed the transport.
    ProvenNotDispatched { error: Box<ManagedBackendError> },
    /// At least one byte may have crossed and no authoritative completion survived.
    CompletionUnknown { error: Box<ManagedBackendError> },
}

/// Exact correlation-bearing outcome of one `turn/interrupt` attempt.
#[must_use = "turn interruption outcomes must be reconciled by exact correlation"]
#[derive(Debug)]
pub struct TurnInterruptOutcome {
    request: ExactForegroundTurnRequest,
    disposition: TurnInterruptDisposition,
}

/// Same-session causal ordering fact returned by accepted coarse cleanup.
///
/// This fact proves only that pinned core accepted cleanup before a later Beryl operation
/// submitted through the same surviving loaded session. It is not cleanup completion and
/// cannot be transferred to a replacement session.
#[derive(Debug, Eq, PartialEq)]
pub struct SameSessionCleanupOrdering {
    session_authority_generation: u64,
}

/// Closed disposition of one optional coarse background-terminal cleanup request.
#[derive(Debug)]
pub enum CoarseThreadCleanupDisposition {
    /// CAS accepted the request and established the contained same-session ordering fact.
    RequestAccepted {
        ordering: SameSessionCleanupOrdering,
    },
    /// Local byte-level evidence proves that no request byte crossed the transport.
    ProvenNotDispatched { error: Box<ManagedBackendError> },
    /// A matching provider rejection invalidated the exact loaded-session authority.
    SessionAuthorityInvalidated { error: Box<ManagedBackendError> },
    /// Dispatch or completion is ambiguous; the exact session was retired before return.
    CompletionUnknown { error: Box<ManagedBackendError> },
}

/// Exact correlation-bearing outcome of coarse thread background-terminal cleanup.
#[must_use = "coarse cleanup outcomes must be reconciled by exact correlation"]
#[derive(Debug)]
pub struct CoarseThreadCleanupOutcome {
    request: ExactForegroundTurnRequest,
    disposition: CoarseThreadCleanupDisposition,
}

/// Source-pinned exact hard-target limitations for Codex App Server 0.146.0.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactHardStopLimitation {
    /// Child/subagent core interruption is untargeted and cannot fence internal successors.
    ChildOrSubagentInterruptionUnsupported,
    /// A reusable numeric process id is not an exact process-instance identity.
    IndividualTurnProcessTerminationIdentityUnsafe,
}

impl StopOperationCorrelation {
    /// Constructs the local correlation from the durable operation's opaque 128-bit nonce.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the exact opaque bytes without assigning provider semantics.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl StopAttemptCorrelation {
    /// Constructs the local correlation from the durable attempt's opaque 128-bit nonce.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the exact opaque bytes without assigning provider semantics.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl StopAttemptDisposition {
    /// Returns the durable correlation of the sole claimed attempt.
    #[must_use]
    pub const fn correlation(self) -> StopAttemptCorrelation {
        match self {
            Self::ClaimedNotDispatched(correlation) | Self::PossiblyDispatched(correlation) => {
                correlation
            }
        }
    }

    /// Returns whether any request byte from the attempt may already have crossed.
    #[must_use]
    pub const fn may_have_dispatched(self) -> bool {
        matches!(self, Self::PossiblyDispatched(_))
    }
}

impl fmt::Debug for StopOperationCorrelation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StopOperationCorrelation([opaque; 16])")
    }
}

impl fmt::Debug for StopAttemptCorrelation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StopAttemptCorrelation([opaque; 16])")
    }
}

impl ExactForegroundTurn {
    /// Binds the exact Beryl runtime, loaded generation, CAS thread, and CAS turn.
    #[must_use]
    pub const fn new(
        runtime_id: RuntimeId,
        loaded_session_generation: CasLoadedSessionGeneration,
        thread_id: CasThreadId,
        turn_id: CasTurnId,
    ) -> Self {
        Self {
            runtime_id,
            loaded_session_generation,
            thread_id,
            turn_id,
        }
    }

    #[must_use]
    pub const fn runtime_id(&self) -> RuntimeId {
        self.runtime_id
    }

    #[must_use]
    pub const fn loaded_session_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_session_generation
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

impl CallerNoSuccessorFence {
    /// Issues the witness while the caller owns the outer no-successor election.
    #[must_use]
    pub const fn issue() -> Self {
        Self { _private: () }
    }
}

impl ExactForegroundTurnAuthorization {
    pub(crate) fn new(
        target: ExactForegroundTurn,
        operation: StopOperationCorrelation,
        attempt: StopAttemptCorrelation,
        fence: CallerNoSuccessorFence,
        session_authority_generation: u64,
        authorization_epoch: u64,
    ) -> Self {
        Self {
            core: ExactForegroundTurnAuthorizationCore::new(
                target,
                fence,
                session_authority_generation,
                authorization_epoch,
            ),
            operation,
            attempt,
        }
    }

    pub(crate) const fn core(&self) -> &ExactForegroundTurnAuthorizationCore {
        &self.core
    }

    #[must_use]
    pub const fn target(&self) -> &ExactForegroundTurn {
        self.core.target()
    }

    #[must_use]
    pub const fn operation_correlation(&self) -> StopOperationCorrelation {
        self.operation
    }

    #[must_use]
    pub const fn attempt_correlation(&self) -> StopAttemptCorrelation {
        self.attempt
    }

    pub(crate) fn into_request(self) -> ExactForegroundTurnRequest {
        let (target, fence) = self.core.into_request_parts();
        ExactForegroundTurnRequest {
            target,
            operation: self.operation,
            attempt: self.attempt,
            _fence: fence,
        }
    }
}

impl ExactForegroundTurnRequest {
    #[must_use]
    pub const fn target(&self) -> &ExactForegroundTurn {
        &self.target
    }

    #[must_use]
    pub const fn operation_correlation(&self) -> StopOperationCorrelation {
        self.operation
    }

    #[must_use]
    pub const fn attempt_correlation(&self) -> StopAttemptCorrelation {
        self.attempt
    }

    /// Confirms that the consumed request carried the explicit caller witness.
    #[must_use]
    pub const fn had_no_successor_fence(&self) -> bool {
        true
    }
}

impl TurnInterruptOutcome {
    pub(crate) const fn new(
        request: ExactForegroundTurnRequest,
        disposition: TurnInterruptDisposition,
    ) -> Self {
        Self {
            request,
            disposition,
        }
    }

    #[must_use]
    pub const fn request(&self) -> &ExactForegroundTurnRequest {
        &self.request
    }

    #[must_use]
    pub const fn disposition(&self) -> &TurnInterruptDisposition {
        &self.disposition
    }
}

impl SameSessionCleanupOrdering {
    pub(crate) const fn new(session_authority_generation: u64) -> Self {
        Self {
            session_authority_generation,
        }
    }

    /// Returns only an opaque equality token for local same-session comparison.
    #[must_use]
    pub const fn session_token(&self) -> u64 {
        self.session_authority_generation
    }
}

impl CoarseThreadCleanupOutcome {
    pub(crate) const fn new(
        request: ExactForegroundTurnRequest,
        disposition: CoarseThreadCleanupDisposition,
    ) -> Self {
        Self {
            request,
            disposition,
        }
    }

    #[must_use]
    pub const fn request(&self) -> &ExactForegroundTurnRequest {
        &self.request
    }

    #[must_use]
    pub const fn disposition(&self) -> &CoarseThreadCleanupDisposition {
        &self.disposition
    }
}

impl ExactHardStopLimitation {
    /// Returns the two fixed limitations of the pinned release without target handles.
    #[must_use]
    pub const fn pinned() -> [Self; 2] {
        [
            Self::ChildOrSubagentInterruptionUnsupported,
            Self::IndividualTurnProcessTerminationIdentityUnsafe,
        ]
    }
}

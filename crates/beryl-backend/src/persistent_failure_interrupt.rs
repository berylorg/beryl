use std::fmt;

use crate::{
    CallerNoSuccessorFence, ExactForegroundTurn, TurnInterruptDisposition,
    exact_interruption::ExactForegroundTurnAuthorizationCore,
};

/// Opaque process-local correlation for one volatile persistent-failure interrupt attempt.
///
/// This value is neither durable stop identity nor provider idempotency and is never sent on the
/// wire.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct PersistentFailureInterruptCorrelation([u8; 16]);

/// Non-cloneable authorization for one volatile exact-turn persistent-failure interrupt.
///
/// This capability cannot authorize a durable stop or coarse cleanup request.
#[derive(Debug)]
pub struct PersistentFailureInterruptAuthorization {
    core: ExactForegroundTurnAuthorizationCore,
    correlation: PersistentFailureInterruptCorrelation,
}

/// Returned local identity of one volatile persistent-failure interrupt request.
#[derive(Debug)]
pub struct PersistentFailureInterruptRequest {
    target: ExactForegroundTurn,
    correlation: PersistentFailureInterruptCorrelation,
    _fence: CallerNoSuccessorFence,
}

/// Correlation-bearing outcome of one volatile persistent-failure interrupt attempt.
///
/// The outcome is diagnostics-only. It supplies no durable receipt, retry authority, lifecycle
/// completion, failure-generation proof, or target-selection claim.
#[must_use = "persistent-failure interruption outcomes must be retained by exact correlation"]
#[derive(Debug)]
pub struct PersistentFailureInterruptOutcome {
    request: PersistentFailureInterruptRequest,
    disposition: TurnInterruptDisposition,
}

impl PersistentFailureInterruptCorrelation {
    /// Constructs one process-local opaque correlation.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the exact opaque bytes without assigning provider or durable semantics.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Debug for PersistentFailureInterruptCorrelation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PersistentFailureInterruptCorrelation([opaque; 16])")
    }
}

impl PersistentFailureInterruptAuthorization {
    pub(crate) const fn new(
        target: ExactForegroundTurn,
        correlation: PersistentFailureInterruptCorrelation,
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
            correlation,
        }
    }

    pub(crate) const fn core(&self) -> &ExactForegroundTurnAuthorizationCore {
        &self.core
    }

    pub(crate) fn into_request(self) -> PersistentFailureInterruptRequest {
        let (target, fence) = self.core.into_request_parts();
        PersistentFailureInterruptRequest {
            target,
            correlation: self.correlation,
            _fence: fence,
        }
    }

    /// Returns the exact target bound into this volatile authorization.
    #[must_use]
    pub const fn target(&self) -> &ExactForegroundTurn {
        self.core.target()
    }

    /// Returns the process-local correlation without granting durable semantics.
    #[must_use]
    pub const fn correlation(&self) -> PersistentFailureInterruptCorrelation {
        self.correlation
    }
}

impl PersistentFailureInterruptRequest {
    /// Returns the exact target consumed by the request.
    #[must_use]
    pub const fn target(&self) -> &ExactForegroundTurn {
        &self.target
    }

    /// Returns the process-local correlation without granting durable semantics.
    #[must_use]
    pub const fn correlation(&self) -> PersistentFailureInterruptCorrelation {
        self.correlation
    }

    /// Confirms that the consumed request carried the explicit caller witness.
    #[must_use]
    pub const fn had_no_successor_fence(&self) -> bool {
        true
    }
}

impl PersistentFailureInterruptOutcome {
    pub(crate) const fn new(
        request: PersistentFailureInterruptRequest,
        disposition: TurnInterruptDisposition,
    ) -> Self {
        Self {
            request,
            disposition,
        }
    }

    /// Returns the consumed volatile request identity.
    #[must_use]
    pub const fn request(&self) -> &PersistentFailureInterruptRequest {
        &self.request
    }

    /// Returns the closed provider dispatch disposition.
    #[must_use]
    pub const fn disposition(&self) -> &TurnInterruptDisposition {
        &self.disposition
    }
}

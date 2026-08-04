use crate::hard_stop::{CallerNoSuccessorFence, ExactForegroundTurn};

/// Shared private session authority behind the non-interchangeable interrupt families.
///
/// Correlation and reconciliation identity deliberately stay in the public family wrappers. This
/// core contains only the exact target, caller fence, and session-local revocation coordinates
/// required by the common wire dispatcher.
#[derive(Debug)]
pub(crate) struct ExactForegroundTurnAuthorizationCore {
    target: ExactForegroundTurn,
    fence: CallerNoSuccessorFence,
    session_authority_generation: u64,
    authorization_epoch: u64,
}

impl ExactForegroundTurnAuthorizationCore {
    pub(crate) const fn new(
        target: ExactForegroundTurn,
        fence: CallerNoSuccessorFence,
        session_authority_generation: u64,
        authorization_epoch: u64,
    ) -> Self {
        Self {
            target,
            fence,
            session_authority_generation,
            authorization_epoch,
        }
    }

    pub(crate) const fn target(&self) -> &ExactForegroundTurn {
        &self.target
    }

    pub(crate) const fn session_authority_generation(&self) -> u64 {
        self.session_authority_generation
    }

    pub(crate) const fn authorization_epoch(&self) -> u64 {
        self.authorization_epoch
    }

    pub(crate) fn into_request_parts(self) -> (ExactForegroundTurn, CallerNoSuccessorFence) {
        (self.target, self.fence)
    }
}

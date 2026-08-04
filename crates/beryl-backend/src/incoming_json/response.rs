use thiserror::Error;

/// Closed response schema selected by the method that owns one serialized request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResponseFamily {
    Initialize,
    Compatibility(crate::CompatibilityProbe),
    ConfigRead,
    ModelList,
    ThreadStart,
    ThreadRead,
    ThreadResume,
    ThreadFork,
    ThreadRollback,
    ThreadInjectItems,
    ThreadUnsubscribe,
    TurnStart,
    TurnSteer,
    TurnInterrupt,
    ThreadCompactStart,
    ThreadBackgroundTerminalsClean,
}

impl ResponseFamily {
    pub(crate) const fn method(self) -> &'static str {
        match self {
            Self::Initialize => "initialize",
            Self::Compatibility(probe) => probe.method(),
            Self::ConfigRead => "config/read",
            Self::ModelList => "model/list",
            Self::ThreadStart => "thread/start",
            Self::ThreadRead => "thread/read",
            Self::ThreadResume => "thread/resume",
            Self::ThreadFork => "thread/fork",
            Self::ThreadRollback => "thread/rollback",
            Self::ThreadInjectItems => "thread/inject_items",
            Self::ThreadUnsubscribe => "thread/unsubscribe",
            Self::TurnStart => "turn/start",
            Self::TurnSteer => "turn/steer",
            Self::TurnInterrupt => "turn/interrupt",
            Self::ThreadCompactStart => "thread/compact/start",
            Self::ThreadBackgroundTerminalsClean => "thread/backgroundTerminals/clean",
        }
    }

    fn matches_result(self, result: &crate::BoundedResponseResult) -> bool {
        match (self, result) {
            (Self::Initialize, crate::BoundedResponseResult::Initialize(_))
            | (Self::ConfigRead, crate::BoundedResponseResult::ConfigRead(_))
            | (Self::ModelList, crate::BoundedResponseResult::ModelList(_))
            | (Self::ThreadStart, crate::BoundedResponseResult::ThreadStart(_))
            | (Self::ThreadRead, crate::BoundedResponseResult::ThreadRead(_))
            | (Self::ThreadResume, crate::BoundedResponseResult::ThreadResume(_))
            | (Self::ThreadFork, crate::BoundedResponseResult::ThreadFork(_))
            | (Self::TurnStart, crate::BoundedResponseResult::TurnStart(_))
            | (Self::TurnSteer, crate::BoundedResponseResult::TurnSteer(_))
            | (Self::ThreadUnsubscribe, crate::BoundedResponseResult::ThreadUnsubscribe(_)) => true,
            (
                Self::ThreadCompactStart,
                crate::BoundedResponseResult::EmptyAcknowledgement(
                    crate::EmptyAcknowledgement::ThreadCompactStart,
                ),
            )
            | (
                Self::ThreadInjectItems,
                crate::BoundedResponseResult::EmptyAcknowledgement(
                    crate::EmptyAcknowledgement::ThreadInjectItems,
                ),
            )
            | (
                Self::ThreadBackgroundTerminalsClean,
                crate::BoundedResponseResult::EmptyAcknowledgement(
                    crate::EmptyAcknowledgement::ThreadBackgroundTerminalsClean,
                ),
            )
            | (
                Self::TurnInterrupt,
                crate::BoundedResponseResult::EmptyAcknowledgement(
                    crate::EmptyAcknowledgement::TurnInterrupt,
                ),
            ) => true,
            (
                Self::Compatibility(expected),
                crate::BoundedResponseResult::Compatibility(actual),
            ) => actual.probe() == expected,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResponseExpectation {
    pub(crate) id: u64,
    pub(crate) family: ResponseFamily,
}

/// Installation and terminal-state failures for the sole response expectation.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(crate) enum ResponseExpectationError {
    #[error("a backend response expectation is already installed")]
    AlreadyInstalled,
    #[error("the backend response expectation is poisoned")]
    Poisoned,
}

enum ResponseExpectationState {
    Idle,
    Installed(ResponseExpectation),
    Poisoned,
}

/// Non-cloneable slot owned by one serialized session reader.
pub(crate) struct ResponseExpectationSlot {
    state: ResponseExpectationState,
}

impl std::fmt::Debug for ResponseExpectationSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match &self.state {
            ResponseExpectationState::Idle => "idle",
            ResponseExpectationState::Installed(_) => "installed",
            ResponseExpectationState::Poisoned => "poisoned",
        };
        formatter
            .debug_struct("ResponseExpectationSlot")
            .field("state", &state)
            .finish()
    }
}

impl Default for ResponseExpectationSlot {
    fn default() -> Self {
        Self {
            state: ResponseExpectationState::Idle,
        }
    }
}

impl ResponseExpectationSlot {
    pub(crate) fn install_fixed(
        &mut self,
        id: u64,
        family: ResponseFamily,
    ) -> Result<(), ResponseExpectationError> {
        self.install(id, family)
    }

    fn install(&mut self, id: u64, family: ResponseFamily) -> Result<(), ResponseExpectationError> {
        match &self.state {
            ResponseExpectationState::Idle => {
                self.state =
                    ResponseExpectationState::Installed(ResponseExpectation { id, family });
                Ok(())
            }
            ResponseExpectationState::Installed(_) => {
                Err(ResponseExpectationError::AlreadyInstalled)
            }
            ResponseExpectationState::Poisoned => Err(ResponseExpectationError::Poisoned),
        }
    }

    pub(crate) const fn current(&self) -> Option<ResponseExpectation> {
        match &self.state {
            ResponseExpectationState::Installed(expectation) => Some(*expectation),
            ResponseExpectationState::Idle | ResponseExpectationState::Poisoned => None,
        }
    }

    pub(crate) const fn is_idle(&self) -> bool {
        matches!(&self.state, ResponseExpectationState::Idle)
    }

    pub(crate) const fn is_poisoned(&self) -> bool {
        matches!(&self.state, ResponseExpectationState::Poisoned)
    }

    /// Cancels only the exact expectation after dispatch is proven not to have occurred.
    pub(crate) fn cancel(&mut self, id: u64) -> bool {
        let exact = matches!(
            &self.state,
            ResponseExpectationState::Installed(expectation) if expectation.id == id
        );
        if exact {
            self.state = ResponseExpectationState::Idle;
        }
        exact
    }

    pub(crate) fn complete_response(
        &mut self,
        id: u64,
        result: &crate::BoundedResponseResult,
    ) -> Result<(), super::ForegroundIngressError> {
        let state = std::mem::replace(&mut self.state, ResponseExpectationState::Poisoned);
        let ResponseExpectationState::Installed(expectation) = state else {
            return Err(match state {
                ResponseExpectationState::Idle => super::ForegroundIngressError::IdleResponse,
                ResponseExpectationState::Poisoned => {
                    super::ForegroundIngressError::MalformedResponse
                }
                ResponseExpectationState::Installed(_) => unreachable!(),
            });
        };
        if expectation.id != id {
            return Err(super::ForegroundIngressError::ResponseIdMismatch {
                expected: expectation.id,
                actual: Some(id),
            });
        }
        if !expectation.family.matches_result(result) {
            return Err(super::ForegroundIngressError::MalformedResponse);
        }
        self.state = ResponseExpectationState::Idle;
        Ok(())
    }

    pub(crate) fn complete_rejection(
        &mut self,
        id: u64,
    ) -> Result<(), super::ForegroundIngressError> {
        let state = std::mem::replace(&mut self.state, ResponseExpectationState::Poisoned);
        let ResponseExpectationState::Installed(expectation) = state else {
            return Err(match state {
                ResponseExpectationState::Idle => super::ForegroundIngressError::IdleResponse,
                ResponseExpectationState::Poisoned => {
                    super::ForegroundIngressError::MalformedResponse
                }
                ResponseExpectationState::Installed(_) => unreachable!(),
            });
        };
        if expectation.id != id {
            return Err(super::ForegroundIngressError::ResponseIdMismatch {
                expected: expectation.id,
                actual: Some(id),
            });
        }
        self.state = ResponseExpectationState::Idle;
        Ok(())
    }

    pub(crate) fn complete_unavailable(
        &mut self,
        id: u64,
    ) -> Result<(), super::ForegroundIngressError> {
        let state = std::mem::replace(&mut self.state, ResponseExpectationState::Poisoned);
        let ResponseExpectationState::Installed(expectation) = state else {
            return Err(super::ForegroundIngressError::MalformedResponse);
        };
        if expectation.id != id {
            return Err(super::ForegroundIngressError::MalformedResponse);
        }
        self.state = ResponseExpectationState::Idle;
        Ok(())
    }

    pub(crate) fn poison(&mut self) {
        if !matches!(&self.state, ResponseExpectationState::Idle) {
            self.state = ResponseExpectationState::Poisoned;
        }
    }

    #[cfg(feature = "lifecycle-test-support")]
    pub(crate) fn poison_for_test(&mut self) {
        self.state = ResponseExpectationState::Poisoned;
    }
}

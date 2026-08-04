use std::time::{Duration, Instant};

use serde::Serialize;

use super::{
    BackendClientTransport, IncomingMessage, ManagedBackendError, ManagedBackendSession,
    ReceiveOutcome, TransportWriteFailure,
};
use crate::{
    CallerNoSuccessorFence, CoarseThreadCleanupDisposition, CoarseThreadCleanupOutcome,
    EmptyAcknowledgement, ExactForegroundTurn, ExactForegroundTurnAuthorization,
    JsonRpcErrorVerdict, PersistentFailureInterruptAuthorization,
    PersistentFailureInterruptCorrelation, PersistentFailureInterruptOutcome,
    SameSessionCleanupOrdering, StopAttemptCorrelation, StopOperationCorrelation,
    TurnInterruptDisposition, TurnInterruptOutcome,
    exact_interruption::ExactForegroundTurnAuthorizationCore, incoming_json::ResponseFamily,
};

const INTERRUPT_METHOD: &str = "turn/interrupt";
const CLEANUP_METHOD: &str = "thread/backgroundTerminals/clean";

#[derive(Serialize)]
struct ExactRequest<P> {
    method: &'static str,
    id: u64,
    params: P,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InterruptParams<'a> {
    thread_id: &'a beryl_model::CasThreadId,
    turn_id: &'a beryl_model::CasTurnId,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupParams<'a> {
    thread_id: &'a beryl_model::CasThreadId,
}

enum EmptyRequestCompletion {
    Accepted,
    Rejected(crate::JsonRpcError),
    ProvenNotDispatched(ManagedBackendError),
    CompletionUnknown(ManagedBackendError),
}

impl<P> ExactRequest<P> {
    const fn new(id: u64, method: &'static str, params: P) -> Self {
        Self { method, id, params }
    }
}

impl ManagedBackendSession {
    /// Reports whether optional pinned coarse cleanup is admitted on this foreground session.
    ///
    /// Admission is a closed local fact established by the exact-release initialize handshake and
    /// its negotiated experimental API capability. This read sends no compatibility probe and
    /// does not imply that cleanup has completed, or that a target authorization is currently
    /// available.
    #[must_use]
    pub fn admits_exact_thread_background_terminals_cleanup(&self) -> bool {
        matches!(
            self.transport,
            BackendClientTransport::ForegroundWebSocket(_)
        ) && !self.transport.is_closed()
            && self.has_full_turn_stream()
            && self.experimental_api_negotiated
            && self
                .initialize
                .as_ref()
                .is_some_and(|initialize| initialize.validate_required_app_server_version().is_ok())
    }

    /// Binds the exact target currently owned by this sole foreground driver.
    ///
    /// The caller establishes this binding only after its CAS-live route has authenticated the
    /// runtime, managed-process generation, loaded-thread generation, thread, and active turn.
    /// A different target cannot replace it without an explicit unbind cut.
    pub fn bind_exact_foreground_turn(
        &mut self,
        target: ExactForegroundTurn,
    ) -> Result<(), ManagedBackendError> {
        self.require_exact_foreground_profile(INTERRUPT_METHOD)?;
        if target.turn_id().as_str().is_empty() {
            return Err(ManagedBackendError::ExactForegroundTurnMismatch);
        }
        if self.exact_foreground_thread.is_some() {
            return Err(ManagedBackendError::ExactForegroundThreadAlreadyBound);
        }
        if self.exact_foreground_turn.is_some() {
            return Err(ManagedBackendError::ExactForegroundTurnAlreadyBound);
        }
        self.advance_foreground_authorization_epoch()?;
        self.exact_foreground_turn = Some(target);
        Ok(())
    }

    /// Revokes request authority and removes the driver's exact target binding.
    pub fn unbind_exact_foreground_turn(
        &mut self,
    ) -> Result<Option<ExactForegroundTurn>, ManagedBackendError> {
        self.advance_foreground_authorization_epoch()?;
        Ok(self.exact_foreground_turn.take())
    }

    /// Mints one session-bound exact authorization from the caller's held target fence.
    pub fn authorize_exact_foreground_turn(
        &mut self,
        target: ExactForegroundTurn,
        operation: StopOperationCorrelation,
        attempt: StopAttemptCorrelation,
        fence: CallerNoSuccessorFence,
    ) -> Result<ExactForegroundTurnAuthorization, ManagedBackendError> {
        let authorization_epoch = self.authorize_bound_exact_target(&target)?;
        Ok(ExactForegroundTurnAuthorization::new(
            target,
            operation,
            attempt,
            fence,
            self.approval_response_authority_generation,
            authorization_epoch,
        ))
    }

    /// Mints one separately typed volatile interruption authorization.
    ///
    /// The caller holds the exact target's no-successor fence and owns all persistent-failure
    /// election policy. The returned capability cannot be passed to durable stop or cleanup.
    pub fn authorize_persistent_failure_interrupt(
        &mut self,
        target: ExactForegroundTurn,
        correlation: PersistentFailureInterruptCorrelation,
        fence: CallerNoSuccessorFence,
    ) -> Result<PersistentFailureInterruptAuthorization, ManagedBackendError> {
        let authorization_epoch = self.authorize_bound_exact_target(&target)?;
        Ok(PersistentFailureInterruptAuthorization::new(
            target,
            correlation,
            fence,
            self.approval_response_authority_generation,
            authorization_epoch,
        ))
    }

    fn authorize_bound_exact_target(
        &mut self,
        target: &ExactForegroundTurn,
    ) -> Result<u64, ManagedBackendError> {
        self.require_exact_foreground_profile(INTERRUPT_METHOD)?;
        let Some(bound_target) = self.exact_foreground_turn.as_ref() else {
            return Err(ManagedBackendError::ExactForegroundTurnUnbound);
        };
        if bound_target != target {
            return Err(ManagedBackendError::ExactForegroundTurnMismatch);
        }
        self.advance_foreground_authorization_epoch()
    }

    /// Revokes every authorization minted before the caller's later target cut.
    pub fn revoke_exact_foreground_turn_authorizations(
        &mut self,
    ) -> Result<(), ManagedBackendError> {
        self.advance_foreground_authorization_epoch().map(|_| ())
    }

    pub(super) fn advance_foreground_authorization_epoch(
        &mut self,
    ) -> Result<u64, ManagedBackendError> {
        let Some(next) = self.foreground_authorization_epoch.checked_add(1) else {
            self.retire_connection();
            return Err(ManagedBackendError::ExactForegroundAuthorizationEpochExhausted);
        };
        self.foreground_authorization_epoch = next;
        Ok(next)
    }

    /// Issues pinned `turn/interrupt` through the sole exact foreground driver.
    pub fn interrupt_exact_foreground_turn(
        &mut self,
        authorization: ExactForegroundTurnAuthorization,
        timeout: Duration,
    ) -> TurnInterruptOutcome {
        let completion = self.dispatch_interrupt(authorization.core(), timeout);
        let request = authorization.into_request();
        let disposition = self.normalize_interrupt_completion(completion);
        TurnInterruptOutcome::new(request, disposition)
    }

    /// Issues one volatile pinned `turn/interrupt` for an already elected persistent failure.
    ///
    /// The operation is one-shot and never retried. Its outcome is local diagnostics rather than a
    /// durable stop receipt or lifecycle-completion claim.
    pub fn interrupt_for_persistent_failure(
        &mut self,
        authorization: PersistentFailureInterruptAuthorization,
        timeout: Duration,
    ) -> PersistentFailureInterruptOutcome {
        let completion = self.dispatch_interrupt(authorization.core(), timeout);
        let request = authorization.into_request();
        let disposition = self.normalize_interrupt_completion(completion);
        PersistentFailureInterruptOutcome::new(request, disposition)
    }

    fn normalize_interrupt_completion(
        &mut self,
        completion: EmptyRequestCompletion,
    ) -> TurnInterruptDisposition {
        match completion {
            EmptyRequestCompletion::Accepted => TurnInterruptDisposition::RequestAccepted,
            EmptyRequestCompletion::Rejected(error)
                if error.verdict() == Some(JsonRpcErrorVerdict::RejectedBeforeCoreInterrupt) =>
            {
                TurnInterruptDisposition::RejectedBeforeCoreInterrupt
            }
            EmptyRequestCompletion::Rejected(error) => {
                self.retire_connection();
                TurnInterruptDisposition::CompletionUnknown {
                    error: Box::new(ManagedBackendError::RequestFailed {
                        method: INTERRUPT_METHOD.to_string(),
                        error: Box::new(error),
                    }),
                }
            }
            EmptyRequestCompletion::ProvenNotDispatched(error) => {
                TurnInterruptDisposition::ProvenNotDispatched {
                    error: Box::new(error),
                }
            }
            EmptyRequestCompletion::CompletionUnknown(error) => {
                self.retire_connection();
                TurnInterruptDisposition::CompletionUnknown {
                    error: Box::new(error),
                }
            }
        }
    }

    /// Requests optional pinned coarse cleanup through the same exact foreground authority.
    pub fn clean_exact_thread_background_terminals(
        &mut self,
        authorization: ExactForegroundTurnAuthorization,
        timeout: Duration,
    ) -> CoarseThreadCleanupOutcome {
        let completion = self.dispatch_cleanup(authorization.core(), timeout);
        let request = authorization.into_request();
        let disposition = match completion {
            EmptyRequestCompletion::Accepted => CoarseThreadCleanupDisposition::RequestAccepted {
                ordering: SameSessionCleanupOrdering::new(
                    self.approval_response_authority_generation,
                ),
            },
            EmptyRequestCompletion::ProvenNotDispatched(error) => {
                CoarseThreadCleanupDisposition::ProvenNotDispatched {
                    error: Box::new(error),
                }
            }
            EmptyRequestCompletion::Rejected(error) => {
                self.retire_connection();
                CoarseThreadCleanupDisposition::SessionAuthorityInvalidated {
                    error: Box::new(ManagedBackendError::RequestFailed {
                        method: CLEANUP_METHOD.to_string(),
                        error: Box::new(error),
                    }),
                }
            }
            EmptyRequestCompletion::CompletionUnknown(error) => {
                self.retire_connection();
                CoarseThreadCleanupDisposition::CompletionUnknown {
                    error: Box::new(error),
                }
            }
        };
        CoarseThreadCleanupOutcome::new(request, disposition)
    }

    fn dispatch_interrupt(
        &mut self,
        authorization: &ExactForegroundTurnAuthorizationCore,
        timeout: Duration,
    ) -> EmptyRequestCompletion {
        let params = InterruptParams {
            thread_id: authorization.target().thread_id(),
            turn_id: authorization.target().turn_id(),
        };
        self.dispatch_exact_empty(
            authorization,
            ResponseFamily::TurnInterrupt,
            params,
            EmptyAcknowledgement::TurnInterrupt,
            timeout,
        )
    }

    fn dispatch_cleanup(
        &mut self,
        authorization: &ExactForegroundTurnAuthorizationCore,
        timeout: Duration,
    ) -> EmptyRequestCompletion {
        if !self.experimental_api_negotiated {
            return EmptyRequestCompletion::ProvenNotDispatched(
                ManagedBackendError::RequestProfileMismatch {
                    method: CLEANUP_METHOD,
                    required_profile: "foreground with negotiated experimental API",
                },
            );
        }
        let params = CleanupParams {
            thread_id: authorization.target().thread_id(),
        };
        self.dispatch_exact_empty(
            authorization,
            ResponseFamily::ThreadBackgroundTerminalsClean,
            params,
            EmptyAcknowledgement::ThreadBackgroundTerminalsClean,
            timeout,
        )
    }

    fn dispatch_exact_empty<P: Serialize>(
        &mut self,
        authorization: &ExactForegroundTurnAuthorizationCore,
        family: ResponseFamily,
        params: P,
        acknowledgement: EmptyAcknowledgement,
        timeout: Duration,
    ) -> EmptyRequestCompletion {
        let method = family.method();
        if let Err(error) = self.validate_exact_authorization(authorization, method) {
            return EmptyRequestCompletion::ProvenNotDispatched(error);
        }
        let Some(next_request_id) = self.next_request_id.checked_add(1) else {
            return EmptyRequestCompletion::ProvenNotDispatched(
                ManagedBackendError::RequestIdExhausted { method },
            );
        };
        let request_id = self.next_request_id;
        if self
            .response_expectation
            .install_fixed(request_id, family)
            .is_err()
        {
            return EmptyRequestCompletion::ProvenNotDispatched(
                ManagedBackendError::ResponseExpectationUnavailable { method },
            );
        }

        let request = ExactRequest::new(request_id, method, params);
        match self.transport.write_message(method, &request) {
            Ok(_) => self.next_request_id = next_request_id,
            Err(TransportWriteFailure::ProvenNotDispatched(error)) => {
                if !self.response_expectation.cancel(request_id) {
                    self.retire_connection();
                    return EmptyRequestCompletion::CompletionUnknown(
                        ManagedBackendError::ResponseExpectationUnavailable { method },
                    );
                }
                return EmptyRequestCompletion::ProvenNotDispatched(error);
            }
            Err(TransportWriteFailure::MayHaveDispatched(error)) => {
                self.retire_connection();
                return EmptyRequestCompletion::CompletionUnknown(error);
            }
        }

        let deadline = Instant::now() + timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                self.retire_connection();
                return EmptyRequestCompletion::CompletionUnknown(
                    ManagedBackendError::RequestTimeout {
                        method: method.to_string(),
                        timeout,
                    },
                );
            };
            let outcome = match self.recv_message_timeout(method, remaining) {
                Ok(outcome) => outcome,
                Err(error) => {
                    self.retire_connection();
                    return EmptyRequestCompletion::CompletionUnknown(error);
                }
            };
            match outcome {
                ReceiveOutcome::Quiet => {
                    self.retire_connection();
                    return EmptyRequestCompletion::CompletionUnknown(
                        ManagedBackendError::RequestTimeout {
                            method: method.to_string(),
                            timeout,
                        },
                    );
                }
                ReceiveOutcome::OrderedProgress => {}
                ReceiveOutcome::Message(message @ IncomingMessage::Approval { .. }) => {
                    match self.submit_approval_message(method, message) {
                        Ok(_) | Err(ManagedBackendError::ApprovalTargetFailed { .. }) => {}
                        Err(error) => {
                            self.retire_connection();
                            return EmptyRequestCompletion::CompletionUnknown(error);
                        }
                    }
                }
                ReceiveOutcome::Response(crate::BoundedResponseResult::EmptyAcknowledgement(
                    actual,
                )) if actual == acknowledgement => return EmptyRequestCompletion::Accepted,
                ReceiveOutcome::Response(_) => {
                    self.retire_connection();
                    return EmptyRequestCompletion::CompletionUnknown(
                        ManagedBackendError::UnexpectedBoundedResponse { method },
                    );
                }
                ReceiveOutcome::Rejection(error) => {
                    return EmptyRequestCompletion::Rejected(error);
                }
            }
        }
    }

    fn validate_exact_authorization(
        &self,
        authorization: &ExactForegroundTurnAuthorizationCore,
        method: &'static str,
    ) -> Result<(), ManagedBackendError> {
        self.require_exact_foreground_profile(method)?;
        if authorization.session_authority_generation()
            != self.approval_response_authority_generation
        {
            return Err(ManagedBackendError::ExactForegroundAuthorizationMismatch);
        }
        if authorization.authorization_epoch() != self.foreground_authorization_epoch {
            return Err(ManagedBackendError::ExactForegroundAuthorizationStale);
        }
        let Some(bound_target) = self.exact_foreground_turn.as_ref() else {
            return Err(ManagedBackendError::ExactForegroundTurnUnbound);
        };
        if bound_target != authorization.target() {
            return Err(ManagedBackendError::ExactForegroundTurnMismatch);
        }
        Ok(())
    }

    pub(super) fn require_exact_foreground_profile(
        &self,
        method: &'static str,
    ) -> Result<(), ManagedBackendError> {
        if !matches!(
            self.transport,
            BackendClientTransport::ForegroundWebSocket(_)
        ) {
            return Err(ManagedBackendError::RequestProfileMismatch {
                method,
                required_profile: "foreground",
            });
        }
        if self.transport.is_closed() {
            return Err(ManagedBackendError::TransportClosed {
                method: method.to_string(),
            });
        }
        if self.initialize.is_none() {
            return Err(ManagedBackendError::ClientNotInitialized);
        }
        if !self.has_full_turn_stream() {
            return Err(ManagedBackendError::RequestProfileMismatch {
                method,
                required_profile: "foreground",
            });
        }
        if self.ordered_turn_stream_sink.is_none() {
            return Err(ManagedBackendError::OrderedTurnStreamSinkUnbound);
        }
        Ok(())
    }
}

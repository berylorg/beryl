use std::{
    fmt,
    time::{Duration, Instant},
};

use beryl_model::{CasLoadedSessionGeneration, CasThreadId, RuntimeId};
use serde::Serialize;

use super::{
    IncomingMessage, ManagedBackendError, ManagedBackendSession, NonIdempotentRequestOutcome,
    ReceiveOutcome, TransportWriteFailure,
};
use crate::{CallerNoSuccessorFence, EmptyAcknowledgement, incoming_json::ResponseFamily};

const COMPACT_METHOD: &str = "thread/compact/start";

/// Opaque local correlation for one claimed context-compaction request attempt.
///
/// CAS never receives this value and does not treat it as an idempotency key.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct CompactionAttemptCorrelation([u8; 16]);

/// Exact idle foreground CAS thread selected by the caller's operation gate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactForegroundThread {
    runtime_id: RuntimeId,
    loaded_session_generation: CasLoadedSessionGeneration,
    thread_id: CasThreadId,
}

/// Non-cloneable authority for one exact compact-start dispatch.
#[derive(Debug)]
pub struct ExactForegroundThreadAuthorization {
    target: ExactForegroundThread,
    attempt: CompactionAttemptCorrelation,
    fence: CallerNoSuccessorFence,
    session_authority_generation: u64,
    authorization_epoch: u64,
}

/// Returned local identity of one consumed compact-start authorization.
#[derive(Debug)]
pub struct CompactThreadRequest {
    target: ExactForegroundThread,
    attempt: CompactionAttemptCorrelation,
    _fence: CallerNoSuccessorFence,
}

/// Closed disposition of one non-idempotent `thread/compact/start` request.
#[derive(Debug)]
pub enum CompactThreadDisposition {
    /// CAS returned the exact empty enqueue acknowledgement.
    RequestAccepted,
    /// CAS returned one matching structured rejection before accepting compaction.
    ExactRejection { error: crate::JsonRpcError },
    /// Local byte-level evidence proves that no request byte crossed the transport.
    ProvenNotDispatched { error: Box<ManagedBackendError> },
    /// The request may have crossed, but no authoritative response survived.
    CompletionUnknown { error: Box<ManagedBackendError> },
}

/// Correlation-bearing outcome of one exact compact-start attempt.
#[must_use = "compact-start outcomes must be reconciled by exact attempt correlation"]
#[derive(Debug)]
pub struct CompactThreadOutcome {
    request: CompactThreadRequest,
    disposition: CompactThreadDisposition,
}

#[derive(Serialize)]
struct CompactRequest<'a> {
    method: &'static str,
    id: u64,
    params: CompactParams<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompactParams<'a> {
    thread_id: &'a CasThreadId,
}

impl CompactionAttemptCorrelation {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl fmt::Debug for CompactionAttemptCorrelation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CompactionAttemptCorrelation([opaque; 16])")
    }
}

impl ExactForegroundThread {
    /// Binds the exact Beryl runtime, loaded generation, and subscribed CAS thread.
    #[must_use]
    pub const fn new(
        runtime_id: RuntimeId,
        loaded_session_generation: CasLoadedSessionGeneration,
        thread_id: CasThreadId,
    ) -> Self {
        Self {
            runtime_id,
            loaded_session_generation,
            thread_id,
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
}

impl ExactForegroundThreadAuthorization {
    #[must_use]
    pub const fn target(&self) -> &ExactForegroundThread {
        &self.target
    }

    #[must_use]
    pub const fn attempt_correlation(&self) -> CompactionAttemptCorrelation {
        self.attempt
    }

    fn into_request(self) -> CompactThreadRequest {
        CompactThreadRequest {
            target: self.target,
            attempt: self.attempt,
            _fence: self.fence,
        }
    }
}

impl CompactThreadRequest {
    #[must_use]
    pub const fn target(&self) -> &ExactForegroundThread {
        &self.target
    }

    #[must_use]
    pub const fn attempt_correlation(&self) -> CompactionAttemptCorrelation {
        self.attempt
    }

    #[must_use]
    pub const fn had_no_successor_fence(&self) -> bool {
        true
    }
}

impl CompactThreadOutcome {
    fn new(request: CompactThreadRequest, disposition: CompactThreadDisposition) -> Self {
        Self {
            request,
            disposition,
        }
    }

    #[must_use]
    pub const fn request(&self) -> &CompactThreadRequest {
        &self.request
    }

    #[must_use]
    pub const fn disposition(&self) -> &CompactThreadDisposition {
        &self.disposition
    }
}

impl ManagedBackendSession {
    /// Binds the exact idle thread currently owned by the sole foreground driver.
    pub fn bind_exact_foreground_thread(
        &mut self,
        target: ExactForegroundThread,
    ) -> Result<(), ManagedBackendError> {
        self.require_exact_foreground_profile(COMPACT_METHOD)?;
        if self.exact_foreground_turn.is_some() || self.exact_foreground_thread.is_some() {
            return Err(ManagedBackendError::ExactForegroundThreadAlreadyBound);
        }
        self.advance_foreground_authorization_epoch()?;
        self.exact_foreground_thread = Some(target);
        Ok(())
    }

    /// Revokes compact-start authority and removes the exact thread binding.
    pub fn unbind_exact_foreground_thread(
        &mut self,
    ) -> Result<Option<ExactForegroundThread>, ManagedBackendError> {
        self.advance_foreground_authorization_epoch()?;
        Ok(self.exact_foreground_thread.take())
    }

    /// Mints one attempt-bound compact-start authorization under the caller's operation fence.
    pub fn authorize_exact_foreground_thread(
        &mut self,
        target: ExactForegroundThread,
        attempt: CompactionAttemptCorrelation,
        fence: CallerNoSuccessorFence,
    ) -> Result<ExactForegroundThreadAuthorization, ManagedBackendError> {
        self.require_exact_foreground_profile(COMPACT_METHOD)?;
        let Some(bound_target) = self.exact_foreground_thread.as_ref() else {
            return Err(ManagedBackendError::ExactForegroundThreadUnbound);
        };
        if bound_target != &target {
            return Err(ManagedBackendError::ExactForegroundThreadMismatch);
        }
        let authorization_epoch = self.advance_foreground_authorization_epoch()?;
        Ok(ExactForegroundThreadAuthorization {
            target,
            attempt,
            fence,
            session_authority_generation: self.approval_response_authority_generation,
            authorization_epoch,
        })
    }

    /// Sends exactly one pinned compact-start request and waits only for its JSON-RPC response.
    ///
    /// `request_timeout` bounds acknowledgement delivery. It is not the feature-owned compaction
    /// completion timeout, which begins after acceptance outside this request loop.
    pub fn compact_exact_foreground_thread(
        &mut self,
        authorization: ExactForegroundThreadAuthorization,
        request_timeout: Duration,
    ) -> CompactThreadOutcome {
        let completion = self.dispatch_compact(&authorization, request_timeout);
        let request = authorization.into_request();
        let disposition = match completion {
            NonIdempotentRequestOutcome::ExactResponse { response: () } => {
                CompactThreadDisposition::RequestAccepted
            }
            NonIdempotentRequestOutcome::ExactRejection { error } => {
                CompactThreadDisposition::ExactRejection { error }
            }
            NonIdempotentRequestOutcome::ProvenNotDispatched { error } => {
                CompactThreadDisposition::ProvenNotDispatched { error }
            }
            NonIdempotentRequestOutcome::CompletionUnknown { error } => {
                CompactThreadDisposition::CompletionUnknown { error }
            }
        };
        CompactThreadOutcome::new(request, disposition)
    }

    fn dispatch_compact(
        &mut self,
        authorization: &ExactForegroundThreadAuthorization,
        request_timeout: Duration,
    ) -> NonIdempotentRequestOutcome<()> {
        if let Err(error) = self.validate_compaction_authorization(authorization) {
            return proven_not_dispatched(error);
        }
        let Some(next_request_id) = self.next_request_id.checked_add(1) else {
            return proven_not_dispatched(ManagedBackendError::RequestIdExhausted {
                method: COMPACT_METHOD,
            });
        };
        let request_id = self.next_request_id;
        if self
            .response_expectation
            .install_fixed(request_id, ResponseFamily::ThreadCompactStart)
            .is_err()
        {
            return proven_not_dispatched(ManagedBackendError::ResponseExpectationUnavailable {
                method: COMPACT_METHOD,
            });
        }
        let request = CompactRequest {
            method: COMPACT_METHOD,
            id: request_id,
            params: CompactParams {
                thread_id: authorization.target().thread_id(),
            },
        };
        match self.transport.write_message(COMPACT_METHOD, &request) {
            Ok(_) => self.next_request_id = next_request_id,
            Err(TransportWriteFailure::ProvenNotDispatched(error)) => {
                if !self.response_expectation.cancel(request_id) {
                    self.retire_connection();
                    return completion_unknown(
                        ManagedBackendError::ResponseExpectationUnavailable {
                            method: COMPACT_METHOD,
                        },
                    );
                }
                return proven_not_dispatched(error);
            }
            Err(TransportWriteFailure::MayHaveDispatched(error)) => {
                self.retire_connection();
                return completion_unknown(error);
            }
        }
        self.await_compact_response(request_timeout)
    }

    fn validate_compaction_authorization(
        &self,
        authorization: &ExactForegroundThreadAuthorization,
    ) -> Result<(), ManagedBackendError> {
        self.require_exact_foreground_profile(COMPACT_METHOD)?;
        if authorization.session_authority_generation != self.approval_response_authority_generation
        {
            return Err(ManagedBackendError::ExactForegroundAuthorizationMismatch);
        }
        if authorization.authorization_epoch != self.foreground_authorization_epoch {
            return Err(ManagedBackendError::ExactForegroundAuthorizationStale);
        }
        let Some(bound_target) = self.exact_foreground_thread.as_ref() else {
            return Err(ManagedBackendError::ExactForegroundThreadUnbound);
        };
        if bound_target != authorization.target() {
            return Err(ManagedBackendError::ExactForegroundThreadMismatch);
        }
        Ok(())
    }

    fn await_compact_response(
        &mut self,
        request_timeout: Duration,
    ) -> NonIdempotentRequestOutcome<()> {
        let deadline = Instant::now() + request_timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return self.unknown_compact_timeout(request_timeout);
            };
            let outcome = match self.recv_message_timeout(COMPACT_METHOD, remaining) {
                Ok(outcome) => outcome,
                Err(error) => {
                    self.retire_connection();
                    return completion_unknown(error);
                }
            };
            match outcome {
                ReceiveOutcome::Quiet => return self.unknown_compact_timeout(request_timeout),
                ReceiveOutcome::OrderedProgress => {}
                ReceiveOutcome::Message(message @ IncomingMessage::Approval { .. }) => {
                    match self.submit_approval_message(COMPACT_METHOD, message) {
                        Ok(_) | Err(ManagedBackendError::ApprovalTargetFailed { .. }) => {}
                        Err(error) => {
                            self.retire_connection();
                            return completion_unknown(error);
                        }
                    }
                }
                ReceiveOutcome::Response(crate::BoundedResponseResult::EmptyAcknowledgement(
                    EmptyAcknowledgement::ThreadCompactStart,
                )) => return NonIdempotentRequestOutcome::ExactResponse { response: () },
                ReceiveOutcome::Response(_) => {
                    self.retire_connection();
                    return completion_unknown(ManagedBackendError::UnexpectedBoundedResponse {
                        method: COMPACT_METHOD,
                    });
                }
                ReceiveOutcome::Rejection(error) => {
                    return NonIdempotentRequestOutcome::ExactRejection { error };
                }
            }
        }
    }

    fn unknown_compact_timeout(
        &mut self,
        request_timeout: Duration,
    ) -> NonIdempotentRequestOutcome<()> {
        self.retire_connection();
        completion_unknown(ManagedBackendError::RequestTimeout {
            method: COMPACT_METHOD.to_string(),
            timeout: request_timeout,
        })
    }
}

fn proven_not_dispatched(error: ManagedBackendError) -> NonIdempotentRequestOutcome<()> {
    NonIdempotentRequestOutcome::ProvenNotDispatched {
        error: Box::new(error),
    }
}

fn completion_unknown(error: ManagedBackendError) -> NonIdempotentRequestOutcome<()> {
    NonIdempotentRequestOutcome::CompletionUnknown {
        error: Box::new(error),
    }
}

use std::time::{Duration, Instant};

use serde::Serialize;

use super::{
    BackendClientTransport, IncomingMessage, ManagedBackendError, ManagedBackendSession,
    NonIdempotentRequestOutcome, ReceiveOutcome, TransportWriteFailure,
};
use crate::{
    BoundedResponseResult, EmptyAcknowledgement, ThreadInjectionOutcome, ThreadInjectionPreflight,
    ThreadInjectionRejection, ThreadInjectionSource,
    incoming_json::ResponseFamily,
    thread_injection::{
        THREAD_INJECT_ITEMS_METHOD, ThreadInjectItemsParams, ThreadInjectionSourceFailureSlot,
    },
    thread_lineage::FreshIdleThread,
};

const METHOD: &str = THREAD_INJECT_ITEMS_METHOD;

#[derive(Serialize)]
struct ThreadInjectItemsRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: ThreadInjectItemsParams<'a>,
}

impl ManagedBackendSession {
    /// Injects one validated recovery prefix into one consumed fresh-idle thread.
    ///
    /// Every outcome consumes `target`. An unsuccessful outcome never
    /// authorizes retrying injection against that same CAS thread.
    pub fn inject_thread_items(
        &mut self,
        target: FreshIdleThread,
        preflight: &ThreadInjectionPreflight,
        source: &mut dyn ThreadInjectionSource,
        timeout: Duration,
    ) -> ThreadInjectionOutcome {
        let thread_id = target.thread_id().clone();
        match self.non_idempotent_thread_injection(&thread_id, preflight, source, timeout) {
            NonIdempotentRequestOutcome::ExactResponse { response: () } => {
                ThreadInjectionOutcome::Succeeded {
                    thread: target.into_loaded(),
                }
            }
            NonIdempotentRequestOutcome::ExactRejection { error } => {
                drop(target);
                ThreadInjectionOutcome::Rejected {
                    thread_id,
                    rejection: ThreadInjectionRejection::from_json_rpc(error),
                }
            }
            NonIdempotentRequestOutcome::ProvenNotDispatched { error } => {
                drop(target);
                ThreadInjectionOutcome::ProvenNotDispatched { thread_id, error }
            }
            NonIdempotentRequestOutcome::CompletionUnknown { error } => {
                drop(target);
                if is_concrete_transport_loss(&error) {
                    ThreadInjectionOutcome::TransportLost { thread_id, error }
                } else {
                    ThreadInjectionOutcome::CompletionUnknown { thread_id, error }
                }
            }
        }
    }

    fn non_idempotent_thread_injection(
        &mut self,
        thread_id: &beryl_model::CasThreadId,
        preflight: &ThreadInjectionPreflight,
        source: &mut dyn ThreadInjectionSource,
        timeout: Duration,
    ) -> NonIdempotentRequestOutcome<()> {
        if !matches!(
            self.transport,
            BackendClientTransport::ForegroundWebSocket(_)
        ) {
            let transport = match &self.transport {
                BackendClientTransport::Stdio { .. } => "stdio",
                BackendClientTransport::RequestOnlyWebSocket(_) => "request-only websocket",
                BackendClientTransport::ForegroundWebSocket(_) => unreachable!(),
            };
            return proven_not_dispatched(
                ManagedBackendError::ThreadInjectionTransportUnsupported {
                    method: METHOD.to_string(),
                    transport,
                },
            );
        }
        if self.initialize.is_none() {
            return proven_not_dispatched(ManagedBackendError::ClientNotInitialized);
        }
        if !self.has_full_turn_stream() {
            return proven_not_dispatched(ManagedBackendError::RequestProfileMismatch {
                method: METHOD,
                required_profile: "full turn stream",
            });
        }
        if self.ordered_turn_stream_sink.is_none() {
            return proven_not_dispatched(ManagedBackendError::OrderedTurnStreamSinkUnbound);
        }
        if self.transport.is_closed() {
            return proven_not_dispatched(ManagedBackendError::TransportClosed {
                method: METHOD.to_string(),
            });
        }

        let Some(next_request_id) = self.next_request_id.checked_add(1) else {
            return proven_not_dispatched(ManagedBackendError::RequestIdExhausted {
                method: METHOD,
            });
        };
        let request_id = self.next_request_id;
        if self
            .response_expectation
            .install_fixed(request_id, ResponseFamily::ThreadInjectItems)
            .is_err()
        {
            return proven_not_dispatched(ManagedBackendError::ResponseExpectationUnavailable {
                method: METHOD,
            });
        }
        let write_result = {
            let source_failure = ThreadInjectionSourceFailureSlot::default();
            let params =
                ThreadInjectItemsParams::new(thread_id, preflight, source, &source_failure);
            let request = ThreadInjectItemsRequest::new(request_id, params);
            self.transport
                .write_injection_message(METHOD, &request, &source_failure)
        };
        match write_result {
            Ok(_) => self.next_request_id = next_request_id,
            Err(failure) => return self.finish_thread_injection_write(request_id, failure),
        }

        self.await_thread_injection(request_id, timeout)
    }

    fn finish_thread_injection_write(
        &mut self,
        request_id: u64,
        failure: TransportWriteFailure,
    ) -> NonIdempotentRequestOutcome<()> {
        match failure {
            TransportWriteFailure::ProvenNotDispatched(error) => {
                if !self.response_expectation.cancel(request_id) {
                    self.retire_connection();
                    return proven_not_dispatched(
                        ManagedBackendError::ResponseExpectationUnavailable { method: METHOD },
                    );
                }
                proven_not_dispatched(error)
            }
            TransportWriteFailure::MayHaveDispatched(error) => {
                self.retire_connection();
                completion_unknown(error)
            }
        }
    }

    fn await_thread_injection(
        &mut self,
        request_id: u64,
        timeout: Duration,
    ) -> NonIdempotentRequestOutcome<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return self.fail_dispatched_thread_injection(
                    request_id,
                    ManagedBackendError::RequestTimeout {
                        method: METHOD.to_string(),
                        timeout,
                    },
                );
            };
            let outcome = match self.recv_message_timeout(METHOD, remaining) {
                Ok(outcome) => outcome,
                Err(error) => {
                    return self.fail_dispatched_thread_injection(request_id, error);
                }
            };
            match outcome {
                ReceiveOutcome::Quiet => {
                    return self.fail_dispatched_thread_injection(
                        request_id,
                        ManagedBackendError::RequestTimeout {
                            method: METHOD.to_string(),
                            timeout,
                        },
                    );
                }
                ReceiveOutcome::OrderedProgress => {}
                ReceiveOutcome::Message(message @ IncomingMessage::Approval { .. }) => {
                    match self.submit_approval_message(METHOD, message) {
                        Ok(_) | Err(ManagedBackendError::ApprovalTargetFailed { .. }) => {}
                        Err(error) => {
                            return self.fail_dispatched_thread_injection(request_id, error);
                        }
                    }
                }
                ReceiveOutcome::Response(result) => {
                    return self.finish_thread_injection_response(request_id, result);
                }
                ReceiveOutcome::Rejection(error) => {
                    return NonIdempotentRequestOutcome::ExactRejection { error };
                }
            }
        }
    }

    fn finish_thread_injection_response(
        &mut self,
        request_id: u64,
        result: BoundedResponseResult,
    ) -> NonIdempotentRequestOutcome<()> {
        if !matches!(
            result,
            BoundedResponseResult::EmptyAcknowledgement(EmptyAcknowledgement::ThreadInjectItems)
        ) {
            return self.fail_dispatched_thread_injection(
                request_id,
                ManagedBackendError::UnexpectedBoundedResponse { method: METHOD },
            );
        }
        NonIdempotentRequestOutcome::ExactResponse { response: () }
    }

    fn fail_dispatched_thread_injection(
        &mut self,
        _request_id: u64,
        error: ManagedBackendError,
    ) -> NonIdempotentRequestOutcome<()> {
        self.retire_connection();
        completion_unknown(error)
    }
}

impl<'a> ThreadInjectItemsRequest<'a> {
    const fn new(id: u64, params: ThreadInjectItemsParams<'a>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: METHOD,
            params,
        }
    }
}

fn is_concrete_transport_loss(error: &ManagedBackendError) -> bool {
    if let ManagedBackendError::ApprovalDenialWrite { source, .. } = error {
        return is_concrete_transport_loss(source);
    }
    matches!(
        error,
        ManagedBackendError::WriteRequest { .. }
            | ManagedBackendError::ProcessExited { .. }
            | ManagedBackendError::TransportClosed { .. }
            | ManagedBackendError::WebSocketTransport { .. }
    )
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

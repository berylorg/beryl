use std::time::{Duration, Instant};

use beryl_model::CasThreadId;
use serde::Serialize;

use super::{
    BackendClientTransport, IncomingMessage, ManagedBackendError, ManagedBackendSession,
    NonIdempotentRequestOutcome, ReceiveOutcome, TransportWriteFailure,
};
use crate::{
    BoundedResponseResult, TurnStartOptions,
    incoming_json::ResponseFamily,
    turn::{
        StreamedInputSource, StreamedInputSourceFailureSlot, StreamedTurnStartParams,
        StreamedUserMessageVerifier, TurnStartResponseWire,
    },
};

const METHOD: &str = "turn/start";

#[derive(Serialize)]
struct StreamedTurnStartRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: StreamedTurnStartParams<'a>,
}

impl ManagedBackendSession {
    pub(super) fn non_idempotent_streamed_turn_start(
        &mut self,
        thread_id: &CasThreadId,
        source: Box<dyn StreamedInputSource>,
        options: &TurnStartOptions,
        timeout: Duration,
    ) -> NonIdempotentRequestOutcome<TurnStartResponseWire> {
        if !matches!(
            self.transport,
            BackendClientTransport::ForegroundWebSocket(_)
        ) {
            let transport = match &self.transport {
                BackendClientTransport::Stdio { .. } => "stdio",
                BackendClientTransport::RequestOnlyWebSocket(_) => "request-only websocket",
                BackendClientTransport::ForegroundWebSocket(_) => unreachable!(),
            };
            return proven_not_dispatched(ManagedBackendError::StreamedInputTransportUnsupported {
                method: METHOD.to_string(),
                transport,
            });
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
            .install_fixed(request_id, ResponseFamily::TurnStart)
            .is_err()
        {
            return proven_not_dispatched(ManagedBackendError::ResponseExpectationUnavailable {
                method: METHOD,
            });
        }
        if let Err(source_error) =
            self.streamed_user_message_verifier
                .install(request_id, thread_id.clone(), source)
        {
            if !self.response_expectation.cancel(request_id) {
                self.retire_connection();
                return proven_not_dispatched(
                    ManagedBackendError::ResponseExpectationUnavailable { method: METHOD },
                );
            }
            return proven_not_dispatched(ManagedBackendError::StreamedUserMessageCorrelation {
                method: METHOD.to_string(),
                source: source_error,
                transport_bytes_written: false,
            });
        }
        let source_failure = StreamedInputSourceFailureSlot::default();
        let params = StreamedTurnStartParams::new(
            thread_id,
            &self.streamed_user_message_verifier,
            options,
            &source_failure,
        );
        let request = StreamedTurnStartRequest::new(request_id, params);
        match self
            .transport
            .write_streamed_message(METHOD, &request, &source_failure)
        {
            Ok(_) => self.next_request_id = next_request_id,
            Err(failure) => return self.finish_streamed_write_failure(request_id, failure),
        }

        self.await_streamed_turn_start(request_id, timeout)
    }

    fn finish_streamed_write_failure(
        &mut self,
        request_id: u64,
        failure: TransportWriteFailure,
    ) -> NonIdempotentRequestOutcome<TurnStartResponseWire> {
        match failure {
            TransportWriteFailure::ProvenNotDispatched(error) => {
                let expectation_removed = self.response_expectation.cancel(request_id);
                let verifier_removed = self
                    .streamed_user_message_verifier
                    .remove(request_id)
                    .is_ok();
                if !expectation_removed || !verifier_removed {
                    self.retire_connection();
                    return proven_not_dispatched(
                        ManagedBackendError::ResponseExpectationUnavailable { method: METHOD },
                    );
                }
                proven_not_dispatched(error)
            }
            TransportWriteFailure::MayHaveDispatched(error) => {
                let _ = self.streamed_user_message_verifier.remove(request_id);
                self.retire_connection();
                completion_unknown(error)
            }
        }
    }

    fn await_streamed_turn_start(
        &mut self,
        request_id: u64,
        timeout: Duration,
    ) -> NonIdempotentRequestOutcome<TurnStartResponseWire> {
        let deadline = Instant::now() + timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return self.fail_dispatched_streamed_start(
                    request_id,
                    ManagedBackendError::RequestTimeout {
                        method: METHOD.to_string(),
                        timeout,
                    },
                );
            };
            let outcome = match self.recv_message_timeout(METHOD, remaining) {
                Ok(outcome) => outcome,
                Err(error) => return self.fail_dispatched_streamed_start(request_id, error),
            };
            match outcome {
                ReceiveOutcome::Quiet => {
                    return self.fail_dispatched_streamed_start(
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
                            return self.fail_dispatched_streamed_start(request_id, error);
                        }
                    }
                }
                ReceiveOutcome::Response(result) => {
                    return self.finish_streamed_response(request_id, result);
                }
                ReceiveOutcome::Rejection(error) => {
                    return self.finish_streamed_rejection(request_id, error);
                }
            }
        }
    }

    fn finish_streamed_response(
        &mut self,
        request_id: u64,
        result: BoundedResponseResult,
    ) -> NonIdempotentRequestOutcome<TurnStartResponseWire> {
        let BoundedResponseResult::TurnStart(response) = result else {
            return self.fail_dispatched_streamed_start(
                request_id,
                ManagedBackendError::UnexpectedBoundedResponse { method: METHOD },
            );
        };
        let verifier = match self.take_streamed_verifier(request_id) {
            Ok(verifier) => verifier,
            Err(error) => {
                self.retire_connection();
                return completion_unknown(error);
            }
        };
        if let Err(source) = verifier.verify_successful_response(response.turn_id()) {
            self.retire_connection();
            return completion_unknown(ManagedBackendError::StreamedUserMessageCorrelation {
                method: METHOD.to_string(),
                source,
                transport_bytes_written: true,
            });
        }
        NonIdempotentRequestOutcome::ExactResponse { response }
    }

    fn finish_streamed_rejection(
        &mut self,
        request_id: u64,
        error: crate::JsonRpcError,
    ) -> NonIdempotentRequestOutcome<TurnStartResponseWire> {
        let verifier = match self.take_streamed_verifier(request_id) {
            Ok(verifier) => verifier,
            Err(error) => {
                self.retire_connection();
                return completion_unknown(error);
            }
        };
        if let Err(source) = verifier.verify_rejection() {
            self.retire_connection();
            return completion_unknown(ManagedBackendError::StreamedUserMessageCorrelation {
                method: METHOD.to_string(),
                source,
                transport_bytes_written: true,
            });
        }
        NonIdempotentRequestOutcome::ExactRejection { error }
    }

    fn take_streamed_verifier(
        &self,
        request_id: u64,
    ) -> Result<StreamedUserMessageVerifier, ManagedBackendError> {
        self.streamed_user_message_verifier
            .remove(request_id)
            .map_err(
                |source| ManagedBackendError::StreamedUserMessageCorrelation {
                    method: METHOD.to_string(),
                    source,
                    transport_bytes_written: true,
                },
            )
    }

    fn fail_dispatched_streamed_start(
        &mut self,
        request_id: u64,
        error: ManagedBackendError,
    ) -> NonIdempotentRequestOutcome<TurnStartResponseWire> {
        let _ = self.streamed_user_message_verifier.remove(request_id);
        self.retire_connection();
        completion_unknown(error)
    }
}

fn proven_not_dispatched(
    error: ManagedBackendError,
) -> NonIdempotentRequestOutcome<TurnStartResponseWire> {
    NonIdempotentRequestOutcome::ProvenNotDispatched {
        error: Box::new(error),
    }
}

fn completion_unknown(
    error: ManagedBackendError,
) -> NonIdempotentRequestOutcome<TurnStartResponseWire> {
    NonIdempotentRequestOutcome::CompletionUnknown {
        error: Box::new(error),
    }
}

impl<'a> StreamedTurnStartRequest<'a> {
    const fn new(id: u64, params: StreamedTurnStartParams<'a>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: METHOD,
            params,
        }
    }
}

use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

use beryl_model::{CasThreadId, CasTurnId};
use serde::Serialize;

use super::{
    BackendClientTransport, IncomingMessage, ManagedBackendError, ManagedBackendSession,
    NonIdempotentRequestOutcome, ReceiveOutcome, TransportWriteFailure,
};
use crate::{
    BoundedResponseResult, ClientUserMessageId,
    incoming_json::ResponseFamily,
    turn::{
        StreamedInputSource, StreamedInputSourceFailureSlot, StreamedTurnSteerParams,
        TurnSteerResponseWire,
    },
};

const METHOD: &str = "turn/steer";

#[derive(Serialize)]
struct StreamedTurnSteerRequest<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'static str,
    params: StreamedTurnSteerParams<'a>,
}

impl ManagedBackendSession {
    pub(super) fn non_idempotent_streamed_turn_steer(
        &mut self,
        thread_id: &CasThreadId,
        expected_turn_id: &CasTurnId,
        client_user_message_id: &ClientUserMessageId,
        source: Box<dyn StreamedInputSource>,
        timeout: Duration,
    ) -> NonIdempotentRequestOutcome<TurnSteerResponseWire> {
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
            .install_fixed(request_id, ResponseFamily::TurnSteer)
            .is_err()
        {
            return proven_not_dispatched(ManagedBackendError::ResponseExpectationUnavailable {
                method: METHOD,
            });
        }

        let header = source.header();
        let source = RefCell::new(source);
        let source_failure = StreamedInputSourceFailureSlot::default();
        let params = StreamedTurnSteerParams::new(
            thread_id,
            client_user_message_id,
            expected_turn_id,
            header,
            &source,
            &source_failure,
        );
        let request = StreamedTurnSteerRequest::new(request_id, params);
        match self
            .transport
            .write_streamed_message(METHOD, &request, &source_failure)
        {
            Ok(_) => self.next_request_id = next_request_id,
            Err(failure) => {
                return self.finish_turn_steer_write_failure(request_id, failure);
            }
        }
        drop(source);

        self.await_streamed_turn_steer(request_id, expected_turn_id, timeout)
    }

    fn finish_turn_steer_write_failure(
        &mut self,
        request_id: u64,
        failure: TransportWriteFailure,
    ) -> NonIdempotentRequestOutcome<TurnSteerResponseWire> {
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

    fn await_streamed_turn_steer(
        &mut self,
        request_id: u64,
        expected_turn_id: &CasTurnId,
        timeout: Duration,
    ) -> NonIdempotentRequestOutcome<TurnSteerResponseWire> {
        let deadline = Instant::now() + timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return self.fail_dispatched_turn_steer(ManagedBackendError::RequestTimeout {
                    method: METHOD.to_string(),
                    timeout,
                });
            };
            let outcome = match self.recv_message_timeout(METHOD, remaining) {
                Ok(outcome) => outcome,
                Err(error) => return self.fail_dispatched_turn_steer(error),
            };
            match outcome {
                ReceiveOutcome::Quiet => {
                    return self.fail_dispatched_turn_steer(ManagedBackendError::RequestTimeout {
                        method: METHOD.to_string(),
                        timeout,
                    });
                }
                ReceiveOutcome::OrderedProgress => {}
                ReceiveOutcome::Message(message @ IncomingMessage::Approval { .. }) => {
                    match self.submit_approval_message(METHOD, message) {
                        Ok(_) | Err(ManagedBackendError::ApprovalTargetFailed { .. }) => {}
                        Err(error) => return self.fail_dispatched_turn_steer(error),
                    }
                }
                ReceiveOutcome::Response(result) => {
                    return self.finish_turn_steer_response(request_id, expected_turn_id, result);
                }
                ReceiveOutcome::Rejection(error) => {
                    return NonIdempotentRequestOutcome::ExactRejection { error };
                }
            }
        }
    }

    fn finish_turn_steer_response(
        &mut self,
        _request_id: u64,
        expected_turn_id: &CasTurnId,
        result: BoundedResponseResult,
    ) -> NonIdempotentRequestOutcome<TurnSteerResponseWire> {
        let BoundedResponseResult::TurnSteer(response) = result else {
            return self.fail_dispatched_turn_steer(
                ManagedBackendError::UnexpectedBoundedResponse { method: METHOD },
            );
        };
        if response.turn_id() != expected_turn_id {
            let error = ManagedBackendError::TurnResponseIdentityMismatch {
                method: METHOD.to_string(),
                expected: expected_turn_id.clone(),
                actual: response.turn_id().clone(),
            };
            return self.fail_dispatched_turn_steer(error);
        }
        NonIdempotentRequestOutcome::ExactResponse { response }
    }

    fn fail_dispatched_turn_steer(
        &mut self,
        error: ManagedBackendError,
    ) -> NonIdempotentRequestOutcome<TurnSteerResponseWire> {
        self.retire_connection();
        completion_unknown(error)
    }
}

fn proven_not_dispatched(
    error: ManagedBackendError,
) -> NonIdempotentRequestOutcome<TurnSteerResponseWire> {
    NonIdempotentRequestOutcome::ProvenNotDispatched {
        error: Box::new(error),
    }
}

fn completion_unknown(
    error: ManagedBackendError,
) -> NonIdempotentRequestOutcome<TurnSteerResponseWire> {
    NonIdempotentRequestOutcome::CompletionUnknown {
        error: Box::new(error),
    }
}

impl<'a> StreamedTurnSteerRequest<'a> {
    const fn new(id: u64, params: StreamedTurnSteerParams<'a>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: METHOD,
            params,
        }
    }
}

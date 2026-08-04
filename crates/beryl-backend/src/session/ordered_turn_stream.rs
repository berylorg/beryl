use std::time::{Duration, Instant};

use super::{
    BackendClientTransport, IncomingMessage, ManagedBackendError, ManagedBackendSession,
    ReceiveOutcome,
};
use crate::{
    ApprovalInterruption, ApprovalOperationCompletion, ApprovalResponseDisposition,
    OrderedTurnStreamBindingError, OrderedTurnStreamCompletion, OrderedTurnStreamOperation,
    OrderedTurnStreamProgress, OrderedTurnStreamSink,
};

impl ManagedBackendSession {
    /// Binds the sole caller-owned ordered sink to a full-profile WebSocket session.
    pub fn bind_ordered_turn_stream_sink(
        &mut self,
        mut sink: Box<dyn OrderedTurnStreamSink>,
    ) -> Result<(), OrderedTurnStreamBindingError> {
        if !matches!(
            self.transport,
            BackendClientTransport::ForegroundWebSocket(_)
        ) {
            return Err(OrderedTurnStreamBindingError::StdioUnavailable);
        }
        if !self.has_full_turn_stream() {
            return Err(OrderedTurnStreamBindingError::FullTurnStreamRequired);
        }
        if self.ordered_turn_stream_sink.is_some() {
            return Err(OrderedTurnStreamBindingError::AlreadyBound);
        }
        if self.transport.is_closed() {
            return Err(OrderedTurnStreamBindingError::TransportClosed);
        }
        while let Some(message) = self.pre_bind_approvals.pop_front() {
            if let Err(error) = Self::submit_approval_message_through(
                &mut self.transport,
                self.approval_response_authority_generation,
                sink.as_mut(),
                "ordered turn stream binding",
                message,
            ) {
                let binding = binding_error(&error);
                self.retire_connection();
                return Err(binding);
            }
        }
        self.ordered_turn_stream_sink = Some(sink);
        Ok(())
    }

    /// Polls one bound connection until an ordered operation completes or the interval is quiet.
    pub fn poll_ordered_turn_stream_progress(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<OrderedTurnStreamProgress, ManagedBackendError> {
        if self.ordered_turn_stream_sink.is_none() {
            return Err(ManagedBackendError::OrderedTurnStreamSinkUnbound);
        }
        let deadline = Instant::now() + idle_timeout;
        loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Ok(OrderedTurnStreamProgress::Quiet);
            };
            match self.recv_message_timeout("ordered turn stream", remaining)? {
                ReceiveOutcome::Quiet => return Ok(OrderedTurnStreamProgress::Quiet),
                ReceiveOutcome::OrderedProgress => {
                    return Ok(OrderedTurnStreamProgress::Progress);
                }
                ReceiveOutcome::Message(message) => {
                    self.submit_approval_message("ordered turn stream", message)?;
                    return Ok(OrderedTurnStreamProgress::Progress);
                }
                ReceiveOutcome::Response(_) | ReceiveOutcome::Rejection(_) => {
                    self.retire_connection();
                    return Err(ManagedBackendError::ForegroundIngress {
                        method: "ordered turn stream".to_string(),
                        source: crate::ForegroundIngressError::IdleResponse,
                    });
                }
            }
        }
    }

    pub(super) fn recv_message_timeout(
        &mut self,
        method: &str,
        timeout: Duration,
    ) -> Result<ReceiveOutcome, ManagedBackendError> {
        let verifier = match self.streamed_user_message_verifier.active_handle() {
            Ok(verifier) => verifier,
            Err(source) => {
                self.retire_connection();
                return Err(ManagedBackendError::StreamedUserMessageCorrelation {
                    method: method.to_string(),
                    source,
                    transport_bytes_written: true,
                });
            }
        };
        let ordered_sink: Option<&mut (dyn OrderedTurnStreamSink + '_)> = self
            .ordered_turn_stream_sink
            .as_mut()
            .map(|sink| &mut **sink as &mut (dyn OrderedTurnStreamSink + '_));
        let outcome = self.transport.recv_message_timeout(
            method,
            timeout,
            verifier,
            ordered_sink,
            self.approval_response_authority_generation,
            &mut self.response_expectation,
        );
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                self.retire_connection();
                return Err(error);
            }
        };
        if let ReceiveOutcome::Message(message @ IncomingMessage::Approval { request, .. }) =
            &outcome
        {
            message
                .bind_approval_response_authority(self.approval_response_authority_generation)
                .map_err(|source| ManagedBackendError::InvalidApprovalRequest {
                    kind: request.kind(),
                    source,
                })?;
        }
        if self.ordered_turn_stream_sink.is_none() {
            if let ReceiveOutcome::Message(message) = outcome {
                if message
                    .approval_parts()
                    .0
                    .kind()
                    .separate_interruption_required()
                {
                    drop(message);
                    self.retire_connection();
                    return Err(ManagedBackendError::PermissionApprovalStopOwnerUnbound);
                }
                if let Err(message) = self.pre_bind_approvals.try_push(message) {
                    let capacity = self.pre_bind_approvals.diagnostics().capacity;
                    drop(message);
                    self.retire_connection();
                    return Err(ManagedBackendError::PreBindControlCapacityExceeded {
                        method: method.to_string(),
                        capacity,
                    });
                }
                let denial = {
                    let (_, responder) = self
                        .pre_bind_approvals
                        .back()
                        .expect("a just-admitted approval is the pre-bind tail")
                        .approval_parts();
                    ManagedBackendSession::auto_deny_approval_through(
                        &mut self.transport,
                        self.approval_response_authority_generation,
                        responder,
                    )
                };
                if let Err(error) = denial {
                    self.retire_connection();
                    return Err(error);
                }
                return Ok(ReceiveOutcome::OrderedProgress);
            }
        }
        Ok(outcome)
    }

    pub(super) fn submit_approval_message(
        &mut self,
        method: &str,
        message: IncomingMessage,
    ) -> Result<bool, ManagedBackendError> {
        let result = Self::submit_approval_message_through(
            &mut self.transport,
            self.approval_response_authority_generation,
            self.ordered_turn_stream_sink
                .as_mut()
                .expect("approval submission is used only after ordered binding")
                .as_mut(),
            method,
            message,
        );
        if result.as_ref().is_err_and(|error| {
            !matches!(
                error,
                ManagedBackendError::ApprovalTargetFailed { request, .. }
                    if !request.kind().separate_interruption_required()
            )
        }) {
            self.retire_connection();
        }
        result
    }

    fn submit_approval_message_through(
        transport: &mut BackendClientTransport,
        approval_response_authority_generation: u64,
        sink: &mut dyn OrderedTurnStreamSink,
        method: &str,
        message: IncomingMessage,
    ) -> Result<bool, ManagedBackendError> {
        let IncomingMessage::Approval { request, responder } = message;
        let kind = request.kind();
        let result = sink.submit(OrderedTurnStreamOperation::Approval(request));
        match result {
            Ok(OrderedTurnStreamCompletion::Approval(ApprovalOperationCompletion::Routed {
                interruption,
            })) => {
                if !approval_interruption_matches(kind, &responder, &interruption) {
                    settle_failed_approval_denial(
                        transport,
                        approval_response_authority_generation,
                        &responder,
                    )?;
                    return Err(ManagedBackendError::ApprovalInterruptionMismatch {
                        kind,
                        actual: interruption,
                    });
                }
                let denial = settle_approval_denial(
                    transport,
                    approval_response_authority_generation,
                    &responder,
                );
                denial?;
                Ok(true)
            }
            Ok(OrderedTurnStreamCompletion::Approval(
                ApprovalOperationCompletion::TargetFailed { request, cause },
            )) => {
                if request.kind() != kind || !responder.matches(&request) {
                    settle_failed_approval_denial(
                        transport,
                        approval_response_authority_generation,
                        &responder,
                    )?;
                    return Err(ManagedBackendError::OrderedTurnStreamUnexpectedCompletion {
                        method: method.to_string(),
                    });
                }
                settle_failed_approval_denial(
                    transport,
                    approval_response_authority_generation,
                    &responder,
                )?;
                Err(ManagedBackendError::ApprovalTargetFailed { request, cause })
            }
            Ok(_) => {
                settle_failed_approval_denial(
                    transport,
                    approval_response_authority_generation,
                    &responder,
                )?;
                Err(ManagedBackendError::OrderedTurnStreamUnexpectedCompletion {
                    method: method.to_string(),
                })
            }
            Err(source) => {
                let denial = settle_failed_approval_denial(
                    transport,
                    approval_response_authority_generation,
                    &responder,
                );
                if let Err(error) = denial {
                    drop(source);
                    return Err(error);
                }
                Err(ManagedBackendError::OrderedTurnStream {
                    method: method.to_string(),
                    source: Box::new(source),
                })
            }
        }
    }
}

fn approval_interruption_matches(
    kind: crate::ApprovalRequestKind,
    responder: &crate::turn::ApprovalResponder,
    interruption: &ApprovalInterruption,
) -> bool {
    match (kind, interruption) {
        (
            crate::ApprovalRequestKind::CommandExecution | crate::ApprovalRequestKind::FileChange,
            ApprovalInterruption::NotRequired,
        ) => true,
        (
            crate::ApprovalRequestKind::Permissions,
            ApprovalInterruption::DurableStopOwned { target, .. },
        ) => {
            responder.thread_id() == Some(target.thread_id())
                && responder.turn_id() == Some(target.turn_id())
        }
        _ => false,
    }
}

fn settle_failed_approval_denial(
    transport: &mut BackendClientTransport,
    approval_response_authority_generation: u64,
    responder: &crate::turn::ApprovalResponder,
) -> Result<(), ManagedBackendError> {
    if responder.kind().denial_response_interrupts_turn() {
        settle_approval_denial(transport, approval_response_authority_generation, responder)
    } else {
        Ok(())
    }
}

fn settle_approval_denial(
    transport: &mut BackendClientTransport,
    approval_response_authority_generation: u64,
    responder: &crate::turn::ApprovalResponder,
) -> Result<(), ManagedBackendError> {
    match responder.response_disposition() {
        ApprovalResponseDisposition::ResponseRequired => {
            ManagedBackendSession::auto_deny_approval_through(
                transport,
                approval_response_authority_generation,
                responder,
            )
        }
        ApprovalResponseDisposition::AutoDenied => Ok(()),
        ApprovalResponseDisposition::Denied => {
            Err(ManagedBackendError::ApprovalResponseAlreadySent {
                kind: responder.kind(),
            })
        }
    }
}

fn binding_error(error: &ManagedBackendError) -> OrderedTurnStreamBindingError {
    match error {
        ManagedBackendError::ApprovalTargetFailed { cause, .. } => {
            OrderedTurnStreamBindingError::BufferedSubmission(*cause)
        }
        ManagedBackendError::OrderedTurnStream { source, .. } => {
            OrderedTurnStreamBindingError::BufferedSubmission(source.cause())
        }
        ManagedBackendError::ApprovalInterruptionMismatch { .. }
        | ManagedBackendError::OrderedTurnStreamUnexpectedCompletion { .. } => {
            OrderedTurnStreamBindingError::BufferedUnexpectedCompletion
        }
        _ => OrderedTurnStreamBindingError::BufferedNormalization,
    }
}

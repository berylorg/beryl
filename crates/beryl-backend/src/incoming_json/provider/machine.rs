use beryl_model::{CasItemId, CasThreadId, CasTurnId, DynamicToolCallId, DynamicToolName};
use bounded_json::{ContainerKind, Event, ParseFailure, ScalarKind};

use super::{
    capture::ObservationCapture, dynamic_capture::DynamicToolCapture, schema,
    steering_capture::SteeringUserMessageCapture,
};
use crate::{
    DYNAMIC_TOOL_CALL_REQUEST_ID_MAX_BYTES, DYNAMIC_TOOL_NAMESPACE_MAX_BYTES,
    DynamicToolArgumentContainer, DynamicToolArgumentControl, DynamicToolArgumentScalarKind,
    DynamicToolCall, DynamicToolCallError, DynamicToolCallRequestId, DynamicToolCallSchemaError,
    ImageDetail, OrderedTurnStreamCompletion, OrderedTurnStreamOperation, OrderedTurnStreamSink,
    OrderedTurnStreamSubmitCause, OrderedTurnStreamSubmitError, ProviderContainer,
    ProviderDeltaKind, ProviderEnumValue, ProviderField, ProviderFiniteF64, ProviderItemKind,
    ProviderItemLifecycle, ProviderObservationBegin, ProviderObservationControl,
    ProviderObservationError, ProviderObservationRoute, ProviderObservationSchemaError,
    ProviderScalar, ProviderStructuredPosition, ProviderValueContext, SteeringUserMessageError,
    incoming_json::{
        DecodeReaderError, DecodeStats, DecodedIncoming, ForegroundIngressError,
        KnownControlFamily, ResponseExpectation,
    },
    turn::{
        APPROVAL_REQUEST_ID_MAX_BYTES, ApprovalRequest, ApprovalRequestId, ApprovalRequestKind,
        ApprovalRequestSchemaError, COMMAND_EXECUTION_REQUEST_APPROVAL_METHOD,
        FILE_CHANGE_REQUEST_APPROVAL_METHOD, PERMISSIONS_REQUEST_APPROVAL_METHOD,
        StreamedUserMessageCorrelationError, StreamedUserMessageVerifierHandle,
        UserMessageEchoLifecycle,
    },
};
use helpers::*;
use normal_terminal::NormalTerminalMachine;
use thread_closed::ThreadClosedMachine;
use thread_status::ThreadStatusChangedMachine;
use turn_started::TurnStartedMachine;

const STACK_CAPACITY: usize = 144;
const STRUCTURED_DEPTH_LIMIT: u8 = 128;
const FIXED_SCALAR_BYTES: usize = 256;

mod helpers;
mod normal_terminal;
mod thread_closed;
mod thread_status;
mod turn_started;

pub(super) enum MachineError {
    Provider(ProviderObservationError),
    Correlation(StreamedUserMessageCorrelationError),
    Steering(SteeringUserMessageError),
    IncompatibleEnvelopeOrder,
    Approval {
        kind: ApprovalRequestKind,
        source: ApprovalRequestSchemaError,
    },
    DynamicTool(DynamicToolCallError),
    Ordered(Box<OrderedTurnStreamSubmitError>),
    OrderedUnexpectedCompletion,
    Envelope(ForegroundIngressError),
}

impl From<ProviderObservationError> for MachineError {
    fn from(value: ProviderObservationError) -> Self {
        Self::Provider(value)
    }
}

impl From<ProviderObservationSchemaError> for MachineError {
    fn from(value: ProviderObservationSchemaError) -> Self {
        Self::Provider(value.into())
    }
}

impl From<OrderedTurnStreamSubmitCause> for MachineError {
    fn from(value: OrderedTurnStreamSubmitCause) -> Self {
        Self::Provider(value.into())
    }
}

impl From<StreamedUserMessageCorrelationError> for MachineError {
    fn from(value: StreamedUserMessageCorrelationError) -> Self {
        Self::Correlation(value)
    }
}

impl From<SteeringUserMessageError> for MachineError {
    fn from(value: SteeringUserMessageError) -> Self {
        Self::Steering(value)
    }
}

impl From<DynamicToolCallError> for MachineError {
    fn from(value: DynamicToolCallError) -> Self {
        Self::DynamicTool(value)
    }
}

impl From<DynamicToolCallSchemaError> for MachineError {
    fn from(value: DynamicToolCallSchemaError) -> Self {
        Self::DynamicTool(value.into())
    }
}

impl From<ForegroundIngressError> for MachineError {
    fn from(value: ForegroundIngressError) -> Self {
        Self::Envelope(value)
    }
}

#[derive(Clone, Copy)]
enum TargetMethod {
    Lifecycle(ProviderItemLifecycle),
    Delta(ProviderDeltaKind),
}

pub(super) struct Machine<'a> {
    mode: Mode<'a>,
}

// The target mode owns the fixed-capacity parser stack inline so ingress memory remains bounded
// without a target-only heap indirection.
#[allow(clippy::large_enum_variant)]
enum Mode<'a> {
    Undecided {
        classifier: Classifier,
        verifier: Option<StreamedUserMessageVerifierHandle<'a>>,
        sink: Option<&'a mut dyn OrderedTurnStreamSink>,
        response_authority_generation: u64,
        response_expectation: Option<ResponseExpectation>,
    },
    Discard(DiscardMachine),
    Response(ResponseMachine),
    Target(TargetMachine<'a>),
    Approval(ApprovalMachine),
    ThreadClosed(ThreadClosedMachine<'a>),
    ThreadStatus(ThreadStatusChangedMachine<'a>),
    TurnStarted(TurnStartedMachine<'a>),
    NormalTerminal(NormalTerminalMachine<'a>),
    DynamicTool(DynamicToolMachine<'a>),
    Transition,
}

impl<'a> Machine<'a> {
    pub(super) fn new(
        verifier: Option<StreamedUserMessageVerifierHandle<'a>>,
        sink: Option<&'a mut dyn OrderedTurnStreamSink>,
        response_authority_generation: u64,
        response_expectation: Option<ResponseExpectation>,
    ) -> Self {
        Self {
            mode: Mode::Undecided {
                classifier: Classifier::new(),
                verifier,
                sink,
                response_authority_generation,
                response_expectation,
            },
        }
    }

    pub(super) const fn uses_classification_prefix(&self) -> bool {
        matches!(
            &self.mode,
            Mode::Undecided { classifier, .. } if !classifier.is_quarantined()
        )
    }

    pub(super) fn resolve_classification_prefix_pressure(&mut self) {
        if let Mode::Undecided { classifier, .. } = &mut self.mode {
            classifier.resolve_prefix_pressure();
        }
    }

    pub(super) const fn is_response_message(&self) -> bool {
        matches!(self.mode, Mode::Response(_))
    }

    pub(super) fn uses_capture_output(&self) -> bool {
        matches!(&self.mode, Mode::Target(target) if target.uses_capture_output())
            || matches!(&self.mode, Mode::DynamicTool(target) if target.uses_capture_output())
    }

    pub(super) fn capture_output_window(&mut self) -> Result<&mut [u8], MachineError> {
        match &mut self.mode {
            Mode::Target(target) => target.capture_output_window(),
            Mode::DynamicTool(target) => target.capture_output_window(),
            _ => unreachable!("only a target scalar writes into a provider lease"),
        }
    }

    pub(super) fn commit_capture_output(&mut self, produced: usize) -> Result<(), MachineError> {
        match &mut self.mode {
            Mode::Target(target) => target.commit_capture_output(produced),
            Mode::DynamicTool(target) => target.commit_capture_output(produced),
            _ if produced == 0 => Ok(()),
            _ => unreachable!("only a target scalar writes into a provider lease"),
        }
    }

    pub(super) fn commit_scratch_output(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        match &mut self.mode {
            Mode::Undecided { classifier, .. } => {
                classifier.bytes(bytes);
                Ok(())
            }
            Mode::Discard(discard) => {
                discard.bytes(bytes);
                Ok(())
            }
            Mode::Response(response) => {
                response.scratch_bytes(bytes);
                Ok(())
            }
            Mode::Target(target) => target.scratch_bytes(bytes),
            Mode::Approval(approval) => {
                let kind = approval.kind;
                approval
                    .scratch_bytes(bytes)
                    .map_err(|source| MachineError::Approval { kind, source })
            }
            Mode::ThreadClosed(closed) => closed.scratch_bytes(bytes),
            Mode::ThreadStatus(status) => status.scratch_bytes(bytes),
            Mode::TurnStarted(turn) => turn.scratch_bytes(bytes),
            Mode::NormalTerminal(terminal) => terminal.scratch_bytes(bytes),
            Mode::DynamicTool(target) => target.scratch_bytes(bytes),
            Mode::Transition => unreachable!("mode transition is not externally visible"),
        }
    }

    pub(super) fn flush_full_page(&mut self) -> Result<(), MachineError> {
        match &mut self.mode {
            Mode::Target(target) => target.flush_full_page(),
            Mode::DynamicTool(target) => target.flush_full_page(),
            _ => Ok(()),
        }
    }

    pub(super) fn flush_capture_output(&mut self) -> Result<(), MachineError> {
        match &mut self.mode {
            Mode::Target(target) => target.flush_capture_output(),
            Mode::DynamicTool(target) => target.flush_capture_output(),
            _ => Ok(()),
        }
    }

    pub(super) fn mark_transport_lost(&mut self) {
        if let Mode::Target(target) = &mut self.mode {
            target.mark_transport_lost();
        } else if let Mode::DynamicTool(target) = &mut self.mode {
            target.mark_transport_lost();
        }
    }

    pub(super) fn event(&mut self, event: Event) -> Result<(), MachineError> {
        let decision = match &mut self.mode {
            Mode::Undecided { classifier, .. } => classifier.event(event),
            Mode::Discard(discard) => {
                discard.event(event);
                return Ok(());
            }
            Mode::Response(response) => {
                response.event(event);
                return Ok(());
            }
            Mode::Target(target) => return target.event(event),
            Mode::Approval(approval) => {
                let kind = approval.kind;
                return approval
                    .event(event)
                    .map_err(|source| MachineError::Approval { kind, source });
            }
            Mode::ThreadClosed(closed) => return closed.event(event),
            Mode::ThreadStatus(status) => return status.event(event),
            Mode::TurnStarted(turn) => return turn.event(event),
            Mode::NormalTerminal(terminal) => return terminal.event(event),
            Mode::DynamicTool(target) => return target.event(event),
            Mode::Transition => unreachable!("mode transition is not externally visible"),
        };
        let Some(decision) = decision else {
            return Ok(());
        };
        match decision {
            Classification::Discard(disposition) => {
                self.mode = Mode::Discard(DiscardMachine::new(disposition));
            }
            Classification::ResponseSuccess { actual_id } => {
                let Mode::Undecided {
                    response_expectation,
                    ..
                } = std::mem::replace(&mut self.mode, Mode::Transition)
                else {
                    unreachable!("classification decision came from undecided mode")
                };
                self.mode =
                    Mode::Response(ResponseMachine::success(actual_id, response_expectation));
            }
            Classification::ResponseFailure => {
                let Mode::Undecided {
                    response_expectation,
                    ..
                } = std::mem::replace(&mut self.mode, Mode::Transition)
                else {
                    unreachable!("classification decision came from undecided mode")
                };
                self.mode = Mode::Response(ResponseMachine::failure(response_expectation));
            }
            Classification::Target(ClassifiedTarget::Provider(method)) => {
                let Mode::Undecided { verifier, sink, .. } =
                    std::mem::replace(&mut self.mode, Mode::Transition)
                else {
                    unreachable!("classification decision came from undecided mode")
                };
                self.mode = Mode::Target(TargetMachine::new(method, verifier, sink));
            }
            Classification::Target(ClassifiedTarget::Approval(kind)) => {
                let Mode::Undecided { .. } = std::mem::replace(&mut self.mode, Mode::Transition)
                else {
                    unreachable!("classification decision came from undecided mode")
                };
                self.mode = Mode::Approval(ApprovalMachine::new(kind));
            }
            Classification::Target(ClassifiedTarget::DynamicTool) => {
                let Mode::Undecided {
                    sink,
                    response_authority_generation,
                    ..
                } = std::mem::replace(&mut self.mode, Mode::Transition)
                else {
                    unreachable!("classification decision came from undecided mode")
                };
                let sink = sink.ok_or(DynamicToolCallSchemaError::OrderedSinkUnbound)?;
                self.mode =
                    Mode::DynamicTool(DynamicToolMachine::new(sink, response_authority_generation));
            }
            Classification::Target(ClassifiedTarget::ThreadClosed) => {
                let Mode::Undecided { sink, .. } =
                    std::mem::replace(&mut self.mode, Mode::Transition)
                else {
                    unreachable!("classification decision came from undecided mode")
                };
                self.mode = Mode::ThreadClosed(ThreadClosedMachine::new(sink));
            }
            Classification::Target(ClassifiedTarget::ThreadStatus) => {
                let Mode::Undecided { sink, .. } =
                    std::mem::replace(&mut self.mode, Mode::Transition)
                else {
                    unreachable!("classification decision came from undecided mode")
                };
                self.mode = Mode::ThreadStatus(ThreadStatusChangedMachine::new(sink));
            }
            Classification::Target(ClassifiedTarget::TurnStarted) => {
                let Mode::Undecided { sink, .. } =
                    std::mem::replace(&mut self.mode, Mode::Transition)
                else {
                    unreachable!("classification decision came from undecided mode")
                };
                self.mode = Mode::TurnStarted(TurnStartedMachine::new(sink));
            }
            Classification::Target(ClassifiedTarget::NormalTerminal) => {
                let Mode::Undecided { sink, .. } =
                    std::mem::replace(&mut self.mode, Mode::Transition)
                else {
                    unreachable!("classification decision came from undecided mode")
                };
                self.mode = Mode::NormalTerminal(NormalTerminalMachine::new(sink));
            }
        }
        Ok(())
    }

    pub(super) fn map_parse_failure(&mut self, failure: ParseFailure) -> DecodeReaderError {
        match &mut self.mode {
            Mode::Target(target) => target.map_parse_failure(failure),
            Mode::Approval(approval) => approval.map_parse_failure(failure),
            Mode::ThreadClosed(closed) => closed.map_parse_failure(failure),
            Mode::ThreadStatus(status) => status.map_parse_failure(failure),
            Mode::TurnStarted(turn) => turn.map_parse_failure(failure),
            Mode::NormalTerminal(terminal) => terminal.map_parse_failure(failure),
            Mode::DynamicTool(target) => target.map_parse_failure(failure),
            Mode::Response(_) | Mode::Discard(_) => json_failure(failure),
            _ => json_failure(failure),
        }
    }

    pub(super) fn map_output_pressure(&self) -> DecodeReaderError {
        match self.mode {
            Mode::Target(_) => {
                DecodeReaderError::Provider(ProviderObservationSchemaError::AmbiguousSchema.into())
            }
            Mode::Approval(ref approval) => DecodeReaderError::Approval {
                kind: approval.kind,
                source: ApprovalRequestSchemaError::IdentityTooLong,
            },
            Mode::ThreadClosed(_) => {
                DecodeReaderError::Envelope(ForegroundIngressError::MalformedThreadClosed)
            }
            Mode::ThreadStatus(_) => {
                DecodeReaderError::Envelope(ForegroundIngressError::MalformedThreadStatusChanged)
            }
            Mode::TurnStarted(_) => {
                DecodeReaderError::Envelope(ForegroundIngressError::MalformedTurnStarted)
            }
            Mode::NormalTerminal(_) => {
                DecodeReaderError::Envelope(ForegroundIngressError::MalformedNormalTurnTerminal)
            }
            Mode::DynamicTool(_) => {
                DecodeReaderError::DynamicTool(DynamicToolCallSchemaError::IdentityTooLong.into())
            }
            Mode::Response(_) | Mode::Discard(_) => {
                json_message("bounded JSON output capacity was unexpectedly insufficient")
            }
            _ => json_message("bounded JSON output capacity was unexpectedly insufficient"),
        }
    }

    pub(super) fn finish(&mut self) -> Result<DecodedIncoming, MachineError> {
        match &mut self.mode {
            Mode::Target(target) => target.finish(),
            Mode::Approval(approval) => {
                let kind = approval.kind;
                approval
                    .finish()
                    .map_err(|source| MachineError::Approval { kind, source })
            }
            Mode::ThreadClosed(closed) => closed.finish(),
            Mode::ThreadStatus(status) => status.finish(),
            Mode::TurnStarted(turn) => turn.finish(),
            Mode::NormalTerminal(terminal) => terminal.finish(),
            Mode::DynamicTool(target) => target.finish(),
            Mode::Discard(discard) => discard.finish().map_err(Into::into),
            Mode::Response(response) => response.finish().map_err(Into::into),
            Mode::Undecided { classifier, .. } if classifier.is_quarantined() => {
                Err(ForegroundIngressError::Quarantined.into())
            }
            _ => Err(ProviderObservationSchemaError::EnvelopeShape.into()),
        }
    }

    pub(super) fn stats(&self) -> DecodeStats {
        match &self.mode {
            Mode::Target(target) => target.stats,
            _ => DecodeStats::default(),
        }
    }
}

include!("machine/classifier_support.rs");
include!("machine/response.rs");
include!("machine/discard.rs");
include!("machine/classifier.rs");
include!("machine/approval.rs");
include!("machine/dynamic_tool.rs");
include!("machine/state.rs");
include!("machine/web_other.rs");
include!("machine/target_io.rs");
include!("machine/target_mcp.rs");
include!("machine/steering.rs");
include!("machine/number.rs");
include!("machine/container.rs");
include!("machine/fields.rs");
include!("machine/scalar.rs");
include!("machine/item.rs");

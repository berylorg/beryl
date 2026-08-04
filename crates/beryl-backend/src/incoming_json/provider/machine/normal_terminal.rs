use super::*;
use crate::{
    CodexErrorInfo, NormalTurnTerminal, NormalTurnTerminalStatus, OrderedTurnStreamSubmitError,
    TerminalNonSteerableTurnKind,
    turn::{NormalTurnTerminalDiagnostic, NormalTurnTerminalDiagnosticField},
};

include!("normal_terminal/scalar.rs");

pub(super) struct NormalTerminalMachine<'a> {
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    expected: Expected,
    scalar: TerminalScalar,
    thread_id: Option<CasThreadId>,
    turn_id: Option<CasTurnId>,
    status: Option<NormalTurnTerminalStatus>,
    codex_error_info: Option<CodexErrorInfo>,
    pending_codex_object: Option<CodexObjectVariant>,
    diagnostic: NormalTurnTerminalDiagnostic,
}

impl<'a> NormalTerminalMachine<'a> {
    pub(super) fn new(sink: Option<&'a mut dyn OrderedTurnStreamSink>) -> Self {
        Self {
            sink,
            expected: Expected::RootParamsName,
            scalar: TerminalScalar::None,
            thread_id: None,
            turn_id: None,
            status: None,
            codex_error_info: None,
            pending_codex_object: None,
            diagnostic: NormalTurnTerminalDiagnostic::new(),
        }
    }

    pub(super) fn scratch_bytes(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        let accepted = match &mut self.scalar {
            TerminalScalar::Name {
                probe, expected, ..
            } => {
                probe.push(bytes, expected);
                true
            }
            TerminalScalar::Identity { bytes: fixed, .. } => fixed.push(bytes),
            TerminalScalar::Choice { probe, kind, .. } => {
                probe.push(bytes, choice_wires(*kind));
                true
            }
            TerminalScalar::Diagnostic { .. } => self.diagnostic.push(bytes),
            TerminalScalar::Integer { accumulator, .. } => {
                accumulator.push(bytes);
                true
            }
            TerminalScalar::None => bytes.is_empty(),
        };
        accepted.then_some(()).ok_or_else(malformed)
    }

    pub(super) fn event(&mut self, event: Event) -> Result<(), MachineError> {
        if !matches!(self.scalar, TerminalScalar::None) {
            return self.scalar_event(event);
        }
        self.expected_event(event)
    }

    fn scalar_event(&mut self, event: Event) -> Result<(), MachineError> {
        let expected_kind = match self.scalar {
            TerminalScalar::Name { .. }
            | TerminalScalar::Choice {
                kind: ChoiceKind::CodexObject,
                ..
            } => ScalarKind::Name,
            TerminalScalar::Identity { .. }
            | TerminalScalar::Choice { .. }
            | TerminalScalar::Diagnostic { .. } => ScalarKind::String,
            TerminalScalar::Integer { .. } => ScalarKind::Number,
            TerminalScalar::None => return Err(malformed()),
        };
        match event {
            Event::ScalarFragment(kind) if kind == expected_kind => Ok(()),
            Event::ScalarEnd(kind) if kind == expected_kind => self.finish_scalar(),
            _ => Err(malformed()),
        }
    }

    fn finish_scalar(&mut self) -> Result<(), MachineError> {
        let scalar = std::mem::replace(&mut self.scalar, TerminalScalar::None);
        match scalar {
            TerminalScalar::Name {
                probe,
                expected,
                next,
            } if probe.exact(expected) => {
                self.expected = next;
                Ok(())
            }
            TerminalScalar::Identity { bytes, kind, next } => {
                let value = bytes.as_str().ok_or_else(malformed)?;
                match kind {
                    IdentityKind::Thread => {
                        self.thread_id = Some(CasThreadId::new(value).map_err(|_| malformed())?);
                    }
                    IdentityKind::Turn => {
                        self.turn_id = Some(CasTurnId::new(value).map_err(|_| malformed())?);
                    }
                }
                self.expected = next;
                Ok(())
            }
            TerminalScalar::Choice { probe, kind, next } => {
                let index = probe.finish(choice_wires(kind)).ok_or_else(malformed)?;
                self.finish_choice(kind, index)?;
                self.expected = next;
                Ok(())
            }
            TerminalScalar::Diagnostic { field, next } if self.diagnostic.finish(field) => {
                self.expected = next;
                Ok(())
            }
            TerminalScalar::Integer {
                accumulator,
                kind: IntegerKind::DiscardSigned,
                next,
            } if accumulator.is_i64() => {
                self.expected = next;
                Ok(())
            }
            TerminalScalar::Integer {
                accumulator,
                kind: IntegerKind::HttpStatus,
                next,
            } => {
                let status = accumulator.as_u16().ok_or_else(malformed)?;
                self.finish_http_status(Some(status))?;
                self.expected = next;
                Ok(())
            }
            _ => Err(malformed()),
        }
    }

    fn finish_choice(&mut self, kind: ChoiceKind, index: usize) -> Result<(), MachineError> {
        match kind {
            ChoiceKind::ItemsView if index == 0 => Ok(()),
            ChoiceKind::Status => {
                self.status = Some(match index {
                    0 => NormalTurnTerminalStatus::Completed,
                    1 => NormalTurnTerminalStatus::Interrupted,
                    2 => NormalTurnTerminalStatus::Failed,
                    _ => return Err(malformed()),
                });
                Ok(())
            }
            ChoiceKind::CodexUnit => {
                self.codex_error_info = Some(match index {
                    0 => CodexErrorInfo::ContextWindowExceeded,
                    1 => CodexErrorInfo::SessionBudgetExceeded,
                    2 => CodexErrorInfo::UsageLimitExceeded,
                    3 => CodexErrorInfo::ServerOverloaded,
                    4 => CodexErrorInfo::CyberPolicy,
                    5 => CodexErrorInfo::InternalServerError,
                    6 => CodexErrorInfo::Unauthorized,
                    7 => CodexErrorInfo::BadRequest,
                    8 => CodexErrorInfo::ThreadRollbackFailed,
                    9 => CodexErrorInfo::SandboxError,
                    10 => CodexErrorInfo::Other,
                    _ => return Err(malformed()),
                });
                Ok(())
            }
            ChoiceKind::CodexObject => {
                self.pending_codex_object = Some(match index {
                    0 => CodexObjectVariant::HttpConnectionFailed,
                    1 => CodexObjectVariant::ResponseStreamConnectionFailed,
                    2 => CodexObjectVariant::ResponseStreamDisconnected,
                    3 => CodexObjectVariant::ResponseTooManyFailedAttempts,
                    4 => CodexObjectVariant::ActiveTurnNotSteerable,
                    _ => return Err(malformed()),
                });
                Ok(())
            }
            ChoiceKind::TurnKind => {
                let kind = match index {
                    0 => TerminalNonSteerableTurnKind::Review,
                    1 => TerminalNonSteerableTurnKind::Compact,
                    _ => return Err(malformed()),
                };
                if !matches!(
                    self.pending_codex_object,
                    Some(CodexObjectVariant::ActiveTurnNotSteerable)
                ) {
                    return Err(malformed());
                }
                self.codex_error_info =
                    Some(CodexErrorInfo::ActiveTurnNotSteerable { turn_kind: kind });
                self.pending_codex_object = None;
                Ok(())
            }
            ChoiceKind::ItemsView => Err(malformed()),
        }
    }

    fn finish_http_status(&mut self, http_status_code: Option<u16>) -> Result<(), MachineError> {
        let variant = self.pending_codex_object.take().ok_or_else(malformed)?;
        self.codex_error_info = Some(match variant {
            CodexObjectVariant::HttpConnectionFailed => {
                CodexErrorInfo::HttpConnectionFailed { http_status_code }
            }
            CodexObjectVariant::ResponseStreamConnectionFailed => {
                CodexErrorInfo::ResponseStreamConnectionFailed { http_status_code }
            }
            CodexObjectVariant::ResponseStreamDisconnected => {
                CodexErrorInfo::ResponseStreamDisconnected { http_status_code }
            }
            CodexObjectVariant::ResponseTooManyFailedAttempts => {
                CodexErrorInfo::ResponseTooManyFailedAttempts { http_status_code }
            }
            CodexObjectVariant::ActiveTurnNotSteerable => return Err(malformed()),
        });
        Ok(())
    }

    pub(super) fn map_parse_failure(&self, failure: ParseFailure) -> DecodeReaderError {
        json_failure(failure)
    }

    pub(super) fn finish(&mut self) -> Result<DecodedIncoming, MachineError> {
        if self.expected != Expected::Done
            || !matches!(self.scalar, TerminalScalar::None)
            || !self.diagnostic.is_idle()
            || self.pending_codex_object.is_some()
        {
            return Err(malformed());
        }
        let thread_id = self.thread_id.take().ok_or_else(malformed)?;
        let turn_id = self.turn_id.take().ok_or_else(malformed)?;
        let status = self.status.ok_or_else(malformed)?;
        let diagnostic =
            std::mem::replace(&mut self.diagnostic, NormalTurnTerminalDiagnostic::new());
        let terminal = NormalTurnTerminal::decoded(
            thread_id,
            turn_id,
            status,
            self.codex_error_info.take(),
            diagnostic,
        );
        let operation = OrderedTurnStreamOperation::NormalTurnTerminal(terminal);
        let Some(sink) = self.sink.as_deref_mut() else {
            return Err(MachineError::Ordered(Box::new(
                OrderedTurnStreamSubmitError::new(
                    operation,
                    OrderedTurnStreamSubmitCause::Unavailable,
                ),
            )));
        };
        match sink.submit(operation) {
            Ok(OrderedTurnStreamCompletion::Applied) => Ok(DecodedIncoming::OrderedHandled),
            Ok(_) => Err(MachineError::OrderedUnexpectedCompletion),
            Err(source) => Err(MachineError::Ordered(Box::new(source))),
        }
    }
}

include!("normal_terminal/grammar.rs");

fn malformed() -> MachineError {
    ForegroundIngressError::MalformedNormalTurnTerminal.into()
}

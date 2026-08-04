const JSON_RPC_DIAGNOSTIC_BYTES: usize = 4_096;

use crate::incoming_json::ResponseFamily;

include!("response/common.rs");
include!("response/number.rs");
include!("response/failure.rs");
include!("response/ordered_object.rs");
include!("response/initialize.rs");
include!("response/config.rs");
include!("response/lineage/status.rs");
include!("response/lineage/thread.rs");
include!("response/lineage/result.rs");
include!("response/thread_read/source.rs");
include!("response/thread_read/thread.rs");
include!("response/thread_read/result.rs");
include!("response/turn_start.rs");
include!("response/turn_steer.rs");
include!("response/model/efforts.rs");
include!("response/model/record.rs");
include!("response/model/page.rs");
include!("response/compatibility.rs");
include!("response/success.rs");

enum ResponseMachine {
    Success(SuccessResponse),
    Failure(FailureResponse),
}

impl ResponseMachine {
    fn success(actual_id: Option<u64>, expected: Option<ResponseExpectation>) -> Self {
        Self::Success(SuccessResponse::new(actual_id, expected))
    }

    fn failure(expected: Option<ResponseExpectation>) -> Self {
        Self::Failure(FailureResponse::new(expected))
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match self {
            Self::Success(response) => response.scratch_bytes(bytes),
            Self::Failure(response) => response.scratch_bytes(bytes),
        }
    }

    fn event(&mut self, event: Event) {
        match self {
            Self::Success(response) => response.event(event),
            Self::Failure(response) => response.event(event),
        }
    }

    fn finish(&mut self) -> Result<DecodedIncoming, ForegroundIngressError> {
        match self {
            Self::Success(response) => response.finish(),
            Self::Failure(response) => response.finish(),
        }
    }
}

#[derive(Clone, Copy)]
enum ValueTracker {
    Unstarted,
    Scalar(ScalarKind),
    Container(u16),
    Complete,
}

impl ValueTracker {
    const fn new() -> Self {
        Self::Unstarted
    }

    const fn is_complete(self) -> bool {
        matches!(self, Self::Complete)
    }

    fn event(&mut self, event: Event) -> bool {
        match (*self, event) {
            (Self::Unstarted, Event::ContainerStart(_)) => *self = Self::Container(1),
            (Self::Unstarted, Event::ScalarStart(kind)) => *self = Self::Scalar(kind),
            (Self::Unstarted, Event::Boolean(_) | Event::Null) => *self = Self::Complete,
            (Self::Scalar(expected), Event::ScalarFragment(actual)) if expected == actual => {}
            (Self::Scalar(expected), Event::ScalarEnd(actual)) if expected == actual => {
                *self = Self::Complete;
            }
            (Self::Container(depth), Event::ContainerStart(_)) => {
                *self = Self::Container(depth.saturating_add(1));
            }
            (Self::Container(1), Event::ContainerEnd(_)) => *self = Self::Complete,
            (Self::Container(depth), Event::ContainerEnd(_)) => {
                *self = Self::Container(depth - 1);
            }
            (Self::Container(_), _) => {}
            _ => return false,
        }
        true
    }
}

const ID_NAME: [&[u8]; 1] = [b"id"];

struct ErrorObject {
    state: ErrorState,
    code: Option<i64>,
    diagnostic: [u8; JSON_RPC_DIAGNOSTIC_BYTES],
    diagnostic_len: usize,
    diagnostic_was_truncated: bool,
    data_was_present: bool,
    turn_steer_data_probe: Option<TurnSteerErrorDataProbe>,
    malformed: bool,
}

enum ErrorState {
    Start,
    CodeNameStart,
    CodeName(ClassifierProbe),
    CodeValue,
    Code(NumberBytes),
    DataOrMessageNameStart,
    DataOrMessageName(ClassifierProbe),
    DataValue,
    DataDiscard(ValueTracker),
    MessageNameStart,
    MessageName(ClassifierProbe),
    MessageValue,
    Message,
    AfterMessage,
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

impl ErrorObject {
    fn new() -> Self {
        Self {
            state: ErrorState::Start,
            code: None,
            diagnostic: [0; JSON_RPC_DIAGNOSTIC_BYTES],
            diagnostic_len: 0,
            diagnostic_was_truncated: false,
            data_was_present: false,
            turn_steer_data_probe: None,
            malformed: false,
        }
    }

    fn bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ErrorState::CodeName(probe) => {
                probe.push(bytes, &CODE_ERROR_NAME);
            }
            ErrorState::MessageName(probe) => {
                probe.push(bytes, &MESSAGE_ERROR_NAME);
            }
            ErrorState::DataOrMessageName(probe) => {
                probe.push(bytes, &DATA_OR_MESSAGE_NAMES);
            }
            ErrorState::Code(number) => number.push(bytes),
            ErrorState::Message => self.push_diagnostic(bytes),
            ErrorState::DataDiscard(_) => {
                if let Some(probe) = &mut self.turn_steer_data_probe {
                    probe.scratch_bytes(bytes);
                }
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, ErrorState::Complete);
        self.state = match state {
            ErrorState::Start => self.start(event),
            ErrorState::CodeNameStart => self.start_exact_name(event, true),
            ErrorState::CodeName(probe) => {
                self.finish_exact_name(probe, event, ErrorState::CodeValue)
            }
            ErrorState::CodeValue => self.start_code(event),
            ErrorState::Code(number) => self.code_event(number, event),
            ErrorState::DataOrMessageNameStart => self.start_data_or_message_name(event),
            ErrorState::DataOrMessageName(probe) => self.finish_data_or_message_name(probe, event),
            ErrorState::DataValue => self.start_data(event),
            ErrorState::DataDiscard(mut value) => {
                if let Some(probe) = &mut self.turn_steer_data_probe {
                    probe.event(event);
                }
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ErrorState::MessageNameStart
                } else {
                    ErrorState::DataDiscard(value)
                }
            }
            ErrorState::MessageNameStart => self.start_exact_name(event, false),
            ErrorState::MessageName(probe) => {
                self.finish_exact_name(probe, event, ErrorState::MessageValue)
            }
            ErrorState::MessageValue => self.start_message(event),
            ErrorState::Message => self.message_event(event),
            ErrorState::AfterMessage => self.after_message(event),
            ErrorState::Remainder(depth) => self.remainder_event(depth, event),
            ErrorState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ErrorState::Complete
                } else {
                    ErrorState::Fallback(value)
                }
            }
            ErrorState::Complete => ErrorState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> ErrorState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            return ErrorState::CodeNameStart;
        }
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return ErrorState::Complete;
        }
        if value.is_complete() {
            ErrorState::Complete
        } else {
            ErrorState::Fallback(value)
        }
    }

    fn start_exact_name(&mut self, event: Event, is_code: bool) -> ErrorState {
        match event {
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset(1);
                if is_code {
                    ErrorState::CodeName(probe)
                } else {
                    ErrorState::MessageName(probe)
                }
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_exact_name(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
        next: ErrorState,
    ) -> ErrorState {
        if !matches!(event, Event::ScalarEnd(ScalarKind::Name)) {
            if !matches!(event, Event::ScalarFragment(ScalarKind::Name)) {
                self.malformed = true;
            }
            return match next {
                ErrorState::CodeValue => ErrorState::CodeName(probe),
                ErrorState::MessageValue => ErrorState::MessageName(probe),
                _ => unreachable!("exact error name has one value successor"),
            };
        }
        let expected = match next {
            ErrorState::CodeValue => CODE_ERROR_NAME[0],
            ErrorState::MessageValue => MESSAGE_ERROR_NAME[0],
            _ => unreachable!("exact error name has one value successor"),
        };
        if probe.exact(0, expected.len()) {
            next
        } else {
            self.malformed = true;
            ErrorState::Remainder(1)
        }
    }

    fn start_code(&mut self, event: Event) -> ErrorState {
        match event {
            Event::ScalarStart(ScalarKind::Number) => ErrorState::Code(NumberBytes::new()),
            _ => self.start_remainder(event),
        }
    }

    fn start_data_or_message_name(&mut self, event: Event) -> ErrorState {
        match event {
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset(3);
                ErrorState::DataOrMessageName(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_data_or_message_name(&mut self, probe: ClassifierProbe, event: Event) -> ErrorState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => ErrorState::DataOrMessageName(probe),
            Event::ScalarEnd(ScalarKind::Name)
                if probe.exact(0, DATA_OR_MESSAGE_NAMES[0].len()) =>
            {
                self.data_was_present = true;
                ErrorState::DataValue
            }
            Event::ScalarEnd(ScalarKind::Name)
                if probe.exact(1, DATA_OR_MESSAGE_NAMES[1].len()) =>
            {
                ErrorState::MessageValue
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_data(&mut self, event: Event) -> ErrorState {
        let mut probe = TurnSteerErrorDataProbe::new();
        probe.event(event);
        self.turn_steer_data_probe = Some(probe);
        let mut value = ValueTracker::new();
        if !value.event(event) {
            self.malformed = true;
            return ErrorState::Remainder(1);
        }
        if value.is_complete() {
            ErrorState::MessageNameStart
        } else {
            ErrorState::DataDiscard(value)
        }
    }

    fn start_message(&mut self, event: Event) -> ErrorState {
        match event {
            Event::ScalarStart(ScalarKind::String) => ErrorState::Message,
            _ => self.start_remainder(event),
        }
    }

    fn code_event(&mut self, number: NumberBytes, event: Event) -> ErrorState {
        match event {
            Event::ScalarFragment(ScalarKind::Number) => ErrorState::Code(number),
            Event::ScalarEnd(ScalarKind::Number) => {
                self.code = number.parse_i64();
                if self.code.is_none() {
                    self.malformed = true;
                }
                ErrorState::DataOrMessageNameStart
            }
            _ => self.start_remainder(event),
        }
    }

    fn message_event(&mut self, event: Event) -> ErrorState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => ErrorState::Message,
            Event::ScalarEnd(ScalarKind::String) => ErrorState::AfterMessage,
            _ => self.start_remainder(event),
        }
    }

    fn after_message(&mut self, event: Event) -> ErrorState {
        if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
            ErrorState::Complete
        } else {
            self.start_remainder(event)
        }
    }

    fn start_remainder(&mut self, event: Event) -> ErrorState {
        self.malformed = true;
        self.remainder_event(1, event)
    }

    fn remainder_event(&mut self, mut depth: u16, event: Event) -> ErrorState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            ErrorState::Complete
        } else {
            ErrorState::Remainder(depth)
        }
    }

    fn push_diagnostic(&mut self, bytes: &[u8]) {
        if bytes.is_empty() || self.diagnostic_was_truncated {
            return;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.malformed = true;
            return;
        };
        let remaining = JSON_RPC_DIAGNOSTIC_BYTES.saturating_sub(self.diagnostic_len);
        let proposed = remaining.min(bytes.len());
        let retained = (0..=proposed)
            .rev()
            .find(|index| text.is_char_boundary(*index))
            .unwrap_or(0);
        let end = self.diagnostic_len + retained;
        self.diagnostic[self.diagnostic_len..end].copy_from_slice(&bytes[..retained]);
        self.diagnostic_len = end;
        self.diagnostic_was_truncated |= retained < bytes.len();
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, ErrorState::Complete)
    }

    fn take_error(&mut self, family: ResponseFamily) -> Option<crate::JsonRpcError> {
        if !self.is_complete() || self.malformed {
            return None;
        }
        let code = self.code?;
        let diagnostic = std::str::from_utf8(&self.diagnostic[..self.diagnostic_len]).ok()?;
        let verdict =
            turn_steer_rejection_verdict(family, code, self.turn_steer_data_probe.as_ref())
                .or_else(|| turn_interrupt_rejection_verdict(family, code, self.data_was_present))
                .or_else(|| compatibility_rejection_verdict(family, code, self.data_was_present));
        Some(crate::JsonRpcError::projected(
            code,
            diagnostic,
            self.diagnostic_was_truncated,
            self.data_was_present,
            verdict,
        ))
    }
}

fn turn_interrupt_rejection_verdict(
    family: ResponseFamily,
    code: i64,
    data_was_present: bool,
) -> Option<crate::JsonRpcErrorVerdict> {
    (family == ResponseFamily::TurnInterrupt
        && matches!(code, -32_600 | -32_603)
        && !data_was_present)
        .then_some(crate::JsonRpcErrorVerdict::RejectedBeforeCoreInterrupt)
}

fn turn_steer_rejection_verdict(
    family: ResponseFamily,
    code: i64,
    probe: Option<&TurnSteerErrorDataProbe>,
) -> Option<crate::JsonRpcErrorVerdict> {
    if family != ResponseFamily::TurnSteer || code != -32_600 {
        return None;
    }
    probe
        .and_then(TurnSteerErrorDataProbe::turn_kind)
        .map(|turn_kind| crate::JsonRpcErrorVerdict::ActiveTurnNotSteerable { turn_kind })
}

const CODE_ERROR_NAME: [&[u8]; 1] = [b"code"];
const MESSAGE_ERROR_NAME: [&[u8]; 1] = [b"message"];
const DATA_OR_MESSAGE_NAMES: [&[u8]; 2] = [b"data", b"message"];

fn compatibility_rejection_verdict(
    family: ResponseFamily,
    code: i64,
    data_was_present: bool,
) -> Option<crate::JsonRpcErrorVerdict> {
    if code != -32_600 || data_was_present {
        return None;
    }
    let ResponseFamily::Compatibility(probe) = family else {
        return None;
    };
    if matches!(
        probe,
        crate::CompatibilityProbe::ThreadCompactStart
            | crate::CompatibilityProbe::ThreadFork
            | crate::CompatibilityProbe::ThreadInjectItems
            | crate::CompatibilityProbe::ThreadResume
            | crate::CompatibilityProbe::ThreadRollback
            | crate::CompatibilityProbe::TurnInterrupt
            | crate::CompatibilityProbe::TurnStart
            | crate::CompatibilityProbe::TurnSteer
    ) {
        Some(crate::JsonRpcErrorVerdict::CompatibilityProbeRecognized { probe })
    } else {
        None
    }
}

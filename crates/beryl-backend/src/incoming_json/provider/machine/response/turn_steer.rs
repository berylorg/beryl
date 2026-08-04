struct TurnSteerResultMachine {
    state: TurnSteerResultState,
    turn_id: FixedScalar<256>,
    malformed: bool,
}

enum TurnSteerResultState {
    Start,
    Name,
    NameScalar(ExactName),
    Value,
    TurnId,
    AfterTurnId,
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

impl TurnSteerResultMachine {
    const fn new() -> Self {
        Self {
            state: TurnSteerResultState::Start,
            turn_id: FixedScalar::new(),
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            TurnSteerResultState::NameScalar(name) => name.push(bytes),
            TurnSteerResultState::TurnId => self.turn_id.push(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, TurnSteerResultState::Complete);
        self.state = match state {
            TurnSteerResultState::Start => self.start(event),
            TurnSteerResultState::Name => self.name(event),
            TurnSteerResultState::NameScalar(name) => self.name_scalar(name, event),
            TurnSteerResultState::Value => self.value(event),
            TurnSteerResultState::TurnId => self.turn_id_event(event),
            TurnSteerResultState::AfterTurnId => self.after_turn_id(event),
            TurnSteerResultState::Remainder(depth) => self.remainder(depth, event),
            TurnSteerResultState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    TurnSteerResultState::Complete
                } else {
                    TurnSteerResultState::Fallback(value)
                }
            }
            TurnSteerResultState::Complete => TurnSteerResultState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> TurnSteerResultState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            TurnSteerResultState::Name
        } else {
            self.start_fallback(event)
        }
    }

    fn name(&mut self, event: Event) -> TurnSteerResultState {
        match event {
            Event::ScalarStart(ScalarKind::Name) => {
                TurnSteerResultState::NameScalar(ExactName::new(b"turnId"))
            }
            _ => self.start_remainder(event),
        }
    }

    fn name_scalar(&mut self, name: ExactName, event: Event) -> TurnSteerResultState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => TurnSteerResultState::NameScalar(name),
            Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => TurnSteerResultState::Value,
            _ => self.start_remainder(event),
        }
    }

    fn value(&mut self, event: Event) -> TurnSteerResultState {
        if matches!(event, Event::ScalarStart(ScalarKind::String)) {
            TurnSteerResultState::TurnId
        } else {
            self.start_remainder(event)
        }
    }

    fn turn_id_event(&mut self, event: Event) -> TurnSteerResultState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => TurnSteerResultState::TurnId,
            Event::ScalarEnd(ScalarKind::String) => TurnSteerResultState::AfterTurnId,
            _ => self.start_remainder(event),
        }
    }

    fn after_turn_id(&mut self, event: Event) -> TurnSteerResultState {
        if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
            TurnSteerResultState::Complete
        } else {
            self.start_remainder(event)
        }
    }

    fn start_fallback(&mut self, event: Event) -> TurnSteerResultState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            TurnSteerResultState::Complete
        } else {
            TurnSteerResultState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> TurnSteerResultState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> TurnSteerResultState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            TurnSteerResultState::Complete
        } else {
            TurnSteerResultState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, TurnSteerResultState::Complete)
    }

    fn take_response(&mut self) -> Option<crate::TurnSteerResponseWire> {
        if self.malformed || !self.is_complete() {
            return None;
        }
        crate::TurnSteerResponseWire::try_new(self.turn_id.as_str()?)
    }
}

struct TurnSteerErrorDataProbe {
    state: TurnSteerErrorDataState,
    turn_kind: Option<crate::JsonRpcTurnKind>,
}

enum TurnSteerErrorDataState {
    Start,
    NameStart(TurnSteerErrorDataField),
    Name {
        field: TurnSteerErrorDataField,
        name: ExactName,
    },
    Value(TurnSteerErrorDataField),
    String(TurnSteerErrorDataString),
    TurnKind(ClassifierProbe),
    CloseActiveTurn,
    CloseCodexInfo,
    CloseData,
    Complete,
    Rejected,
}

#[derive(Clone, Copy)]
enum TurnSteerErrorDataField {
    Message,
    CodexErrorInfo,
    ActiveTurnNotSteerable,
    TurnKind,
    AdditionalDetails,
}

#[derive(Clone, Copy)]
enum TurnSteerErrorDataString {
    Message,
    AdditionalDetails,
}

impl TurnSteerErrorDataProbe {
    const fn new() -> Self {
        Self {
            state: TurnSteerErrorDataState::Start,
            turn_kind: None,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            TurnSteerErrorDataState::Name { name, .. } => name.push(bytes),
            TurnSteerErrorDataState::TurnKind(probe) => {
                probe.push(bytes, &TURN_STEER_ERROR_TURN_KIND_WIRES);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, TurnSteerErrorDataState::Rejected);
        self.state = match state {
            TurnSteerErrorDataState::Start => {
                if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
                    TurnSteerErrorDataState::NameStart(TurnSteerErrorDataField::Message)
                } else {
                    TurnSteerErrorDataState::Rejected
                }
            }
            TurnSteerErrorDataState::NameStart(field) => {
                if matches!(event, Event::ScalarStart(ScalarKind::Name)) {
                    TurnSteerErrorDataState::Name {
                        field,
                        name: ExactName::new(turn_steer_error_data_field_name(field)),
                    }
                } else {
                    TurnSteerErrorDataState::Rejected
                }
            }
            TurnSteerErrorDataState::Name { field, name } => match event {
                Event::ScalarFragment(ScalarKind::Name) => {
                    TurnSteerErrorDataState::Name { field, name }
                }
                Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => {
                    TurnSteerErrorDataState::Value(field)
                }
                _ => TurnSteerErrorDataState::Rejected,
            },
            TurnSteerErrorDataState::Value(field) => self.start_value(field, event),
            TurnSteerErrorDataState::String(kind) => self.string_event(kind, event),
            TurnSteerErrorDataState::TurnKind(probe) => self.turn_kind_event(probe, event),
            TurnSteerErrorDataState::CloseActiveTurn => {
                if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
                    TurnSteerErrorDataState::CloseCodexInfo
                } else {
                    TurnSteerErrorDataState::Rejected
                }
            }
            TurnSteerErrorDataState::CloseCodexInfo => {
                if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
                    TurnSteerErrorDataState::NameStart(TurnSteerErrorDataField::AdditionalDetails)
                } else {
                    TurnSteerErrorDataState::Rejected
                }
            }
            TurnSteerErrorDataState::CloseData => {
                if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
                    TurnSteerErrorDataState::Complete
                } else {
                    TurnSteerErrorDataState::Rejected
                }
            }
            TurnSteerErrorDataState::Complete | TurnSteerErrorDataState::Rejected => {
                TurnSteerErrorDataState::Rejected
            }
        };
    }

    fn start_value(
        &mut self,
        field: TurnSteerErrorDataField,
        event: Event,
    ) -> TurnSteerErrorDataState {
        match (field, event) {
            (TurnSteerErrorDataField::Message, Event::ScalarStart(ScalarKind::String)) => {
                TurnSteerErrorDataState::String(TurnSteerErrorDataString::Message)
            }
            (
                TurnSteerErrorDataField::CodexErrorInfo,
                Event::ContainerStart(ContainerKind::Object),
            ) => {
                TurnSteerErrorDataState::NameStart(TurnSteerErrorDataField::ActiveTurnNotSteerable)
            }
            (
                TurnSteerErrorDataField::ActiveTurnNotSteerable,
                Event::ContainerStart(ContainerKind::Object),
            ) => TurnSteerErrorDataState::NameStart(TurnSteerErrorDataField::TurnKind),
            (TurnSteerErrorDataField::TurnKind, Event::ScalarStart(ScalarKind::String)) => {
                let mut probe = ClassifierProbe::new();
                probe.reset((1_u16 << TURN_STEER_ERROR_TURN_KIND_WIRES.len()) - 1);
                TurnSteerErrorDataState::TurnKind(probe)
            }
            (TurnSteerErrorDataField::AdditionalDetails, Event::Null) => {
                TurnSteerErrorDataState::CloseData
            }
            (
                TurnSteerErrorDataField::AdditionalDetails,
                Event::ScalarStart(ScalarKind::String),
            ) => TurnSteerErrorDataState::String(TurnSteerErrorDataString::AdditionalDetails),
            _ => TurnSteerErrorDataState::Rejected,
        }
    }

    fn string_event(
        &mut self,
        kind: TurnSteerErrorDataString,
        event: Event,
    ) -> TurnSteerErrorDataState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => TurnSteerErrorDataState::String(kind),
            Event::ScalarEnd(ScalarKind::String) => match kind {
                TurnSteerErrorDataString::Message => {
                    TurnSteerErrorDataState::NameStart(TurnSteerErrorDataField::CodexErrorInfo)
                }
                TurnSteerErrorDataString::AdditionalDetails => TurnSteerErrorDataState::CloseData,
            },
            _ => TurnSteerErrorDataState::Rejected,
        }
    }

    fn turn_kind_event(&mut self, probe: ClassifierProbe, event: Event) -> TurnSteerErrorDataState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => TurnSteerErrorDataState::TurnKind(probe),
            Event::ScalarEnd(ScalarKind::String) => {
                self.turn_kind = TURN_STEER_ERROR_TURN_KIND_WIRES
                    .iter()
                    .enumerate()
                    .find_map(|(index, wire)| {
                        probe
                            .exact(index, wire.len())
                            .then_some(TURN_STEER_ERROR_TURN_KIND_VALUES[index])
                    });
                if self.turn_kind.is_some() {
                    TurnSteerErrorDataState::CloseActiveTurn
                } else {
                    TurnSteerErrorDataState::Rejected
                }
            }
            _ => TurnSteerErrorDataState::Rejected,
        }
    }

    const fn turn_kind(&self) -> Option<crate::JsonRpcTurnKind> {
        if matches!(self.state, TurnSteerErrorDataState::Complete) {
            self.turn_kind
        } else {
            None
        }
    }
}

const fn turn_steer_error_data_field_name(field: TurnSteerErrorDataField) -> &'static [u8] {
    match field {
        TurnSteerErrorDataField::Message => b"message",
        TurnSteerErrorDataField::CodexErrorInfo => b"codexErrorInfo",
        TurnSteerErrorDataField::ActiveTurnNotSteerable => b"activeTurnNotSteerable",
        TurnSteerErrorDataField::TurnKind => b"turnKind",
        TurnSteerErrorDataField::AdditionalDetails => b"additionalDetails",
    }
}

const TURN_STEER_ERROR_TURN_KIND_WIRES: [&[u8]; 2] = [b"review", b"compact"];
const TURN_STEER_ERROR_TURN_KIND_VALUES: [crate::JsonRpcTurnKind; 2] = [
    crate::JsonRpcTurnKind::Review,
    crate::JsonRpcTurnKind::Compact,
];

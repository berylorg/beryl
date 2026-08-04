struct TurnStartResultMachine {
    state: TurnStartResultState,
    field: TurnStartResultField,
    turn_seen: bool,
    turn: TurnStartTurnMachine,
    malformed: bool,
}

enum TurnStartResultState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    Turn,
    Discard(ValueTracker),
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

#[derive(Clone, Copy)]
enum TurnStartResultField {
    Turn,
    Discard,
}

impl TurnStartResultMachine {
    const fn new() -> Self {
        Self {
            state: TurnStartResultState::Start,
            field: TurnStartResultField::Discard,
            turn_seen: false,
            turn: TurnStartTurnMachine::new(),
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            TurnStartResultState::NameScalar(probe) => {
                probe.push(bytes, &TURN_START_RESPONSE_RESULT_FIELDS);
            }
            TurnStartResultState::Turn => self.turn.scratch_bytes(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, TurnStartResultState::Complete);
        self.state = match state {
            TurnStartResultState::Start => self.start(event),
            TurnStartResultState::Name => self.name(event),
            TurnStartResultState::NameScalar(probe) => self.finish_name(probe, event),
            TurnStartResultState::Value => self.start_value(event),
            TurnStartResultState::Turn => self.turn_event(event),
            TurnStartResultState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    TurnStartResultState::Name
                } else {
                    TurnStartResultState::Discard(value)
                }
            }
            TurnStartResultState::Remainder(depth) => self.remainder(depth, event),
            TurnStartResultState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    TurnStartResultState::Complete
                } else {
                    TurnStartResultState::Fallback(value)
                }
            }
            TurnStartResultState::Complete => TurnStartResultState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> TurnStartResultState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            TurnStartResultState::Name
        } else {
            self.start_fallback(event)
        }
    }

    fn name(&mut self, event: Event) -> TurnStartResultState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => TurnStartResultState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset(1);
                TurnStartResultState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> TurnStartResultState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => TurnStartResultState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                self.field = if probe.exact(0, TURN_START_RESPONSE_RESULT_FIELDS[0].len()) {
                    if self.turn_seen {
                        self.malformed = true;
                        TurnStartResultField::Discard
                    } else {
                        self.turn_seen = true;
                        TurnStartResultField::Turn
                    }
                } else {
                    TurnStartResultField::Discard
                };
                TurnStartResultState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_value(&mut self, event: Event) -> TurnStartResultState {
        match self.field {
            TurnStartResultField::Turn => {
                self.turn.event(event);
                if self.turn.is_complete() {
                    TurnStartResultState::Name
                } else {
                    TurnStartResultState::Turn
                }
            }
            TurnStartResultField::Discard => self.start_discard(event),
        }
    }

    fn turn_event(&mut self, event: Event) -> TurnStartResultState {
        self.turn.event(event);
        if self.turn.is_complete() {
            TurnStartResultState::Name
        } else {
            TurnStartResultState::Turn
        }
    }

    fn start_discard(&mut self, event: Event) -> TurnStartResultState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return self.start_remainder(event);
        }
        if value.is_complete() {
            TurnStartResultState::Name
        } else {
            TurnStartResultState::Discard(value)
        }
    }

    fn start_fallback(&mut self, event: Event) -> TurnStartResultState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            TurnStartResultState::Complete
        } else {
            TurnStartResultState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> TurnStartResultState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> TurnStartResultState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            TurnStartResultState::Complete
        } else {
            TurnStartResultState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, TurnStartResultState::Complete)
    }

    fn take_response(&mut self) -> Option<crate::turn::TurnStartResponseWire> {
        if self.malformed || !self.is_complete() || !self.turn_seen {
            return None;
        }
        self.turn.take_response()
    }
}

const TURN_START_RESPONSE_RESULT_FIELDS: [&[u8]; 1] = [b"turn"];

struct TurnStartTurnMachine {
    state: TurnStartTurnState,
    next_target: usize,
    field: TurnStartTurnField,
    id: FixedScalar<256>,
    status: Option<crate::TurnStatus>,
    malformed: bool,
}

enum TurnStartTurnState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    Identity,
    Items(ValueTracker),
    Status(ClassifierProbe),
    Discard(ValueTracker),
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

#[derive(Clone, Copy)]
enum TurnStartTurnField {
    Identity,
    Items,
    Status,
    Discard,
}

impl TurnStartTurnMachine {
    const fn new() -> Self {
        Self {
            state: TurnStartTurnState::Start,
            next_target: 0,
            field: TurnStartTurnField::Discard,
            id: FixedScalar::new(),
            status: None,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            TurnStartTurnState::NameScalar(probe) => {
                probe.push(bytes, &TURN_START_TURN_FIELDS);
            }
            TurnStartTurnState::Identity => self.id.push(bytes),
            TurnStartTurnState::Status(probe) => {
                probe.push(bytes, &TURN_START_STATUS_WIRES);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, TurnStartTurnState::Complete);
        self.state = match state {
            TurnStartTurnState::Start => self.start(event),
            TurnStartTurnState::Name => self.name(event),
            TurnStartTurnState::NameScalar(probe) => self.finish_name(probe, event),
            TurnStartTurnState::Value => self.start_value(event),
            TurnStartTurnState::Identity => self.identity_event(event),
            TurnStartTurnState::Items(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    TurnStartTurnState::Name
                } else {
                    TurnStartTurnState::Items(value)
                }
            }
            TurnStartTurnState::Status(probe) => self.finish_status(probe, event),
            TurnStartTurnState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    TurnStartTurnState::Name
                } else {
                    TurnStartTurnState::Discard(value)
                }
            }
            TurnStartTurnState::Remainder(depth) => self.remainder(depth, event),
            TurnStartTurnState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    TurnStartTurnState::Complete
                } else {
                    TurnStartTurnState::Fallback(value)
                }
            }
            TurnStartTurnState::Complete => TurnStartTurnState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> TurnStartTurnState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            TurnStartTurnState::Name
        } else {
            self.start_fallback(event)
        }
    }

    fn name(&mut self, event: Event) -> TurnStartTurnState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => TurnStartTurnState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset((1_u16 << TURN_START_TURN_FIELDS.len()) - 1);
                TurnStartTurnState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> TurnStartTurnState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => TurnStartTurnState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                let target = TURN_START_TURN_FIELDS
                    .iter()
                    .enumerate()
                    .find_map(|(index, wire)| probe.exact(index, wire.len()).then_some(index));
                self.field = match target {
                    Some(index) if index == self.next_target => {
                        self.next_target += 1;
                        TURN_START_TURN_FIELD_VALUES[index]
                    }
                    Some(_) => {
                        self.malformed = true;
                        TurnStartTurnField::Discard
                    }
                    None => TurnStartTurnField::Discard,
                };
                TurnStartTurnState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_value(&mut self, event: Event) -> TurnStartTurnState {
        match self.field {
            TurnStartTurnField::Identity => match event {
                Event::ScalarStart(ScalarKind::String) => TurnStartTurnState::Identity,
                _ => self.start_remainder(event),
            },
            TurnStartTurnField::Items => {
                let mut value = ValueTracker::new();
                if !value.event(event) {
                    return self.start_remainder(event);
                }
                if value.is_complete() {
                    TurnStartTurnState::Name
                } else {
                    TurnStartTurnState::Items(value)
                }
            }
            TurnStartTurnField::Status => match event {
                Event::ScalarStart(ScalarKind::String) => {
                    let mut probe = ClassifierProbe::new();
                    probe.reset((1_u16 << TURN_START_STATUS_WIRES.len()) - 1);
                    TurnStartTurnState::Status(probe)
                }
                _ => self.start_remainder(event),
            },
            TurnStartTurnField::Discard => self.start_discard(event),
        }
    }

    fn identity_event(&mut self, event: Event) -> TurnStartTurnState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => TurnStartTurnState::Identity,
            Event::ScalarEnd(ScalarKind::String) => TurnStartTurnState::Name,
            _ => self.start_remainder(event),
        }
    }

    fn finish_status(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> TurnStartTurnState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => TurnStartTurnState::Status(probe),
            Event::ScalarEnd(ScalarKind::String) => {
                self.status = TURN_START_STATUS_WIRES
                    .iter()
                    .enumerate()
                    .find_map(|(index, wire)| {
                        probe
                            .exact(index, wire.len())
                            .then_some(TURN_START_STATUS_VALUES[index])
                    });
                self.malformed |= self.status.is_none();
                TurnStartTurnState::Name
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_discard(&mut self, event: Event) -> TurnStartTurnState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return self.start_remainder(event);
        }
        if value.is_complete() {
            TurnStartTurnState::Name
        } else {
            TurnStartTurnState::Discard(value)
        }
    }

    fn start_fallback(&mut self, event: Event) -> TurnStartTurnState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            TurnStartTurnState::Complete
        } else {
            TurnStartTurnState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> TurnStartTurnState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> TurnStartTurnState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            TurnStartTurnState::Complete
        } else {
            TurnStartTurnState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, TurnStartTurnState::Complete)
    }

    fn take_response(&mut self) -> Option<crate::turn::TurnStartResponseWire> {
        if self.malformed
            || !self.is_complete()
            || self.next_target != TURN_START_TURN_FIELDS.len()
        {
            return None;
        }
        crate::turn::TurnStartResponseWire::try_new(self.id.as_str()?, self.status.take()?)
    }
}

const TURN_START_TURN_FIELDS: [&[u8]; 3] = [b"id", b"items", b"status"];
const TURN_START_TURN_FIELD_VALUES: [TurnStartTurnField; 3] = [
    TurnStartTurnField::Identity,
    TurnStartTurnField::Items,
    TurnStartTurnField::Status,
];
const TURN_START_STATUS_WIRES: [&[u8]; 4] = [
    b"completed",
    b"interrupted",
    b"failed",
    b"inProgress",
];
const TURN_START_STATUS_VALUES: [crate::TurnStatus; 4] = [
    crate::TurnStatus::Completed,
    crate::TurnStatus::Interrupted,
    crate::TurnStatus::Failed,
    crate::TurnStatus::InProgress,
];

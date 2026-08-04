struct UnsubscribeResultMachine {
    state: UnsubscribeState,
    status: Option<crate::ThreadUnsubscribeStatus>,
    malformed: bool,
}

enum UnsubscribeState {
    Start,
    Name,
    NameScalar(ExactName),
    Value,
    Status(ClassifierProbe),
    AfterStatus,
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

impl UnsubscribeResultMachine {
    const fn new() -> Self {
        Self {
            state: UnsubscribeState::Start,
            status: None,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            UnsubscribeState::NameScalar(name) => name.push(bytes),
            UnsubscribeState::Status(probe) => {
                probe.push(bytes, &UNSUBSCRIBE_STATUS_WIRES);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, UnsubscribeState::Complete);
        self.state = match state {
            UnsubscribeState::Start => {
                if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
                    UnsubscribeState::Name
                } else {
                    self.start_fallback(event)
                }
            }
            UnsubscribeState::Name => match event {
                Event::ScalarStart(ScalarKind::Name) => {
                    UnsubscribeState::NameScalar(ExactName::new(b"status"))
                }
                _ => self.start_remainder(event),
            },
            UnsubscribeState::NameScalar(name) => match event {
                Event::ScalarFragment(ScalarKind::Name) => UnsubscribeState::NameScalar(name),
                Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => UnsubscribeState::Value,
                _ => self.start_remainder(event),
            },
            UnsubscribeState::Value => match event {
                Event::ScalarStart(ScalarKind::String) => {
                    let mut probe = ClassifierProbe::new();
                    probe.reset(7);
                    UnsubscribeState::Status(probe)
                }
                _ => self.start_remainder(event),
            },
            UnsubscribeState::Status(probe) => self.finish_status(probe, event),
            UnsubscribeState::AfterStatus => {
                if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
                    UnsubscribeState::Complete
                } else {
                    self.start_remainder(event)
                }
            }
            UnsubscribeState::Remainder(depth) => self.remainder(depth, event),
            UnsubscribeState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    UnsubscribeState::Complete
                } else {
                    UnsubscribeState::Fallback(value)
                }
            }
            UnsubscribeState::Complete => UnsubscribeState::Complete,
        };
    }

    fn finish_status(&mut self, probe: ClassifierProbe, event: Event) -> UnsubscribeState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => UnsubscribeState::Status(probe),
            Event::ScalarEnd(ScalarKind::String) => {
                self.status =
                    UNSUBSCRIBE_STATUS_WIRES
                        .iter()
                        .enumerate()
                        .find_map(|(index, wire)| {
                            probe
                                .exact(index, wire.len())
                                .then_some(UNSUBSCRIBE_STATUS_VALUES[index])
                        });
                self.malformed |= self.status.is_none();
                UnsubscribeState::AfterStatus
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_fallback(&mut self, event: Event) -> UnsubscribeState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            UnsubscribeState::Complete
        } else {
            UnsubscribeState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> UnsubscribeState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> UnsubscribeState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            UnsubscribeState::Complete
        } else {
            UnsubscribeState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, UnsubscribeState::Complete)
    }

    fn take_status(&mut self) -> Option<crate::ThreadUnsubscribeStatus> {
        if self.malformed || !self.is_complete() {
            None
        } else {
            self.status.take()
        }
    }
}

const UNSUBSCRIBE_STATUS_WIRES: [&[u8]; 3] = [b"notLoaded", b"notSubscribed", b"unsubscribed"];
const UNSUBSCRIBE_STATUS_VALUES: [crate::ThreadUnsubscribeStatus; 3] = [
    crate::ThreadUnsubscribeStatus::NotLoaded,
    crate::ThreadUnsubscribeStatus::NotSubscribed,
    crate::ThreadUnsubscribeStatus::Unsubscribed,
];

enum CompatibilityResultMachine {
    ConfigRead(ConfigResultMachine),
    ModelList(ModelPageMachine),
    ThreadUnsubscribe(UnsubscribeResultMachine),
    UnexpectedMutatingSuccess {
        probe: crate::CompatibilityProbe,
        schema: OrderedObjectMachine,
    },
}

impl CompatibilityResultMachine {
    fn new(probe: crate::CompatibilityProbe) -> Self {
        match probe {
            crate::CompatibilityProbe::ConfigRead => Self::ConfigRead(ConfigResultMachine::new()),
            crate::CompatibilityProbe::ModelList => Self::ModelList(ModelPageMachine::new()),
            crate::CompatibilityProbe::ThreadUnsubscribe => {
                Self::ThreadUnsubscribe(UnsubscribeResultMachine::new())
            }
            probe => Self::UnexpectedMutatingSuccess {
                probe,
                schema: OrderedObjectMachine::new(compatibility_success_fields(probe)),
            },
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match self {
            Self::ConfigRead(machine) => machine.scratch_bytes(bytes),
            Self::ModelList(machine) => machine.scratch_bytes(bytes),
            Self::ThreadUnsubscribe(machine) => machine.scratch_bytes(bytes),
            Self::UnexpectedMutatingSuccess { schema, .. } => schema.scratch_bytes(bytes),
        }
    }

    fn event(&mut self, event: Event) {
        match self {
            Self::ConfigRead(machine) => machine.event(event),
            Self::ModelList(machine) => machine.event(event),
            Self::ThreadUnsubscribe(machine) => machine.event(event),
            Self::UnexpectedMutatingSuccess { schema, .. } => schema.event(event),
        }
    }

    fn is_complete(&self) -> bool {
        match self {
            Self::ConfigRead(machine) => machine.is_complete(),
            Self::ModelList(machine) => machine.is_complete(),
            Self::ThreadUnsubscribe(machine) => machine.is_complete(),
            Self::UnexpectedMutatingSuccess { schema, .. } => schema.is_complete(),
        }
    }

    fn take_result(&mut self) -> Option<crate::CompatibilityProbeResult> {
        match self {
            Self::ConfigRead(machine) => machine
                .take_response()
                .map(crate::CompatibilityProbeResult::ConfigRead),
            Self::ModelList(machine) => machine
                .take_page()
                .map(crate::CompatibilityProbeResult::ModelList),
            Self::ThreadUnsubscribe(machine) => machine
                .take_status()
                .map(crate::CompatibilityProbeResult::ThreadUnsubscribe),
            Self::UnexpectedMutatingSuccess { probe, schema } if schema.is_valid() => {
                crate::CompatibilityProbeResult::unexpected_mutating_success(*probe)
            }
            Self::UnexpectedMutatingSuccess { .. } => None,
        }
    }
}

const fn compatibility_success_fields(probe: crate::CompatibilityProbe) -> &'static [OrderedField] {
    match probe {
        crate::CompatibilityProbe::ThreadCompactStart
        | crate::CompatibilityProbe::ThreadInjectItems
        | crate::CompatibilityProbe::TurnInterrupt => &EMPTY_RESULT_FIELDS,
        crate::CompatibilityProbe::ThreadFork => &THREAD_FORK_RESULT_FIELDS,
        crate::CompatibilityProbe::ThreadResume => &THREAD_RESUME_RESULT_FIELDS,
        crate::CompatibilityProbe::ThreadRollback => &THREAD_ROLLBACK_RESULT_FIELDS,
        crate::CompatibilityProbe::TurnStart => &TURN_START_RESULT_FIELDS,
        crate::CompatibilityProbe::TurnSteer => &TURN_STEER_RESULT_FIELDS,
        crate::CompatibilityProbe::ConfigRead
        | crate::CompatibilityProbe::ModelList
        | crate::CompatibilityProbe::ThreadUnsubscribe => &EMPTY_RESULT_FIELDS,
    }
}

const EMPTY_RESULT_FIELDS: [OrderedField; 0] = [];
const THREAD_ROLLBACK_RESULT_FIELDS: [OrderedField; 1] = [OrderedField::object(b"thread")];
const TURN_START_RESULT_FIELDS: [OrderedField; 1] = [OrderedField::object(b"turn")];
const TURN_STEER_RESULT_FIELDS: [OrderedField; 1] = [OrderedField::string(b"turnId")];

const THREAD_FORK_RESULT_FIELDS: [OrderedField; 13] = [
    OrderedField::object(b"thread"),
    OrderedField::any(b"model"),
    OrderedField::any(b"modelProvider"),
    OrderedField::any(b"serviceTier"),
    OrderedField::any(b"cwd"),
    OrderedField::any(b"runtimeWorkspaceRoots"),
    OrderedField::any(b"instructionSources"),
    OrderedField::any(b"approvalPolicy"),
    OrderedField::any(b"approvalsReviewer"),
    OrderedField::any(b"sandbox"),
    OrderedField::any(b"activePermissionProfile"),
    OrderedField::any(b"reasoningEffort"),
    OrderedField::any(b"multiAgentMode"),
];

const THREAD_RESUME_RESULT_FIELDS: [OrderedField; 16] = [
    OrderedField::object(b"thread"),
    OrderedField::any(b"model"),
    OrderedField::any(b"modelProvider"),
    OrderedField::any(b"serviceTier"),
    OrderedField::any(b"cwd"),
    OrderedField::any(b"runtimeWorkspaceRoots"),
    OrderedField::any(b"instructionSources"),
    OrderedField::any(b"approvalPolicy"),
    OrderedField::any(b"approvalsReviewer"),
    OrderedField::any(b"sandbox"),
    OrderedField::any(b"activePermissionProfile"),
    OrderedField::any(b"reasoningEffort"),
    OrderedField::any(b"multiAgentMode"),
    OrderedField::any(b"initialTurnsPage"),
    OrderedField::any(b"turnsBackwardsCursor"),
    OrderedField::any(b"itemsBackwardsCursor"),
];

struct ThreadStatusMachine {
    state: ThreadStatusState,
    field: ThreadStatusField,
    kind: Option<ThreadStatusKind>,
    type_seen: bool,
    active_flags_seen: bool,
    active_flags: u8,
    malformed: bool,
}

enum ThreadStatusState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    Type(ClassifierProbe),
    ActiveFlagsArray,
    ActiveFlag(ClassifierProbe),
    Discard(ValueTracker),
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

#[derive(Clone, Copy)]
enum ThreadStatusField {
    Type,
    ActiveFlags,
    Discard,
}

#[derive(Clone, Copy)]
enum ThreadStatusKind {
    NotLoaded,
    Idle,
    SystemError,
    Active,
}

impl ThreadStatusKind {
    const fn wire(self) -> &'static str {
        match self {
            Self::NotLoaded => "notLoaded",
            Self::Idle => "idle",
            Self::SystemError => "systemError",
            Self::Active => "active",
        }
    }
}

impl ThreadStatusMachine {
    const fn new() -> Self {
        Self {
            state: ThreadStatusState::Start,
            field: ThreadStatusField::Discard,
            kind: None,
            type_seen: false,
            active_flags_seen: false,
            active_flags: 0,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ThreadStatusState::NameScalar(probe) => {
                probe.push(bytes, &THREAD_STATUS_FIELDS);
            }
            ThreadStatusState::Type(probe) => {
                probe.push(bytes, &THREAD_STATUS_WIRES);
            }
            ThreadStatusState::ActiveFlag(probe) => {
                probe.push(bytes, &THREAD_ACTIVE_FLAG_WIRES);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, ThreadStatusState::Complete);
        self.state = match state {
            ThreadStatusState::Start => self.start(event),
            ThreadStatusState::Name => self.name(event),
            ThreadStatusState::NameScalar(probe) => self.finish_name(probe, event),
            ThreadStatusState::Value => self.start_value(event),
            ThreadStatusState::Type(probe) => self.finish_type(probe, event),
            ThreadStatusState::ActiveFlagsArray => self.active_flags_event(event),
            ThreadStatusState::ActiveFlag(probe) => self.finish_active_flag(probe, event),
            ThreadStatusState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ThreadStatusState::Name
                } else {
                    ThreadStatusState::Discard(value)
                }
            }
            ThreadStatusState::Remainder(depth) => self.remainder(depth, event),
            ThreadStatusState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ThreadStatusState::Complete
                } else {
                    ThreadStatusState::Fallback(value)
                }
            }
            ThreadStatusState::Complete => ThreadStatusState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> ThreadStatusState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            ThreadStatusState::Name
        } else {
            self.start_fallback(event)
        }
    }

    fn name(&mut self, event: Event) -> ThreadStatusState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => ThreadStatusState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset((1_u16 << THREAD_STATUS_FIELDS.len()) - 1);
                ThreadStatusState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(&mut self, probe: ClassifierProbe, event: Event) -> ThreadStatusState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => ThreadStatusState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                let target = THREAD_STATUS_FIELDS
                    .iter()
                    .enumerate()
                    .find_map(|(index, wire)| probe.exact(index, wire.len()).then_some(index));
                self.field = match target {
                    Some(TYPE_FIELD) => {
                        self.malformed |= self.type_seen;
                        self.type_seen = true;
                        ThreadStatusField::Type
                    }
                    Some(ACTIVE_FLAGS_FIELD) => {
                        self.malformed |= self.active_flags_seen || !self.type_seen;
                        self.active_flags_seen = true;
                        ThreadStatusField::ActiveFlags
                    }
                    Some(_) => unreachable!("thread-status field classifier is closed"),
                    None => ThreadStatusField::Discard,
                };
                ThreadStatusState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_value(&mut self, event: Event) -> ThreadStatusState {
        match self.field {
            ThreadStatusField::Type => self.start_type(event),
            ThreadStatusField::ActiveFlags => {
                if !matches!(self.kind, Some(ThreadStatusKind::Active)) {
                    self.malformed = true;
                }
                self.start_active_flags(event)
            }
            ThreadStatusField::Discard => {
                let mut value = ValueTracker::new();
                if !value.event(event) {
                    return self.start_remainder(event);
                }
                if value.is_complete() {
                    ThreadStatusState::Name
                } else {
                    ThreadStatusState::Discard(value)
                }
            }
        }
    }

    fn start_type(&mut self, event: Event) -> ThreadStatusState {
        match event {
            Event::ScalarStart(ScalarKind::String) => {
                let mut probe = ClassifierProbe::new();
                probe.reset(15);
                ThreadStatusState::Type(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_type(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> ThreadStatusState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => ThreadStatusState::Type(probe),
            Event::ScalarEnd(ScalarKind::String) => {
                self.kind = THREAD_STATUS_WIRES
                    .iter()
                    .enumerate()
                    .find_map(|(index, wire)| {
                        probe
                            .exact(index, wire.len())
                            .then_some(THREAD_STATUS_KINDS[index])
                    });
                self.malformed |= self.kind.is_none();
                ThreadStatusState::Name
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_active_flags(&mut self, event: Event) -> ThreadStatusState {
        if matches!(event, Event::ContainerStart(ContainerKind::Array)) {
            ThreadStatusState::ActiveFlagsArray
        } else {
            self.start_remainder(event)
        }
    }

    fn active_flags_event(&mut self, event: Event) -> ThreadStatusState {
        match event {
            Event::ContainerEnd(ContainerKind::Array) => ThreadStatusState::Name,
            Event::ScalarStart(ScalarKind::String) => {
                let mut probe = ClassifierProbe::new();
                probe.reset(3);
                ThreadStatusState::ActiveFlag(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_active_flag(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> ThreadStatusState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => ThreadStatusState::ActiveFlag(probe),
            Event::ScalarEnd(ScalarKind::String) => {
                let bit = THREAD_ACTIVE_FLAG_WIRES
                    .iter()
                    .enumerate()
                    .find_map(|(index, wire)| {
                        probe.exact(index, wire.len()).then_some(1_u8 << index)
                    });
                match bit {
                    Some(bit) if self.active_flags & bit == 0 => self.active_flags |= bit,
                    Some(_) => self.malformed = true,
                    None => {}
                }
                ThreadStatusState::ActiveFlagsArray
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_fallback(&mut self, event: Event) -> ThreadStatusState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            ThreadStatusState::Complete
        } else {
            ThreadStatusState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> ThreadStatusState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> ThreadStatusState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            ThreadStatusState::Complete
        } else {
            ThreadStatusState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, ThreadStatusState::Complete)
    }

    fn take_status(&mut self) -> Option<crate::ThreadStatus> {
        let kind = self.kind?;
        let active = matches!(kind, ThreadStatusKind::Active);
        if self.malformed
            || !self.is_complete()
            || !self.type_seen
            || active != self.active_flags_seen
        {
            return None;
        }
        crate::ThreadStatus::from_bounded_wire(
            kind.wire(),
            self.active_flags & 1 != 0,
            self.active_flags & 2 != 0,
        )
    }
}

const TYPE_FIELD: usize = 0;
const ACTIVE_FLAGS_FIELD: usize = 1;
const THREAD_STATUS_FIELDS: [&[u8]; 2] = [b"type", b"activeFlags"];
const THREAD_STATUS_WIRES: [&[u8]; 4] = [b"notLoaded", b"idle", b"systemError", b"active"];
const THREAD_STATUS_KINDS: [ThreadStatusKind; 4] = [
    ThreadStatusKind::NotLoaded,
    ThreadStatusKind::Idle,
    ThreadStatusKind::SystemError,
    ThreadStatusKind::Active,
];
const THREAD_ACTIVE_FLAG_WIRES: [&[u8]; 2] = [b"waitingOnApproval", b"waitingOnUserInput"];

struct ThreadReadThreadMachine {
    state: ThreadReadThreadState,
    next_target: usize,
    field: ThreadReadThreadField,
    id: FixedScalar<256>,
    model_provider: FixedScalar<256>,
    status_machine: ThreadStatusMachine,
    status: Option<crate::ThreadStatus>,
    source_machine: ThreadReadSourceMachine,
    agent_nickname: FixedScalar<1_024>,
    agent_nickname_is_null: bool,
    malformed: bool,
}

enum ThreadReadThreadState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    String,
    Status,
    Source,
    Discard(ValueTracker),
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

#[derive(Clone, Copy)]
enum ThreadReadThreadField {
    Identity,
    ModelProvider,
    Status,
    Source,
    AgentNickname,
    Discard,
}

impl ThreadReadThreadMachine {
    const fn new() -> Self {
        Self {
            state: ThreadReadThreadState::Start,
            next_target: 0,
            field: ThreadReadThreadField::Discard,
            id: FixedScalar::new(),
            model_provider: FixedScalar::new(),
            status_machine: ThreadStatusMachine::new(),
            status: None,
            source_machine: ThreadReadSourceMachine::new(),
            agent_nickname: FixedScalar::new(),
            agent_nickname_is_null: false,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ThreadReadThreadState::NameScalar(probe) => {
                probe.push(bytes, &THREAD_READ_THREAD_TARGET_FIELDS);
            }
            ThreadReadThreadState::String => match self.field {
                ThreadReadThreadField::Identity => self.id.push(bytes),
                ThreadReadThreadField::ModelProvider => self.model_provider.push(bytes),
                ThreadReadThreadField::AgentNickname => self.agent_nickname.push(bytes),
                _ => self.malformed = true,
            },
            ThreadReadThreadState::Status => self.status_machine.scratch_bytes(bytes),
            ThreadReadThreadState::Source => self.source_machine.scratch_bytes(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, ThreadReadThreadState::Complete);
        self.state = match state {
            ThreadReadThreadState::Start => self.start(event),
            ThreadReadThreadState::Name => self.name(event),
            ThreadReadThreadState::NameScalar(probe) => self.finish_name(probe, event),
            ThreadReadThreadState::Value => self.start_value(event),
            ThreadReadThreadState::String => self.string_event(event),
            ThreadReadThreadState::Status => self.status_event(event),
            ThreadReadThreadState::Source => self.source_event(event),
            ThreadReadThreadState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ThreadReadThreadState::Name
                } else {
                    ThreadReadThreadState::Discard(value)
                }
            }
            ThreadReadThreadState::Remainder(depth) => self.remainder(depth, event),
            ThreadReadThreadState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ThreadReadThreadState::Complete
                } else {
                    ThreadReadThreadState::Fallback(value)
                }
            }
            ThreadReadThreadState::Complete => ThreadReadThreadState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> ThreadReadThreadState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            ThreadReadThreadState::Name
        } else {
            self.start_fallback(event)
        }
    }

    fn name(&mut self, event: Event) -> ThreadReadThreadState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => ThreadReadThreadState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset((1_u16 << THREAD_READ_THREAD_TARGET_FIELDS.len()) - 1);
                ThreadReadThreadState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> ThreadReadThreadState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => ThreadReadThreadState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                let target = THREAD_READ_THREAD_TARGET_FIELDS
                    .iter()
                    .enumerate()
                    .find_map(|(index, wire)| probe.exact(index, wire.len()).then_some(index));
                self.field = match target {
                    Some(index) if index == self.next_target => {
                        self.next_target += 1;
                        THREAD_READ_THREAD_TARGET_VALUES[index]
                    }
                    Some(_) => {
                        self.malformed = true;
                        ThreadReadThreadField::Discard
                    }
                    None => ThreadReadThreadField::Discard,
                };
                ThreadReadThreadState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_value(&mut self, event: Event) -> ThreadReadThreadState {
        match self.field {
            ThreadReadThreadField::Identity | ThreadReadThreadField::ModelProvider => {
                if matches!(event, Event::ScalarStart(ScalarKind::String)) {
                    ThreadReadThreadState::String
                } else {
                    self.start_remainder(event)
                }
            }
            ThreadReadThreadField::AgentNickname => match event {
                Event::ScalarStart(ScalarKind::String) => ThreadReadThreadState::String,
                Event::Null => {
                    self.agent_nickname_is_null = true;
                    ThreadReadThreadState::Name
                }
                _ => self.start_remainder(event),
            },
            ThreadReadThreadField::Status => {
                self.status_machine.event(event);
                if self.status_machine.is_complete() {
                    self.finish_status()
                } else {
                    ThreadReadThreadState::Status
                }
            }
            ThreadReadThreadField::Source => {
                self.source_machine.event(event);
                if self.source_machine.is_complete() {
                    ThreadReadThreadState::Name
                } else {
                    ThreadReadThreadState::Source
                }
            }
            ThreadReadThreadField::Discard => self.start_discard(event),
        }
    }

    fn string_event(&mut self, event: Event) -> ThreadReadThreadState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => ThreadReadThreadState::String,
            Event::ScalarEnd(ScalarKind::String) => ThreadReadThreadState::Name,
            _ => self.start_remainder(event),
        }
    }

    fn status_event(&mut self, event: Event) -> ThreadReadThreadState {
        self.status_machine.event(event);
        if self.status_machine.is_complete() {
            self.finish_status()
        } else {
            ThreadReadThreadState::Status
        }
    }

    fn finish_status(&mut self) -> ThreadReadThreadState {
        self.status = self.status_machine.take_status();
        self.malformed |= self.status.is_none();
        ThreadReadThreadState::Name
    }

    fn source_event(&mut self, event: Event) -> ThreadReadThreadState {
        self.source_machine.event(event);
        if self.source_machine.is_complete() {
            ThreadReadThreadState::Name
        } else {
            ThreadReadThreadState::Source
        }
    }

    fn start_discard(&mut self, event: Event) -> ThreadReadThreadState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return self.start_remainder(event);
        }
        if value.is_complete() {
            ThreadReadThreadState::Name
        } else {
            ThreadReadThreadState::Discard(value)
        }
    }

    fn start_fallback(&mut self, event: Event) -> ThreadReadThreadState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            ThreadReadThreadState::Complete
        } else {
            ThreadReadThreadState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> ThreadReadThreadState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> ThreadReadThreadState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            ThreadReadThreadState::Complete
        } else {
            ThreadReadThreadState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, ThreadReadThreadState::Complete)
    }

    fn take_parts(&mut self) -> Option<(&str, crate::ThreadStatus, &str, Option<&str>)> {
        if self.malformed
            || !self.is_complete()
            || self.next_target != THREAD_READ_THREAD_TARGET_FIELDS.len()
        {
            return None;
        }
        let status = self.status.take()?;
        let id = self.id.as_str()?;
        let model_provider = self.model_provider.as_str()?;
        let top_level = if self.agent_nickname_is_null {
            ThreadReadSourceNickname::Null
        } else {
            ThreadReadSourceNickname::Value(self.agent_nickname.as_str()?)
        };
        let nickname = match (self.source_machine.nickname()?, top_level) {
            (ThreadReadSourceNickname::Absent, ThreadReadSourceNickname::Null) => None,
            (ThreadReadSourceNickname::Absent, ThreadReadSourceNickname::Value(top_level)) => {
                Some(top_level)
            }
            (ThreadReadSourceNickname::Null, ThreadReadSourceNickname::Null) => None,
            (ThreadReadSourceNickname::Value(nested), ThreadReadSourceNickname::Value(top_level))
                if nested == top_level =>
            {
                Some(top_level)
            }
            (ThreadReadSourceNickname::Null, ThreadReadSourceNickname::Value(_))
            | (ThreadReadSourceNickname::Value(_), ThreadReadSourceNickname::Null)
            | (ThreadReadSourceNickname::Value(_), ThreadReadSourceNickname::Value(_)) => {
                return None;
            }
            (ThreadReadSourceNickname::Absent, ThreadReadSourceNickname::Absent)
            | (ThreadReadSourceNickname::Null, ThreadReadSourceNickname::Absent)
            | (ThreadReadSourceNickname::Value(_), ThreadReadSourceNickname::Absent) => {
                unreachable!("the required top-level nickname field is always present")
            }
        };
        Some((id, status, model_provider, nickname))
    }
}

const THREAD_READ_THREAD_TARGET_FIELDS: [&[u8]; 5] = [
    b"id",
    b"modelProvider",
    b"status",
    b"source",
    b"agentNickname",
];

const THREAD_READ_THREAD_TARGET_VALUES: [ThreadReadThreadField; 5] = [
    ThreadReadThreadField::Identity,
    ThreadReadThreadField::ModelProvider,
    ThreadReadThreadField::Status,
    ThreadReadThreadField::Source,
    ThreadReadThreadField::AgentNickname,
];

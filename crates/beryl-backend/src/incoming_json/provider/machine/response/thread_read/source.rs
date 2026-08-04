struct ThreadReadSourceMachine {
    state: ThreadReadSourceState,
    source_field: ThreadReadSourceField,
    sub_agent_field: ThreadReadSubAgentField,
    spawn_field: ThreadReadSpawnField,
    sub_agent_seen: bool,
    thread_spawn_seen: bool,
    nickname_seen: bool,
    nickname_is_null: bool,
    nickname: FixedScalar<1_024>,
    malformed: bool,
}

enum ThreadReadSourceState {
    Start,
    SourceName,
    SourceNameScalar(ClassifierProbe),
    SourceValue,
    SubAgentName,
    SubAgentNameScalar(ClassifierProbe),
    SubAgentValue,
    SpawnName,
    SpawnNameScalar(ClassifierProbe),
    SpawnValue,
    Nickname,
    Discard(ValueTracker, ThreadReadSourceResume),
    Remainder(u16),
    Complete,
}

#[derive(Clone, Copy)]
enum ThreadReadSourceField {
    SubAgent,
    Discard,
}

#[derive(Clone, Copy)]
enum ThreadReadSubAgentField {
    ThreadSpawn,
    Discard,
}

#[derive(Clone, Copy)]
enum ThreadReadSpawnField {
    Nickname,
    Discard,
}

#[derive(Clone, Copy)]
enum ThreadReadSourceResume {
    SourceName,
    SubAgentName,
    SpawnName,
    Complete,
}

enum ThreadReadSourceNickname<'a> {
    Absent,
    Null,
    Value(&'a str),
}

impl ThreadReadSourceMachine {
    const fn new() -> Self {
        Self {
            state: ThreadReadSourceState::Start,
            source_field: ThreadReadSourceField::Discard,
            sub_agent_field: ThreadReadSubAgentField::Discard,
            spawn_field: ThreadReadSpawnField::Discard,
            sub_agent_seen: false,
            thread_spawn_seen: false,
            nickname_seen: false,
            nickname_is_null: false,
            nickname: FixedScalar::new(),
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ThreadReadSourceState::SourceNameScalar(probe) => {
                probe.push(bytes, &THREAD_READ_SOURCE_FIELDS);
            }
            ThreadReadSourceState::SubAgentNameScalar(probe) => {
                probe.push(bytes, &THREAD_READ_SUB_AGENT_FIELDS);
            }
            ThreadReadSourceState::SpawnNameScalar(probe) => {
                probe.push(bytes, &THREAD_READ_SPAWN_FIELDS);
            }
            ThreadReadSourceState::Nickname => self.nickname.push(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, ThreadReadSourceState::Complete);
        self.state = match state {
            ThreadReadSourceState::Start => self.start(event),
            ThreadReadSourceState::SourceName => self.source_name(event),
            ThreadReadSourceState::SourceNameScalar(probe) => {
                self.finish_source_name(probe, event)
            }
            ThreadReadSourceState::SourceValue => self.start_source_value(event),
            ThreadReadSourceState::SubAgentName => self.sub_agent_name(event),
            ThreadReadSourceState::SubAgentNameScalar(probe) => {
                self.finish_sub_agent_name(probe, event)
            }
            ThreadReadSourceState::SubAgentValue => self.start_sub_agent_value(event),
            ThreadReadSourceState::SpawnName => self.spawn_name(event),
            ThreadReadSourceState::SpawnNameScalar(probe) => {
                self.finish_spawn_name(probe, event)
            }
            ThreadReadSourceState::SpawnValue => self.start_spawn_value(event),
            ThreadReadSourceState::Nickname => self.nickname_event(event),
            ThreadReadSourceState::Discard(mut value, resume) => {
                if !value.event(event) {
                    self.malformed = true;
                    self.remainder(resume.depth(), event)
                } else if value.is_complete() {
                    resume.state()
                } else {
                    ThreadReadSourceState::Discard(value, resume)
                }
            }
            ThreadReadSourceState::Remainder(depth) => self.remainder(depth, event),
            ThreadReadSourceState::Complete => ThreadReadSourceState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> ThreadReadSourceState {
        match event {
            Event::ContainerStart(ContainerKind::Object) => ThreadReadSourceState::SourceName,
            Event::Null => ThreadReadSourceState::Complete,
            _ => self.start_discard(event, ThreadReadSourceResume::Complete),
        }
    }

    fn source_name(&mut self, event: Event) -> ThreadReadSourceState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => ThreadReadSourceState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                ThreadReadSourceState::SourceNameScalar(Self::field_probe())
            }
            _ => self.start_remainder(1, event),
        }
    }

    fn finish_source_name(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> ThreadReadSourceState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => {
                ThreadReadSourceState::SourceNameScalar(probe)
            }
            Event::ScalarEnd(ScalarKind::Name) => {
                self.source_field = if probe.exact(0, THREAD_READ_SOURCE_FIELDS[0].len()) {
                    self.malformed |= self.sub_agent_seen;
                    self.sub_agent_seen = true;
                    ThreadReadSourceField::SubAgent
                } else {
                    ThreadReadSourceField::Discard
                };
                ThreadReadSourceState::SourceValue
            }
            _ => self.start_remainder(1, event),
        }
    }

    fn start_source_value(&mut self, event: Event) -> ThreadReadSourceState {
        match self.source_field {
            ThreadReadSourceField::SubAgent
                if matches!(event, Event::ContainerStart(ContainerKind::Object)) =>
            {
                ThreadReadSourceState::SubAgentName
            }
            ThreadReadSourceField::SubAgent => {
                self.malformed = true;
                self.start_discard(event, ThreadReadSourceResume::SourceName)
            }
            ThreadReadSourceField::Discard => {
                self.start_discard(event, ThreadReadSourceResume::SourceName)
            }
        }
    }

    fn sub_agent_name(&mut self, event: Event) -> ThreadReadSourceState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => ThreadReadSourceState::SourceName,
            Event::ScalarStart(ScalarKind::Name) => {
                ThreadReadSourceState::SubAgentNameScalar(Self::field_probe())
            }
            _ => self.start_remainder(2, event),
        }
    }

    fn finish_sub_agent_name(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> ThreadReadSourceState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => {
                ThreadReadSourceState::SubAgentNameScalar(probe)
            }
            Event::ScalarEnd(ScalarKind::Name) => {
                self.sub_agent_field =
                    if probe.exact(0, THREAD_READ_SUB_AGENT_FIELDS[0].len()) {
                        self.malformed |= self.thread_spawn_seen;
                        self.thread_spawn_seen = true;
                        ThreadReadSubAgentField::ThreadSpawn
                    } else {
                        ThreadReadSubAgentField::Discard
                    };
                ThreadReadSourceState::SubAgentValue
            }
            _ => self.start_remainder(2, event),
        }
    }

    fn start_sub_agent_value(&mut self, event: Event) -> ThreadReadSourceState {
        match self.sub_agent_field {
            ThreadReadSubAgentField::ThreadSpawn
                if matches!(event, Event::ContainerStart(ContainerKind::Object)) =>
            {
                ThreadReadSourceState::SpawnName
            }
            ThreadReadSubAgentField::ThreadSpawn => {
                self.malformed = true;
                self.start_discard(event, ThreadReadSourceResume::SubAgentName)
            }
            ThreadReadSubAgentField::Discard => {
                self.start_discard(event, ThreadReadSourceResume::SubAgentName)
            }
        }
    }

    fn spawn_name(&mut self, event: Event) -> ThreadReadSourceState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => {
                self.malformed |= !self.nickname_seen;
                ThreadReadSourceState::SubAgentName
            }
            Event::ScalarStart(ScalarKind::Name) => {
                ThreadReadSourceState::SpawnNameScalar(Self::field_probe())
            }
            _ => self.start_remainder(3, event),
        }
    }

    fn finish_spawn_name(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> ThreadReadSourceState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => {
                ThreadReadSourceState::SpawnNameScalar(probe)
            }
            Event::ScalarEnd(ScalarKind::Name) => {
                self.spawn_field = if probe.exact(0, THREAD_READ_SPAWN_FIELDS[0].len()) {
                    self.malformed |= self.nickname_seen;
                    self.nickname_seen = true;
                    ThreadReadSpawnField::Nickname
                } else {
                    ThreadReadSpawnField::Discard
                };
                ThreadReadSourceState::SpawnValue
            }
            _ => self.start_remainder(3, event),
        }
    }

    fn start_spawn_value(&mut self, event: Event) -> ThreadReadSourceState {
        match self.spawn_field {
            ThreadReadSpawnField::Nickname => match event {
                Event::ScalarStart(ScalarKind::String) => ThreadReadSourceState::Nickname,
                Event::Null => {
                    self.nickname_is_null = true;
                    ThreadReadSourceState::SpawnName
                }
                _ => {
                    self.malformed = true;
                    self.start_discard(event, ThreadReadSourceResume::SpawnName)
                }
            },
            ThreadReadSpawnField::Discard => {
                self.start_discard(event, ThreadReadSourceResume::SpawnName)
            }
        }
    }

    fn nickname_event(&mut self, event: Event) -> ThreadReadSourceState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => ThreadReadSourceState::Nickname,
            Event::ScalarEnd(ScalarKind::String) => ThreadReadSourceState::SpawnName,
            _ => self.start_remainder(3, event),
        }
    }

    fn start_discard(
        &mut self,
        event: Event,
        resume: ThreadReadSourceResume,
    ) -> ThreadReadSourceState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            self.malformed = true;
            return self.remainder(resume.depth(), event);
        }
        if value.is_complete() {
            resume.state()
        } else {
            ThreadReadSourceState::Discard(value, resume)
        }
    }

    fn start_remainder(&mut self, depth: u16, event: Event) -> ThreadReadSourceState {
        self.malformed = true;
        self.remainder(depth, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> ThreadReadSourceState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            ThreadReadSourceState::Complete
        } else {
            ThreadReadSourceState::Remainder(depth)
        }
    }

    fn field_probe() -> ClassifierProbe {
        let mut probe = ClassifierProbe::new();
        probe.reset(1);
        probe
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, ThreadReadSourceState::Complete)
    }

    fn nickname(&self) -> Option<ThreadReadSourceNickname<'_>> {
        if self.malformed || !self.is_complete() {
            return None;
        }
        if !self.nickname_seen {
            return Some(ThreadReadSourceNickname::Absent);
        }
        if self.nickname_is_null {
            return Some(ThreadReadSourceNickname::Null);
        }
        self.nickname
            .as_str()
            .map(ThreadReadSourceNickname::Value)
    }
}

impl ThreadReadSourceResume {
    const fn state(self) -> ThreadReadSourceState {
        match self {
            Self::SourceName => ThreadReadSourceState::SourceName,
            Self::SubAgentName => ThreadReadSourceState::SubAgentName,
            Self::SpawnName => ThreadReadSourceState::SpawnName,
            Self::Complete => ThreadReadSourceState::Complete,
        }
    }

    const fn depth(self) -> u16 {
        match self {
            Self::SourceName => 1,
            Self::SubAgentName => 2,
            Self::SpawnName => 3,
            Self::Complete => 0,
        }
    }
}

const THREAD_READ_SOURCE_FIELDS: [&[u8]; 1] = [b"subAgent"];
const THREAD_READ_SUB_AGENT_FIELDS: [&[u8]; 1] = [b"thread_spawn"];
const THREAD_READ_SPAWN_FIELDS: [&[u8]; 1] = [b"agent_nickname"];

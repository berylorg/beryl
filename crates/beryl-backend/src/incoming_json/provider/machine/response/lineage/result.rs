#[derive(Clone, Copy)]
enum LineageResultKind {
    Start,
    Resume,
    Fork,
}

impl LineageResultKind {
    const fn target_fields(self) -> &'static [&'static [u8]] {
        match self {
            Self::Start | Self::Fork => &LINEAGE_TARGET_FIELDS,
            Self::Resume => &RESUME_TARGET_FIELDS,
        }
    }
}

struct LineageResultMachine {
    kind: LineageResultKind,
    state: LineageResultState,
    next_target: usize,
    field: LineageResultField,
    thread: LineageThreadMachine,
    model: FixedScalar<256>,
    model_provider: FixedScalar<256>,
    reasoning_effort: FixedScalar<256>,
    reasoning_is_null: bool,
    malformed: bool,
}

enum LineageResultState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    Thread,
    String,
    Discard(ValueTracker),
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

#[derive(Clone, Copy)]
enum LineageResultField {
    Thread,
    Model,
    ModelProvider,
    ReasoningEffort,
    Discard,
}

impl LineageResultMachine {
    const fn new(kind: LineageResultKind) -> Self {
        Self {
            kind,
            state: LineageResultState::Start,
            next_target: 0,
            field: LineageResultField::Discard,
            thread: LineageThreadMachine::new(),
            model: FixedScalar::new(),
            model_provider: FixedScalar::new(),
            reasoning_effort: FixedScalar::new(),
            reasoning_is_null: false,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            LineageResultState::NameScalar(probe) => {
                probe.push(bytes, self.kind.target_fields());
            }
            LineageResultState::Thread => self.thread.scratch_bytes(bytes),
            LineageResultState::String => match self.field {
                LineageResultField::Model => self.model.push(bytes),
                LineageResultField::ModelProvider => self.model_provider.push(bytes),
                LineageResultField::ReasoningEffort => self.reasoning_effort.push(bytes),
                _ => self.malformed = true,
            },
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, LineageResultState::Complete);
        self.state = match state {
            LineageResultState::Start => self.start(event),
            LineageResultState::Name => self.name(event),
            LineageResultState::NameScalar(name) => self.name_scalar(name, event),
            LineageResultState::Value => self.start_value(event),
            LineageResultState::Thread => self.thread_event(event),
            LineageResultState::String => self.string_event(event),
            LineageResultState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    LineageResultState::Name
                } else {
                    LineageResultState::Discard(value)
                }
            }
            LineageResultState::Remainder(depth) => self.remainder(depth, event),
            LineageResultState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    LineageResultState::Complete
                } else {
                    LineageResultState::Fallback(value)
                }
            }
            LineageResultState::Complete => LineageResultState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> LineageResultState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            LineageResultState::Name
        } else {
            self.start_fallback(event)
        }
    }

    fn name(&mut self, event: Event) -> LineageResultState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => LineageResultState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset((1_u16 << self.kind.target_fields().len()) - 1);
                LineageResultState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn name_scalar(&mut self, probe: ClassifierProbe, event: Event) -> LineageResultState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => LineageResultState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                let target = self
                    .kind
                    .target_fields()
                    .iter()
                    .enumerate()
                    .find_map(|(index, wire)| probe.exact(index, wire.len()).then_some(index));
                self.field = match target {
                    Some(index) => {
                        if index != self.next_target {
                            self.malformed = true;
                        } else {
                            self.next_target += 1;
                        }
                        LINEAGE_TARGET_VALUES[index]
                    }
                    None => LineageResultField::Discard,
                };
                LineageResultState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_value(&mut self, event: Event) -> LineageResultState {
        match self.field {
            LineageResultField::Thread => {
                self.thread.event(event);
                if self.thread.is_complete() {
                    LineageResultState::Name
                } else {
                    LineageResultState::Thread
                }
            }
            LineageResultField::Model | LineageResultField::ModelProvider => match event {
                Event::ScalarStart(ScalarKind::String) => LineageResultState::String,
                _ => self.start_remainder(event),
            },
            LineageResultField::ReasoningEffort => match event {
                Event::ScalarStart(ScalarKind::String) => LineageResultState::String,
                Event::Null => {
                    self.reasoning_is_null = true;
                    LineageResultState::Name
                }
                _ => self.start_remainder(event),
            },
            LineageResultField::Discard => {
                let mut value = ValueTracker::new();
                if !value.event(event) {
                    return self.start_remainder(event);
                }
                if value.is_complete() {
                    LineageResultState::Name
                } else {
                    LineageResultState::Discard(value)
                }
            }
        }
    }

    fn thread_event(&mut self, event: Event) -> LineageResultState {
        self.thread.event(event);
        if self.thread.is_complete() {
            LineageResultState::Name
        } else {
            LineageResultState::Thread
        }
    }

    fn string_event(&mut self, event: Event) -> LineageResultState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => LineageResultState::String,
            Event::ScalarEnd(ScalarKind::String) => LineageResultState::Name,
            _ => self.start_remainder(event),
        }
    }

    fn start_fallback(&mut self, event: Event) -> LineageResultState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            LineageResultState::Complete
        } else {
            LineageResultState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> LineageResultState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> LineageResultState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            LineageResultState::Complete
        } else {
            LineageResultState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, LineageResultState::Complete)
    }

    fn take_result(&mut self) -> Option<crate::BoundedResponseResult> {
        if self.malformed
            || !self.is_complete()
            || self.next_target != self.kind.target_fields().len()
        {
            return None;
        }
        let (thread_id, status) = self.thread.take_parts()?;
        let model = self.model.as_str()?;
        let model_provider = self.model_provider.as_str()?;
        let reasoning_effort = if self.reasoning_is_null {
            None
        } else {
            Some(self.reasoning_effort.as_str()?)
        };
        let response = crate::ThreadLineageResponse::try_new(
            thread_id,
            status,
            model,
            model_provider,
            reasoning_effort,
        )?;
        Some(match self.kind {
            LineageResultKind::Start => crate::BoundedResponseResult::ThreadStart(response),
            LineageResultKind::Resume => crate::BoundedResponseResult::ThreadResume(response),
            LineageResultKind::Fork => crate::BoundedResponseResult::ThreadFork(response),
        })
    }
}

const LINEAGE_TARGET_FIELDS: [&[u8]; 4] =
    [b"thread", b"model", b"modelProvider", b"reasoningEffort"];

const RESUME_TARGET_FIELDS: [&[u8]; 7] = [
    b"thread",
    b"model",
    b"modelProvider",
    b"reasoningEffort",
    b"initialTurnsPage",
    b"turnsBackwardsCursor",
    b"itemsBackwardsCursor",
];

const LINEAGE_TARGET_VALUES: [LineageResultField; 7] = [
    LineageResultField::Thread,
    LineageResultField::Model,
    LineageResultField::ModelProvider,
    LineageResultField::ReasoningEffort,
    LineageResultField::Discard,
    LineageResultField::Discard,
    LineageResultField::Discard,
];

struct ThreadReadResultMachine {
    state: ThreadReadResultState,
    field: ThreadReadResultField,
    thread_seen: bool,
    thread: ThreadReadThreadMachine,
    malformed: bool,
}

enum ThreadReadResultState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    Thread,
    Discard(ValueTracker),
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

#[derive(Clone, Copy)]
enum ThreadReadResultField {
    Thread,
    Discard,
}

impl ThreadReadResultMachine {
    const fn new() -> Self {
        Self {
            state: ThreadReadResultState::Start,
            field: ThreadReadResultField::Discard,
            thread_seen: false,
            thread: ThreadReadThreadMachine::new(),
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ThreadReadResultState::NameScalar(probe) => {
                probe.push(bytes, &THREAD_READ_RESULT_FIELDS);
            }
            ThreadReadResultState::Thread => self.thread.scratch_bytes(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, ThreadReadResultState::Complete);
        self.state = match state {
            ThreadReadResultState::Start => self.start(event),
            ThreadReadResultState::Name => self.name(event),
            ThreadReadResultState::NameScalar(probe) => self.finish_name(probe, event),
            ThreadReadResultState::Value => self.start_value(event),
            ThreadReadResultState::Thread => self.thread_event(event),
            ThreadReadResultState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ThreadReadResultState::Name
                } else {
                    ThreadReadResultState::Discard(value)
                }
            }
            ThreadReadResultState::Remainder(depth) => self.remainder(depth, event),
            ThreadReadResultState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ThreadReadResultState::Complete
                } else {
                    ThreadReadResultState::Fallback(value)
                }
            }
            ThreadReadResultState::Complete => ThreadReadResultState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> ThreadReadResultState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            ThreadReadResultState::Name
        } else {
            self.start_fallback(event)
        }
    }

    fn name(&mut self, event: Event) -> ThreadReadResultState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => ThreadReadResultState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset(1);
                ThreadReadResultState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> ThreadReadResultState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => ThreadReadResultState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                self.field = if probe.exact(0, THREAD_READ_RESULT_FIELDS[0].len()) {
                    if self.thread_seen {
                        self.malformed = true;
                        ThreadReadResultField::Discard
                    } else {
                        self.thread_seen = true;
                        ThreadReadResultField::Thread
                    }
                } else {
                    ThreadReadResultField::Discard
                };
                ThreadReadResultState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_value(&mut self, event: Event) -> ThreadReadResultState {
        match self.field {
            ThreadReadResultField::Thread => {
                self.thread.event(event);
                if self.thread.is_complete() {
                    ThreadReadResultState::Name
                } else {
                    ThreadReadResultState::Thread
                }
            }
            ThreadReadResultField::Discard => self.start_discard(event),
        }
    }

    fn thread_event(&mut self, event: Event) -> ThreadReadResultState {
        self.thread.event(event);
        if self.thread.is_complete() {
            ThreadReadResultState::Name
        } else {
            ThreadReadResultState::Thread
        }
    }

    fn start_discard(&mut self, event: Event) -> ThreadReadResultState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return self.start_remainder(event);
        }
        if value.is_complete() {
            ThreadReadResultState::Name
        } else {
            ThreadReadResultState::Discard(value)
        }
    }

    fn start_fallback(&mut self, event: Event) -> ThreadReadResultState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            ThreadReadResultState::Complete
        } else {
            ThreadReadResultState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> ThreadReadResultState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> ThreadReadResultState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            ThreadReadResultState::Complete
        } else {
            ThreadReadResultState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, ThreadReadResultState::Complete)
    }

    fn take_response(&mut self) -> Option<crate::ThreadReadMetadata> {
        if self.malformed || !self.is_complete() || !self.thread_seen {
            return None;
        }
        let (thread_id, status, model_provider, agent_nickname) = self.thread.take_parts()?;
        crate::ThreadReadMetadata::try_new(
            thread_id,
            status,
            model_provider,
            agent_nickname,
        )
    }
}

const THREAD_READ_RESULT_FIELDS: [&[u8]; 1] = [b"thread"];

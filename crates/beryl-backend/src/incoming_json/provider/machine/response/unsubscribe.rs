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
                self.status = UNSUBSCRIBE_STATUS_WIRES
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

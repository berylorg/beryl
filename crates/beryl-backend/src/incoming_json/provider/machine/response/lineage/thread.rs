struct LineageThreadMachine {
    state: LineageThreadState,
    next_target: usize,
    field: LineageThreadField,
    id: FixedScalar<256>,
    status_machine: ThreadStatusMachine,
    status: Option<crate::ThreadStatus>,
    malformed: bool,
}

enum LineageThreadState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    Identity,
    Status,
    Discard(ValueTracker),
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

#[derive(Clone, Copy)]
enum LineageThreadField {
    Identity,
    Status,
    Discard,
}

impl LineageThreadMachine {
    const fn new() -> Self {
        Self {
            state: LineageThreadState::Start,
            next_target: 0,
            field: LineageThreadField::Discard,
            id: FixedScalar::new(),
            status_machine: ThreadStatusMachine::new(),
            status: None,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            LineageThreadState::NameScalar(probe) => {
                probe.push(bytes, &LINEAGE_THREAD_TARGET_FIELDS);
            }
            LineageThreadState::Identity => self.id.push(bytes),
            LineageThreadState::Status => self.status_machine.scratch_bytes(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, LineageThreadState::Complete);
        self.state = match state {
            LineageThreadState::Start => self.start(event),
            LineageThreadState::Name => self.name(event),
            LineageThreadState::NameScalar(name) => self.name_scalar(name, event),
            LineageThreadState::Value => self.start_value(event),
            LineageThreadState::Identity => self.identity_event(event),
            LineageThreadState::Status => self.status_event(event),
            LineageThreadState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    LineageThreadState::Name
                } else {
                    LineageThreadState::Discard(value)
                }
            }
            LineageThreadState::Remainder(depth) => self.remainder(depth, event),
            LineageThreadState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    LineageThreadState::Complete
                } else {
                    LineageThreadState::Fallback(value)
                }
            }
            LineageThreadState::Complete => LineageThreadState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> LineageThreadState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            LineageThreadState::Name
        } else {
            self.start_fallback(event)
        }
    }

    fn name(&mut self, event: Event) -> LineageThreadState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => LineageThreadState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset((1_u16 << LINEAGE_THREAD_TARGET_FIELDS.len()) - 1);
                LineageThreadState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn name_scalar(&mut self, probe: ClassifierProbe, event: Event) -> LineageThreadState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => LineageThreadState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                let target = LINEAGE_THREAD_TARGET_FIELDS
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
                        LINEAGE_THREAD_TARGET_VALUES[index]
                    }
                    None => LineageThreadField::Discard,
                };
                LineageThreadState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_value(&mut self, event: Event) -> LineageThreadState {
        match self.field {
            LineageThreadField::Identity => match event {
                Event::ScalarStart(ScalarKind::String) => LineageThreadState::Identity,
                _ => self.start_remainder(event),
            },
            LineageThreadField::Status => {
                self.status_machine.event(event);
                if self.status_machine.is_complete() {
                    self.finish_status()
                } else {
                    LineageThreadState::Status
                }
            }
            LineageThreadField::Discard => {
                let mut value = ValueTracker::new();
                if !value.event(event) {
                    return self.start_remainder(event);
                }
                if value.is_complete() {
                    LineageThreadState::Name
                } else {
                    LineageThreadState::Discard(value)
                }
            }
        }
    }

    fn identity_event(&mut self, event: Event) -> LineageThreadState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => LineageThreadState::Identity,
            Event::ScalarEnd(ScalarKind::String) => LineageThreadState::Name,
            _ => self.start_remainder(event),
        }
    }

    fn status_event(&mut self, event: Event) -> LineageThreadState {
        self.status_machine.event(event);
        if self.status_machine.is_complete() {
            self.finish_status()
        } else {
            LineageThreadState::Status
        }
    }

    fn finish_status(&mut self) -> LineageThreadState {
        self.status = self.status_machine.take_status();
        self.malformed |= self.status.is_none();
        LineageThreadState::Name
    }

    fn start_fallback(&mut self, event: Event) -> LineageThreadState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            LineageThreadState::Complete
        } else {
            LineageThreadState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> LineageThreadState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> LineageThreadState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            LineageThreadState::Complete
        } else {
            LineageThreadState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, LineageThreadState::Complete)
    }

    fn take_parts(&mut self) -> Option<(&str, crate::ThreadStatus)> {
        if self.malformed
            || !self.is_complete()
            || self.next_target != LINEAGE_THREAD_TARGET_FIELDS.len()
        {
            return None;
        }
        let status = self.status.take()?;
        let id = self.id.as_str()?;
        Some((id, status))
    }
}

const LINEAGE_THREAD_TARGET_FIELDS: [&[u8]; 2] = [
    b"id",
    b"status",
];

const LINEAGE_THREAD_TARGET_VALUES: [LineageThreadField; 2] =
    [LineageThreadField::Identity, LineageThreadField::Status];

struct OriginsMachine {
    state: OriginsState,
    next_target: usize,
    field: Option<usize>,
    metadata: OriginMetadataMachine,
    proven: [bool; 2],
    malformed: bool,
}

enum OriginsState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    Metadata,
    Discard(ValueTracker),
    Complete,
}

impl OriginsMachine {
    const fn new() -> Self {
        Self {
            state: OriginsState::Start,
            next_target: 0,
            field: None,
            metadata: OriginMetadataMachine::new(),
            proven: [false; 2],
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            OriginsState::NameScalar(probe) => {
                probe.push(bytes, &ORIGIN_TARGET_FIELDS);
            }
            OriginsState::Metadata => self.metadata.scratch_bytes(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, OriginsState::Complete);
        self.state = match state {
            OriginsState::Start => match event {
                Event::ContainerStart(ContainerKind::Object) => OriginsState::Name,
                _ => self.invalid(event),
            },
            OriginsState::Name => match event {
                Event::ContainerEnd(ContainerKind::Object) => OriginsState::Complete,
                Event::ScalarStart(ScalarKind::Name) => {
                    let mut probe = ClassifierProbe::new();
                    probe.reset(3);
                    OriginsState::NameScalar(probe)
                }
                _ => self.invalid(event),
            },
            OriginsState::NameScalar(probe) => match event {
                Event::ScalarFragment(ScalarKind::Name) => OriginsState::NameScalar(probe),
                Event::ScalarEnd(ScalarKind::Name) => {
                    self.field = ORIGIN_TARGET_FIELDS
                        .iter()
                        .enumerate()
                        .find_map(|(index, name)| probe.exact(index, name.len()).then_some(index));
                    if let Some(index) = self.field {
                        if index != self.next_target {
                            self.malformed = true;
                        } else {
                            self.next_target += 1;
                        }
                    }
                    OriginsState::Value
                }
                _ => self.invalid(event),
            },
            OriginsState::Value => match self.field {
                Some(_) => {
                    self.metadata = OriginMetadataMachine::new();
                    self.metadata.event(event);
                    if self.metadata.is_complete() {
                        self.finish_metadata()
                    } else {
                        OriginsState::Metadata
                    }
                }
                None => self.start_discard(event),
            },
            OriginsState::Metadata => {
                self.metadata.event(event);
                if self.metadata.is_complete() {
                    self.finish_metadata()
                } else {
                    OriginsState::Metadata
                }
            }
            OriginsState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    OriginsState::Name
                } else {
                    OriginsState::Discard(value)
                }
            }
            OriginsState::Complete => OriginsState::Complete,
        };
    }

    fn finish_metadata(&mut self) -> OriginsState {
        let index = self.field.take().expect("target origin retains its index");
        self.proven[index] = self.metadata.is_proven();
        OriginsState::Name
    }

    fn start_discard(&mut self, event: Event) -> OriginsState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            self.malformed = true;
            OriginsState::Complete
        } else if value.is_complete() {
            OriginsState::Name
        } else {
            OriginsState::Discard(value)
        }
    }

    fn invalid(&mut self, event: Event) -> OriginsState {
        self.malformed = true;
        self.start_discard(event)
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, OriginsState::Complete)
    }

    fn required_settings_proven(&self) -> bool {
        !self.malformed && self.is_complete() && self.next_target == 2 && self.proven == [true; 2]
    }
}

const ORIGIN_TARGET_FIELDS: [&[u8]; 2] = [
    b"features.multi_agent_v2.enabled",
    b"features.multi_agent_v2.expose_spawn_agent_model_overrides",
];

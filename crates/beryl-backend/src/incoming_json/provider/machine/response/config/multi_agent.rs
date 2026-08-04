struct MultiAgentSettingsMachine {
    state: MultiAgentSettingsState,
    field: Option<usize>,
    next_target: usize,
    proven: [bool; 2],
    malformed: bool,
}

enum MultiAgentSettingsState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    Discard(ValueTracker),
    Complete,
}

impl MultiAgentSettingsMachine {
    const fn new() -> Self {
        Self {
            state: MultiAgentSettingsState::Start,
            field: None,
            next_target: 0,
            proven: [false; 2],
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        if let MultiAgentSettingsState::NameScalar(probe) = &mut self.state {
            probe.push(bytes, &MULTI_AGENT_TARGET_FIELDS);
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, MultiAgentSettingsState::Complete);
        self.state = match state {
            MultiAgentSettingsState::Start => match event {
                Event::ContainerStart(ContainerKind::Object) => MultiAgentSettingsState::Name,
                _ => self.invalid(event),
            },
            MultiAgentSettingsState::Name => match event {
                Event::ContainerEnd(ContainerKind::Object) => MultiAgentSettingsState::Complete,
                Event::ScalarStart(ScalarKind::Name) => {
                    let mut probe = ClassifierProbe::new();
                    probe.reset(3);
                    MultiAgentSettingsState::NameScalar(probe)
                }
                _ => self.invalid(event),
            },
            MultiAgentSettingsState::NameScalar(probe) => match event {
                Event::ScalarFragment(ScalarKind::Name) => {
                    MultiAgentSettingsState::NameScalar(probe)
                }
                Event::ScalarEnd(ScalarKind::Name) => {
                    self.field = MULTI_AGENT_TARGET_FIELDS
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
                    MultiAgentSettingsState::Value
                }
                _ => self.invalid(event),
            },
            MultiAgentSettingsState::Value => match self.field.take() {
                Some(index) => {
                    self.proven[index] = matches!(event, Event::Boolean(true));
                    if !self.proven[index] {
                        self.malformed = true;
                        self.start_discard(event)
                    } else {
                        MultiAgentSettingsState::Name
                    }
                }
                None => self.start_discard(event),
            },
            MultiAgentSettingsState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    MultiAgentSettingsState::Name
                } else {
                    MultiAgentSettingsState::Discard(value)
                }
            }
            MultiAgentSettingsState::Complete => MultiAgentSettingsState::Complete,
        };
    }

    fn start_discard(&mut self, event: Event) -> MultiAgentSettingsState {
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            MultiAgentSettingsState::Name
        } else {
            MultiAgentSettingsState::Discard(value)
        }
    }

    fn invalid(&mut self, event: Event) -> MultiAgentSettingsState {
        self.malformed = true;
        self.start_discard(event)
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, MultiAgentSettingsState::Complete)
    }

    fn proven(&self) -> bool {
        self.is_complete() && !self.malformed && self.next_target == 2 && self.proven == [true; 2]
    }
}

const MULTI_AGENT_TARGET_FIELDS: [&[u8]; 2] = [b"enabled", b"expose_spawn_agent_model_overrides"];

struct ConfigObjectMachine {
    state: ConfigObjectState,
    seen: u8,
    field: ConfigField,
    model: FixedScalar<256>,
    reasoning: FixedScalar<256>,
    features: RequiredFeatureSettingsMachine,
    model_is_null: bool,
    reasoning_is_null: bool,
    malformed: bool,
}

enum ConfigObjectState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    String,
    Features,
    Discard(ValueTracker),
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

#[derive(Clone, Copy)]
enum ConfigField {
    Model,
    Reasoning,
    Features,
    Unknown,
}

impl ConfigObjectMachine {
    const fn new() -> Self {
        Self {
            state: ConfigObjectState::Start,
            seen: 0,
            field: ConfigField::Unknown,
            model: FixedScalar::new(),
            reasoning: FixedScalar::new(),
            features: RequiredFeatureSettingsMachine::new(),
            model_is_null: false,
            reasoning_is_null: false,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ConfigObjectState::NameScalar(probe) => {
                probe.push(bytes, &CONFIG_TARGET_FIELDS);
            }
            ConfigObjectState::String => match self.field {
                ConfigField::Model => self.model.push(bytes),
                ConfigField::Reasoning => self.reasoning.push(bytes),
                ConfigField::Features | ConfigField::Unknown => self.malformed = true,
            },
            ConfigObjectState::Features => self.features.scratch_bytes(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, ConfigObjectState::Complete);
        self.state = match state {
            ConfigObjectState::Start => self.start(event),
            ConfigObjectState::Name => self.name(event),
            ConfigObjectState::NameScalar(probe) => self.finish_name(probe, event),
            ConfigObjectState::Value => self.value(event),
            ConfigObjectState::String => match event {
                Event::ScalarFragment(ScalarKind::String) => ConfigObjectState::String,
                Event::ScalarEnd(ScalarKind::String) => ConfigObjectState::Name,
                _ => self.start_remainder(event),
            },
            ConfigObjectState::Features => {
                self.features.event(event);
                if self.features.is_complete() {
                    ConfigObjectState::Name
                } else {
                    ConfigObjectState::Features
                }
            }
            ConfigObjectState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ConfigObjectState::Name
                } else {
                    ConfigObjectState::Discard(value)
                }
            }
            ConfigObjectState::Remainder(depth) => self.remainder(depth, event),
            ConfigObjectState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ConfigObjectState::Complete
                } else {
                    ConfigObjectState::Fallback(value)
                }
            }
            ConfigObjectState::Complete => ConfigObjectState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> ConfigObjectState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            return ConfigObjectState::Name;
        }
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            ConfigObjectState::Complete
        } else {
            ConfigObjectState::Fallback(value)
        }
    }

    fn name(&mut self, event: Event) -> ConfigObjectState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => ConfigObjectState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset(7);
                ConfigObjectState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(&mut self, probe: ClassifierProbe, event: Event) -> ConfigObjectState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => ConfigObjectState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                self.field = if probe.exact(0, CONFIG_TARGET_FIELDS[0].len()) {
                    if self.seen != 0 {
                        self.malformed = true;
                    }
                    self.seen |= 1;
                    ConfigField::Model
                } else if probe.exact(1, CONFIG_TARGET_FIELDS[1].len()) {
                    if self.seen != 1 {
                        self.malformed = true;
                    }
                    self.seen |= 2;
                    ConfigField::Reasoning
                } else if probe.exact(2, CONFIG_TARGET_FIELDS[2].len()) {
                    if self.seen != 3 {
                        self.malformed = true;
                    }
                    self.seen |= 4;
                    ConfigField::Features
                } else {
                    ConfigField::Unknown
                };
                ConfigObjectState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn value(&mut self, event: Event) -> ConfigObjectState {
        match self.field {
            ConfigField::Model | ConfigField::Reasoning => match event {
                Event::Null => {
                    match self.field {
                        ConfigField::Model => self.model_is_null = true,
                        ConfigField::Reasoning => self.reasoning_is_null = true,
                        ConfigField::Features | ConfigField::Unknown => unreachable!(),
                    }
                    ConfigObjectState::Name
                }
                Event::ScalarStart(ScalarKind::String) => ConfigObjectState::String,
                _ => {
                    self.malformed = true;
                    self.start_discard(event)
                }
            },
            ConfigField::Features => {
                self.features = RequiredFeatureSettingsMachine::new();
                self.features.event(event);
                if self.features.is_complete() {
                    ConfigObjectState::Name
                } else {
                    ConfigObjectState::Features
                }
            }
            ConfigField::Unknown => self.start_discard(event),
        }
    }

    fn start_discard(&mut self, event: Event) -> ConfigObjectState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return self.start_remainder(event);
        }
        if value.is_complete() {
            ConfigObjectState::Name
        } else {
            ConfigObjectState::Discard(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> ConfigObjectState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> ConfigObjectState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            ConfigObjectState::Complete
        } else {
            ConfigObjectState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, ConfigObjectState::Complete)
    }

    fn defaults(
        &self,
        multi_agent_v2_enabled: bool,
        expose_spawn_agent_model_overrides: bool,
    ) -> Option<crate::BackendConfigDefaults> {
        if self.malformed || !self.is_complete() || self.seen != 7 || !self.features.proven() {
            return None;
        }
        let model = if self.model_is_null {
            None
        } else {
            Some(crate::ProtocolIdentity::try_new(self.model.as_str()?).ok()?)
        };
        let reasoning = if self.reasoning_is_null {
            None
        } else {
            Some(crate::ProtocolIdentity::try_new(self.reasoning.as_str()?).ok()?)
        };
        Some(crate::BackendConfigDefaults::new(
            model,
            reasoning,
            multi_agent_v2_enabled,
            expose_spawn_agent_model_overrides,
        ))
    }
}

const CONFIG_TARGET_FIELDS: [&[u8]; 3] = [b"model", b"model_reasoning_effort", b"features"];

struct RequiredFeatureSettingsMachine {
    state: RequiredFeatureSettingsState,
    multi_agent: MultiAgentSettingsMachine,
    target: bool,
    seen: bool,
    malformed: bool,
}

enum RequiredFeatureSettingsState {
    Start,
    Name,
    NameScalar(ClassifierProbe),
    Value,
    MultiAgent,
    Discard(ValueTracker),
    Complete,
}

impl RequiredFeatureSettingsMachine {
    const fn new() -> Self {
        Self {
            state: RequiredFeatureSettingsState::Start,
            multi_agent: MultiAgentSettingsMachine::new(),
            target: false,
            seen: false,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            RequiredFeatureSettingsState::NameScalar(probe) => {
                probe.push(bytes, &[b"multi_agent_v2"]);
            }
            RequiredFeatureSettingsState::MultiAgent => {
                self.multi_agent.scratch_bytes(bytes);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, RequiredFeatureSettingsState::Complete);
        self.state = match state {
            RequiredFeatureSettingsState::Start => match event {
                Event::ContainerStart(ContainerKind::Object) => RequiredFeatureSettingsState::Name,
                _ => self.invalid(event),
            },
            RequiredFeatureSettingsState::Name => match event {
                Event::ContainerEnd(ContainerKind::Object) => {
                    RequiredFeatureSettingsState::Complete
                }
                Event::ScalarStart(ScalarKind::Name) => {
                    let mut probe = ClassifierProbe::new();
                    probe.reset(1);
                    RequiredFeatureSettingsState::NameScalar(probe)
                }
                _ => self.invalid(event),
            },
            RequiredFeatureSettingsState::NameScalar(probe) => match event {
                Event::ScalarFragment(ScalarKind::Name) => {
                    RequiredFeatureSettingsState::NameScalar(probe)
                }
                Event::ScalarEnd(ScalarKind::Name) => {
                    self.target = probe.exact(0, b"multi_agent_v2".len());
                    if self.target && self.seen {
                        self.malformed = true;
                    }
                    RequiredFeatureSettingsState::Value
                }
                _ => self.invalid(event),
            },
            RequiredFeatureSettingsState::Value if self.target => {
                self.seen = true;
                self.multi_agent = MultiAgentSettingsMachine::new();
                self.multi_agent.event(event);
                if self.multi_agent.is_complete() {
                    RequiredFeatureSettingsState::Name
                } else {
                    RequiredFeatureSettingsState::MultiAgent
                }
            }
            RequiredFeatureSettingsState::Value => self.start_discard(event),
            RequiredFeatureSettingsState::MultiAgent => {
                self.multi_agent.event(event);
                if self.multi_agent.is_complete() {
                    RequiredFeatureSettingsState::Name
                } else {
                    RequiredFeatureSettingsState::MultiAgent
                }
            }
            RequiredFeatureSettingsState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    RequiredFeatureSettingsState::Name
                } else {
                    RequiredFeatureSettingsState::Discard(value)
                }
            }
            RequiredFeatureSettingsState::Complete => RequiredFeatureSettingsState::Complete,
        };
    }

    fn start_discard(&mut self, event: Event) -> RequiredFeatureSettingsState {
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            RequiredFeatureSettingsState::Name
        } else {
            RequiredFeatureSettingsState::Discard(value)
        }
    }

    fn invalid(&mut self, event: Event) -> RequiredFeatureSettingsState {
        self.malformed = true;
        self.start_discard(event)
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, RequiredFeatureSettingsState::Complete)
    }

    fn proven(&self) -> bool {
        self.is_complete() && !self.malformed && self.seen && self.multi_agent.proven()
    }
}

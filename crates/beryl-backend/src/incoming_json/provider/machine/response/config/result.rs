struct ConfigResultMachine {
    state: ConfigResultState,
    config: ConfigObjectMachine,
    origins: OriginsMachine,
    defaults: Option<crate::BackendConfigDefaults>,
    malformed: bool,
}

enum ConfigResultState {
    Start,
    ConfigNameStart,
    ConfigName(ExactName),
    Config,
    OriginsNameStart,
    OriginsName(ExactName),
    OriginsValue,
    Origins,
    AfterOrigins,
    LayersName(ExactName),
    LayersValue,
    LayersDiscard(ValueTracker),
    AfterLayers,
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

impl ConfigResultMachine {
    const fn new() -> Self {
        Self {
            state: ConfigResultState::Start,
            config: ConfigObjectMachine::new(),
            origins: OriginsMachine::new(),
            defaults: None,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ConfigResultState::ConfigName(name)
            | ConfigResultState::OriginsName(name)
            | ConfigResultState::LayersName(name) => name.push(bytes),
            ConfigResultState::Config => self.config.scratch_bytes(bytes),
            ConfigResultState::OriginsValue | ConfigResultState::Origins => {
                self.origins.scratch_bytes(bytes);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, ConfigResultState::Complete);
        self.state = match state {
            ConfigResultState::Start => {
                if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
                    ConfigResultState::ConfigNameStart
                } else {
                    self.start_fallback(event)
                }
            }
            ConfigResultState::ConfigNameStart => self.start_name(event, b"config", true),
            ConfigResultState::ConfigName(name) => {
                self.finish_name(name, event, ConfigResultState::Config)
            }
            ConfigResultState::Config => {
                self.config.event(event);
                if self.config.is_complete() {
                    ConfigResultState::OriginsNameStart
                } else {
                    ConfigResultState::Config
                }
            }
            ConfigResultState::OriginsNameStart => self.start_name(event, b"origins", false),
            ConfigResultState::OriginsName(name) => {
                self.finish_name(name, event, ConfigResultState::OriginsValue)
            }
            ConfigResultState::OriginsValue => {
                self.origins.event(event);
                if self.origins.is_complete() {
                    self.finish_origins()
                } else {
                    ConfigResultState::Origins
                }
            }
            ConfigResultState::Origins => {
                self.origins.event(event);
                if self.origins.is_complete() {
                    self.finish_origins()
                } else {
                    ConfigResultState::Origins
                }
            }
            ConfigResultState::AfterOrigins => match event {
                Event::ContainerEnd(ContainerKind::Object) => ConfigResultState::Complete,
                Event::ScalarStart(ScalarKind::Name) => {
                    ConfigResultState::LayersName(ExactName::new(b"layers"))
                }
                _ => self.start_remainder(event),
            },
            ConfigResultState::LayersName(name) => {
                self.finish_name(name, event, ConfigResultState::LayersValue)
            }
            ConfigResultState::LayersValue => {
                self.start_incidental(event, ContainerKind::Array, true)
            }
            ConfigResultState::LayersDiscard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ConfigResultState::AfterLayers
                } else {
                    ConfigResultState::LayersDiscard(value)
                }
            }
            ConfigResultState::AfterLayers => {
                if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
                    ConfigResultState::Complete
                } else {
                    self.start_remainder(event)
                }
            }
            ConfigResultState::Remainder(depth) => self.remainder(depth, event),
            ConfigResultState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ConfigResultState::Complete
                } else {
                    ConfigResultState::Fallback(value)
                }
            }
            ConfigResultState::Complete => ConfigResultState::Complete,
        };
    }

    fn start_name(
        &mut self,
        event: Event,
        expected: &'static [u8],
        is_config: bool,
    ) -> ConfigResultState {
        match event {
            Event::ScalarStart(ScalarKind::Name) if is_config => {
                ConfigResultState::ConfigName(ExactName::new(expected))
            }
            Event::ScalarStart(ScalarKind::Name) => {
                ConfigResultState::OriginsName(ExactName::new(expected))
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(
        &mut self,
        name: ExactName,
        event: Event,
        next: ConfigResultState,
    ) -> ConfigResultState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => match next {
                ConfigResultState::Config => ConfigResultState::ConfigName(name),
                ConfigResultState::OriginsValue => ConfigResultState::OriginsName(name),
                ConfigResultState::LayersValue => ConfigResultState::LayersName(name),
                _ => unreachable!("config result name has a known successor"),
            },
            Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => next,
            _ => self.start_remainder(event),
        }
    }

    fn start_incidental(
        &mut self,
        event: Event,
        expected: ContainerKind,
        is_layers: bool,
    ) -> ConfigResultState {
        self.malformed |= !matches!(event, Event::ContainerStart(kind) if kind == expected);
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return self.start_remainder(event);
        }
        if value.is_complete() {
            if is_layers {
                ConfigResultState::AfterLayers
            } else {
                ConfigResultState::AfterOrigins
            }
        } else if is_layers {
            ConfigResultState::LayersDiscard(value)
        } else {
            unreachable!("origins uses its dedicated bounded machine")
        }
    }

    fn finish_origins(&mut self) -> ConfigResultState {
        let proven = self.origins.required_settings_proven();
        self.defaults = self.config.defaults(proven, proven);
        self.malformed |= self.defaults.is_none() || !proven;
        ConfigResultState::AfterOrigins
    }

    fn start_fallback(&mut self, event: Event) -> ConfigResultState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            ConfigResultState::Complete
        } else {
            ConfigResultState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> ConfigResultState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> ConfigResultState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            ConfigResultState::Complete
        } else {
            ConfigResultState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, ConfigResultState::Complete)
    }

    fn take_response(&mut self) -> Option<crate::ConfigReadResponse> {
        if self.malformed || !self.is_complete() {
            return None;
        }
        self.defaults.take().map(crate::ConfigReadResponse::new)
    }
}

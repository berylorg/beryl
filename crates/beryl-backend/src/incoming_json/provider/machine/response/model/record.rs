struct ModelRecordMachine {
    state: ModelRecordState,
    next_target: usize,
    field: ModelTargetField,
    id: FixedScalar<256>,
    model: FixedScalar<256>,
    display_name: FixedScalar<1024>,
    hidden: bool,
    is_default: bool,
    supported: Option<crate::SupportedReasoningEfforts>,
    default_effort: Option<crate::DefaultReasoningEffort>,
    malformed: bool,
}

enum ModelRecordState {
    Name,
    NameScalar(ClassifierProbe),
    Value,
    String,
    Supported(SupportedEffortsMachine),
    DefaultEffort(ClassifierProbe),
    Discard(ValueTracker),
    Remainder(u16),
    Complete,
}

#[derive(Clone, Copy)]
enum ModelTargetField {
    Id,
    Model,
    DisplayName,
    Hidden,
    SupportedEfforts,
    DefaultEffort,
    IsDefault,
    Unknown,
}

impl ModelRecordMachine {
    const fn started() -> Self {
        Self {
            state: ModelRecordState::Name,
            next_target: 0,
            field: ModelTargetField::Unknown,
            id: FixedScalar::new(),
            model: FixedScalar::new(),
            display_name: FixedScalar::new(),
            hidden: false,
            is_default: false,
            supported: None,
            default_effort: None,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ModelRecordState::NameScalar(probe) => {
                probe.push(bytes, &MODEL_TARGET_FIELDS);
            }
            ModelRecordState::String => match self.field {
                ModelTargetField::Id => self.id.push(bytes),
                ModelTargetField::Model => self.model.push(bytes),
                ModelTargetField::DisplayName => self.display_name.push(bytes),
                _ => self.malformed = true,
            },
            ModelRecordState::Supported(machine) => machine.scratch_bytes(bytes),
            ModelRecordState::DefaultEffort(probe) => {
                probe.push(bytes, &REASONING_EFFORT_WIRES);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, ModelRecordState::Complete);
        self.state = match state {
            ModelRecordState::Name => self.name(event),
            ModelRecordState::NameScalar(probe) => self.finish_name(probe, event),
            ModelRecordState::Value => self.value(event),
            ModelRecordState::String => match event {
                Event::ScalarFragment(ScalarKind::String) => ModelRecordState::String,
                Event::ScalarEnd(ScalarKind::String) => ModelRecordState::Name,
                _ => self.start_remainder(event),
            },
            ModelRecordState::Supported(mut machine) => {
                machine.event(event);
                if machine.is_complete() {
                    self.supported = machine.take_efforts();
                    self.malformed |= self.supported.is_none();
                    ModelRecordState::Name
                } else {
                    ModelRecordState::Supported(machine)
                }
            }
            ModelRecordState::DefaultEffort(probe) => self.default_effort(probe, event),
            ModelRecordState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ModelRecordState::Name
                } else {
                    ModelRecordState::Discard(value)
                }
            }
            ModelRecordState::Remainder(depth) => self.remainder(depth, event),
            ModelRecordState::Complete => ModelRecordState::Complete,
        };
    }

    fn name(&mut self, event: Event) -> ModelRecordState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => ModelRecordState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset((1_u16 << MODEL_TARGET_FIELDS.len()) - 1);
                ModelRecordState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(&mut self, probe: ClassifierProbe, event: Event) -> ModelRecordState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => ModelRecordState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                let target = MODEL_TARGET_FIELDS
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
                        MODEL_TARGET_VALUES[index]
                    }
                    None => ModelTargetField::Unknown,
                };
                ModelRecordState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn value(&mut self, event: Event) -> ModelRecordState {
        match self.field {
            ModelTargetField::Id
            | ModelTargetField::Model
            | ModelTargetField::DisplayName => match event {
                Event::ScalarStart(ScalarKind::String) => ModelRecordState::String,
                _ => self.invalid_value(event),
            },
            ModelTargetField::Hidden => match event {
                Event::Boolean(value) => {
                    self.hidden = value;
                    ModelRecordState::Name
                }
                _ => self.invalid_value(event),
            },
            ModelTargetField::IsDefault => match event {
                Event::Boolean(value) => {
                    self.is_default = value;
                    ModelRecordState::Name
                }
                _ => self.invalid_value(event),
            },
            ModelTargetField::SupportedEfforts => {
                let mut machine = SupportedEffortsMachine::new();
                machine.event(event);
                if machine.is_complete() {
                    self.supported = machine.take_efforts();
                    self.malformed |= self.supported.is_none();
                    ModelRecordState::Name
                } else {
                    ModelRecordState::Supported(machine)
                }
            }
            ModelTargetField::DefaultEffort => match event {
                Event::ScalarStart(ScalarKind::String) => {
                    ModelRecordState::DefaultEffort(new_effort_probe())
                }
                _ => self.invalid_value(event),
            },
            ModelTargetField::Unknown => self.start_discard(event),
        }
    }

    fn default_effort(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> ModelRecordState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => ModelRecordState::DefaultEffort(probe),
            Event::ScalarEnd(ScalarKind::String) => {
                if probe.length == 0 {
                    self.malformed = true;
                } else {
                    self.default_effort = Some(default_effort_from_probe(probe));
                }
                ModelRecordState::Name
            }
            _ => self.start_remainder(event),
        }
    }

    fn invalid_value(&mut self, event: Event) -> ModelRecordState {
        self.malformed = true;
        self.start_discard(event)
    }

    fn start_discard(&mut self, event: Event) -> ModelRecordState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return self.start_remainder(event);
        }
        if value.is_complete() {
            ModelRecordState::Name
        } else {
            ModelRecordState::Discard(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> ModelRecordState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> ModelRecordState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            ModelRecordState::Complete
        } else {
            ModelRecordState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, ModelRecordState::Complete)
    }

    fn take_record(&mut self) -> Option<crate::ModelRecord> {
        if self.malformed
            || !self.is_complete()
            || self.next_target != MODEL_TARGET_FIELDS.len()
        {
            return None;
        }
        Some(crate::ModelRecord::new(
            crate::ProtocolIdentity::try_new(self.id.as_str()?).ok()?,
            crate::ProtocolIdentity::try_new(self.model.as_str()?).ok()?,
            crate::ModelDisplayName::try_new(self.display_name.as_str()?).ok()?,
            self.hidden,
            self.is_default,
            self.supported.take()?,
            self.default_effort?,
        ))
    }
}

fn default_effort_from_probe(probe: ClassifierProbe) -> crate::DefaultReasoningEffort {
    match effort_from_probe(probe) {
        Some(crate::ReasoningEffort::None) => crate::DefaultReasoningEffort::None,
        Some(crate::ReasoningEffort::Minimal) => crate::DefaultReasoningEffort::Minimal,
        Some(crate::ReasoningEffort::Low) => crate::DefaultReasoningEffort::Low,
        Some(crate::ReasoningEffort::Medium) => crate::DefaultReasoningEffort::Medium,
        Some(crate::ReasoningEffort::High) => crate::DefaultReasoningEffort::High,
        Some(crate::ReasoningEffort::XHigh) => crate::DefaultReasoningEffort::XHigh,
        Some(crate::ReasoningEffort::Max) => crate::DefaultReasoningEffort::Max,
        Some(crate::ReasoningEffort::Ultra) => crate::DefaultReasoningEffort::Ultra,
        None => crate::DefaultReasoningEffort::Other,
    }
}

const MODEL_TARGET_FIELDS: [&[u8]; 7] = [
    b"id",
    b"model",
    b"displayName",
    b"hidden",
    b"supportedReasoningEfforts",
    b"defaultReasoningEffort",
    b"isDefault",
];

const MODEL_TARGET_VALUES: [ModelTargetField; 7] = [
    ModelTargetField::Id,
    ModelTargetField::Model,
    ModelTargetField::DisplayName,
    ModelTargetField::Hidden,
    ModelTargetField::SupportedEfforts,
    ModelTargetField::DefaultEffort,
    ModelTargetField::IsDefault,
];

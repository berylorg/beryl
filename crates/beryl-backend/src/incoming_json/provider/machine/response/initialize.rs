struct InitializeResultMachine {
    state: InitializeState,
    field: usize,
    product: UserAgentProduct,
    platform_family: FixedScalar<16>,
    platform_os: FixedScalar<16>,
    malformed: bool,
}

enum InitializeState {
    Start,
    Name,
    NameScalar(ExactName),
    Value,
    String,
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

impl InitializeResultMachine {
    const fn new() -> Self {
        Self {
            state: InitializeState::Start,
            field: 0,
            product: UserAgentProduct::new(),
            platform_family: FixedScalar::new(),
            platform_os: FixedScalar::new(),
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            InitializeState::NameScalar(name) => name.push(bytes),
            InitializeState::String => match self.field {
                0 => self.product.push(bytes),
                1 => {}
                2 => self.platform_family.push(bytes),
                3 => self.platform_os.push(bytes),
                _ => self.malformed = true,
            },
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, InitializeState::Complete);
        self.state = match state {
            InitializeState::Start => self.start(event),
            InitializeState::Name => self.name(event),
            InitializeState::NameScalar(name) => self.name_scalar(name, event),
            InitializeState::Value => match event {
                Event::ScalarStart(ScalarKind::String) => InitializeState::String,
                _ => self.start_remainder(event),
            },
            InitializeState::String => match event {
                Event::ScalarFragment(ScalarKind::String) => InitializeState::String,
                Event::ScalarEnd(ScalarKind::String) => {
                    self.field += 1;
                    InitializeState::Name
                }
                _ => self.start_remainder(event),
            },
            InitializeState::Remainder(depth) => self.remainder(depth, event),
            InitializeState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    InitializeState::Complete
                } else {
                    InitializeState::Fallback(value)
                }
            }
            InitializeState::Complete => InitializeState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> InitializeState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            return InitializeState::Name;
        }
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            InitializeState::Complete
        } else {
            InitializeState::Fallback(value)
        }
    }

    fn name(&mut self, event: Event) -> InitializeState {
        if self.field == INITIALIZE_FIELDS.len() {
            return if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
                InitializeState::Complete
            } else {
                self.start_remainder(event)
            };
        }
        match event {
            Event::ScalarStart(ScalarKind::Name) => {
                InitializeState::NameScalar(ExactName::new(INITIALIZE_FIELDS[self.field]))
            }
            _ => self.start_remainder(event),
        }
    }

    fn name_scalar(&mut self, name: ExactName, event: Event) -> InitializeState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => InitializeState::NameScalar(name),
            Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => InitializeState::Value,
            _ => self.start_remainder(event),
        }
    }

    fn start_remainder(&mut self, event: Event) -> InitializeState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> InitializeState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            InitializeState::Complete
        } else {
            InitializeState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, InitializeState::Complete)
    }

    fn take_response(&self) -> Option<crate::InitializeResponse> {
        if self.malformed || !self.is_complete() || self.field != INITIALIZE_FIELDS.len() {
            return None;
        }
        let product = self.product.as_str()?;
        let family = self.platform_family.as_str()?;
        let os = self.platform_os.as_str()?;
        let platform = crate::InitializePlatform::from_wire_pair(family, os)?;
        crate::InitializeResponse::try_new(product, platform).ok()
    }
}

const INITIALIZE_FIELDS: [&[u8]; 4] = [
    b"userAgent",
    b"codexHome",
    b"platformFamily",
    b"platformOs",
];

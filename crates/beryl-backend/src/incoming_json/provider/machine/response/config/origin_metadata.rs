struct OriginMetadataMachine {
    state: OriginMetadataState,
    source_type: FixedScalar<32>,
    proven: bool,
}

enum OriginMetadataState {
    Start,
    Name(ExactName),
    NameValue,
    SourceName(ExactName),
    SourceValue,
    SourceType,
    SourceEnd,
    VersionName(ExactName),
    VersionValue,
    Version,
    MetadataEnd,
    Complete,
    Invalid(ValueTracker),
}

impl OriginMetadataMachine {
    const fn new() -> Self {
        Self {
            state: OriginMetadataState::Start,
            source_type: FixedScalar::new(),
            proven: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            OriginMetadataState::Name(name)
            | OriginMetadataState::SourceName(name)
            | OriginMetadataState::VersionName(name) => name.push(bytes),
            OriginMetadataState::SourceType => self.source_type.push(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, OriginMetadataState::Complete);
        self.state = match state {
            OriginMetadataState::Start => match event {
                Event::ContainerStart(ContainerKind::Object) => {
                    OriginMetadataState::Name(ExactName::new(b"name"))
                }
                _ => Self::invalid(event),
            },
            OriginMetadataState::Name(name) => match event {
                Event::ScalarStart(ScalarKind::Name) => OriginMetadataState::Name(name),
                Event::ScalarFragment(ScalarKind::Name) => OriginMetadataState::Name(name),
                Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => {
                    OriginMetadataState::NameValue
                }
                _ => Self::invalid(event),
            },
            OriginMetadataState::NameValue => match event {
                Event::ContainerStart(ContainerKind::Object) => {
                    OriginMetadataState::SourceName(ExactName::new(b"type"))
                }
                _ => Self::invalid(event),
            },
            OriginMetadataState::SourceName(name) => match event {
                Event::ScalarStart(ScalarKind::Name) => OriginMetadataState::SourceName(name),
                Event::ScalarFragment(ScalarKind::Name) => OriginMetadataState::SourceName(name),
                Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => {
                    OriginMetadataState::SourceValue
                }
                _ => Self::invalid(event),
            },
            OriginMetadataState::SourceValue => match event {
                Event::ScalarStart(ScalarKind::String) => OriginMetadataState::SourceType,
                _ => Self::invalid(event),
            },
            OriginMetadataState::SourceType => match event {
                Event::ScalarFragment(ScalarKind::String) => OriginMetadataState::SourceType,
                Event::ScalarEnd(ScalarKind::String)
                    if self.source_type.as_str() == Some("sessionFlags") =>
                {
                    OriginMetadataState::SourceEnd
                }
                _ => Self::invalid(event),
            },
            OriginMetadataState::SourceEnd => match event {
                Event::ContainerEnd(ContainerKind::Object) => {
                    self.proven = true;
                    OriginMetadataState::VersionName(ExactName::new(b"version"))
                }
                _ => Self::invalid(event),
            },
            OriginMetadataState::VersionName(name) => match event {
                Event::ScalarStart(ScalarKind::Name) => OriginMetadataState::VersionName(name),
                Event::ScalarFragment(ScalarKind::Name) => OriginMetadataState::VersionName(name),
                Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => {
                    OriginMetadataState::VersionValue
                }
                _ => Self::invalid(event),
            },
            OriginMetadataState::VersionValue => {
                if matches!(event, Event::ScalarStart(ScalarKind::String)) {
                    OriginMetadataState::Version
                } else {
                    Self::invalid(event)
                }
            }
            OriginMetadataState::Version => match event {
                Event::ScalarFragment(ScalarKind::String) => OriginMetadataState::Version,
                Event::ScalarEnd(ScalarKind::String) => OriginMetadataState::MetadataEnd,
                _ => Self::invalid(event),
            },
            OriginMetadataState::MetadataEnd => match event {
                Event::ContainerEnd(ContainerKind::Object) => OriginMetadataState::Complete,
                _ => Self::invalid(event),
            },
            OriginMetadataState::Complete => OriginMetadataState::Complete,
            OriginMetadataState::Invalid(mut value) => {
                let _ = value.event(event);
                if value.is_complete() {
                    OriginMetadataState::Complete
                } else {
                    OriginMetadataState::Invalid(value)
                }
            }
        };
    }

    fn invalid(event: Event) -> OriginMetadataState {
        let mut value = ValueTracker::new();
        if value.event(event) && !value.is_complete() {
            OriginMetadataState::Invalid(value)
        } else {
            OriginMetadataState::Complete
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, OriginMetadataState::Complete)
    }

    const fn is_proven(&self) -> bool {
        self.proven && self.is_complete()
    }
}

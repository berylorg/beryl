struct SuccessResponse {
    actual_id: Option<u64>,
    expected: Option<ResponseExpectation>,
    result: SuccessResultMachine,
    root_depth: u16,
    root_complete: bool,
    malformed: bool,
}

impl SuccessResponse {
    fn new(actual_id: Option<u64>, expected: Option<ResponseExpectation>) -> Self {
        let result = match expected {
            Some(expectation) if actual_id == Some(expectation.id) => {
                SuccessResultMachine::new(expectation.family)
            }
            _ => SuccessResultMachine::Discard(ValueTracker::new()),
        };
        Self {
            actual_id,
            expected,
            result,
            root_depth: 1,
            root_complete: false,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        self.result.scratch_bytes(bytes);
    }

    fn event(&mut self, event: Event) {
        if !self.result.is_complete() {
            self.note_depth(event);
            self.result.event(event);
            return;
        }
        if matches!(event, Event::ContainerEnd(ContainerKind::Object)) && self.root_depth == 1 {
            self.root_depth = 0;
            self.root_complete = true;
            return;
        }
        self.malformed = true;
        self.note_depth(event);
    }

    fn note_depth(&mut self, event: Event) {
        match event {
            Event::ContainerStart(_) => self.root_depth = self.root_depth.saturating_add(1),
            Event::ContainerEnd(_) if self.root_depth > 0 => self.root_depth -= 1,
            _ => {}
        }
    }

    fn finish(&mut self) -> Result<DecodedIncoming, ForegroundIngressError> {
        if self.malformed || !self.result.is_complete() || !self.root_complete {
            return Err(ForegroundIngressError::MalformedResponse);
        }
        let Some(expected) = self.expected else {
            return Err(ForegroundIngressError::IdleResponse);
        };
        if self.actual_id != Some(expected.id) {
            return Err(ForegroundIngressError::ResponseIdMismatch {
                expected: expected.id,
                actual: self.actual_id,
            });
        }
        match self.result.take_result() {
            SuccessResult::Decoded(result) => Ok(DecodedIncoming::Response {
                id: expected.id,
                result,
            }),
            SuccessResult::Unavailable => Err(ForegroundIngressError::ResponseFamilyUnavailable {
                method: expected.family.method(),
            }),
            SuccessResult::Malformed => Err(ForegroundIngressError::MalformedResponse),
        }
    }
}

enum SuccessResult {
    Decoded(crate::BoundedResponseResult),
    Unavailable,
    Malformed,
}

enum SuccessResultMachine {
    Initialize(InitializeResultMachine),
    ConfigRead(ConfigResultMachine),
    ModelList(ModelPageMachine),
    ThreadLineage(LineageResultMachine),
    ThreadRead(ThreadReadResultMachine),
    TurnStart(TurnStartResultMachine),
    TurnSteer(TurnSteerResultMachine),
    EmptyAcknowledgement {
        acknowledgement: crate::EmptyAcknowledgement,
        schema: OrderedObjectMachine,
    },
    ThreadUnsubscribe(UnsubscribeResultMachine),
    Unavailable(ValueTracker),
    Discard(ValueTracker),
}

const EMPTY_RESULT_FIELDS: [OrderedField; 0] = [];

impl SuccessResultMachine {
    fn new(family: ResponseFamily) -> Self {
        match family {
            ResponseFamily::Initialize => Self::Initialize(InitializeResultMachine::new()),
            ResponseFamily::ConfigRead => Self::ConfigRead(ConfigResultMachine::new()),
            ResponseFamily::ModelList => Self::ModelList(ModelPageMachine::new()),
            ResponseFamily::ThreadStart => {
                Self::ThreadLineage(LineageResultMachine::new(LineageResultKind::Start))
            }
            ResponseFamily::ThreadRead => Self::ThreadRead(ThreadReadResultMachine::new()),
            ResponseFamily::ThreadResume => {
                Self::ThreadLineage(LineageResultMachine::new(LineageResultKind::Resume))
            }
            ResponseFamily::ThreadFork => {
                Self::ThreadLineage(LineageResultMachine::new(LineageResultKind::Fork))
            }
            ResponseFamily::TurnStart => Self::TurnStart(TurnStartResultMachine::new()),
            ResponseFamily::TurnSteer => Self::TurnSteer(TurnSteerResultMachine::new()),
            ResponseFamily::ThreadCompactStart => {
                Self::empty(crate::EmptyAcknowledgement::ThreadCompactStart)
            }
            ResponseFamily::ThreadInjectItems => {
                Self::empty(crate::EmptyAcknowledgement::ThreadInjectItems)
            }
            ResponseFamily::TurnInterrupt => {
                Self::empty(crate::EmptyAcknowledgement::TurnInterrupt)
            }
            ResponseFamily::ThreadUnsubscribe => {
                Self::ThreadUnsubscribe(UnsubscribeResultMachine::new())
            }
            _ => Self::Unavailable(ValueTracker::new()),
        }
    }

    const fn empty(acknowledgement: crate::EmptyAcknowledgement) -> Self {
        Self::EmptyAcknowledgement {
            acknowledgement,
            schema: OrderedObjectMachine::new(&EMPTY_RESULT_FIELDS),
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match self {
            Self::Initialize(machine) => machine.scratch_bytes(bytes),
            Self::ConfigRead(machine) => machine.scratch_bytes(bytes),
            Self::ModelList(machine) => machine.scratch_bytes(bytes),
            Self::ThreadLineage(machine) => machine.scratch_bytes(bytes),
            Self::ThreadRead(machine) => machine.scratch_bytes(bytes),
            Self::TurnStart(machine) => machine.scratch_bytes(bytes),
            Self::TurnSteer(machine) => machine.scratch_bytes(bytes),
            Self::EmptyAcknowledgement { schema, .. } => schema.scratch_bytes(bytes),
            Self::ThreadUnsubscribe(machine) => machine.scratch_bytes(bytes),
            Self::Unavailable(_) | Self::Discard(_) => {}
        }
    }

    fn event(&mut self, event: Event) {
        match self {
            Self::Initialize(machine) => machine.event(event),
            Self::ConfigRead(machine) => machine.event(event),
            Self::ModelList(machine) => machine.event(event),
            Self::ThreadLineage(machine) => machine.event(event),
            Self::ThreadRead(machine) => machine.event(event),
            Self::TurnStart(machine) => machine.event(event),
            Self::TurnSteer(machine) => machine.event(event),
            Self::EmptyAcknowledgement { schema, .. } => schema.event(event),
            Self::ThreadUnsubscribe(machine) => machine.event(event),
            Self::Unavailable(value) | Self::Discard(value) => {
                value.event(event);
            }
        }
    }

    fn is_complete(&self) -> bool {
        match self {
            Self::Initialize(machine) => machine.is_complete(),
            Self::ConfigRead(machine) => machine.is_complete(),
            Self::ModelList(machine) => machine.is_complete(),
            Self::ThreadLineage(machine) => machine.is_complete(),
            Self::ThreadRead(machine) => machine.is_complete(),
            Self::TurnStart(machine) => machine.is_complete(),
            Self::TurnSteer(machine) => machine.is_complete(),
            Self::EmptyAcknowledgement { schema, .. } => schema.is_complete(),
            Self::ThreadUnsubscribe(machine) => machine.is_complete(),
            Self::Unavailable(value) | Self::Discard(value) => value.is_complete(),
        }
    }

    fn take_result(&mut self) -> SuccessResult {
        match self {
            Self::Initialize(machine) => machine
                .take_response()
                .map(crate::BoundedResponseResult::Initialize)
                .map_or(SuccessResult::Malformed, SuccessResult::Decoded),
            Self::ConfigRead(machine) => machine
                .take_response()
                .map(crate::BoundedResponseResult::ConfigRead)
                .map_or(SuccessResult::Malformed, SuccessResult::Decoded),
            Self::ModelList(machine) => machine
                .take_page()
                .map(crate::BoundedResponseResult::ModelList)
                .map_or(SuccessResult::Malformed, SuccessResult::Decoded),
            Self::ThreadLineage(machine) => machine
                .take_result()
                .map_or(SuccessResult::Malformed, SuccessResult::Decoded),
            Self::ThreadRead(machine) => machine
                .take_response()
                .map(crate::BoundedResponseResult::ThreadRead)
                .map_or(SuccessResult::Malformed, SuccessResult::Decoded),
            Self::TurnStart(machine) => machine
                .take_response()
                .map(crate::BoundedResponseResult::TurnStart)
                .map_or(SuccessResult::Malformed, SuccessResult::Decoded),
            Self::TurnSteer(machine) => machine
                .take_response()
                .map(crate::BoundedResponseResult::TurnSteer)
                .map_or(SuccessResult::Malformed, SuccessResult::Decoded),
            Self::EmptyAcknowledgement {
                acknowledgement,
                schema,
            } => {
                if schema.is_valid() {
                    SuccessResult::Decoded(crate::BoundedResponseResult::EmptyAcknowledgement(
                        *acknowledgement,
                    ))
                } else {
                    SuccessResult::Malformed
                }
            }
            Self::ThreadUnsubscribe(machine) => machine
                .take_status()
                .map(crate::BoundedResponseResult::ThreadUnsubscribe)
                .map_or(SuccessResult::Malformed, SuccessResult::Decoded),
            Self::Unavailable(value) if value.is_complete() => SuccessResult::Unavailable,
            Self::Unavailable(_) | Self::Discard(_) => SuccessResult::Malformed,
        }
    }
}

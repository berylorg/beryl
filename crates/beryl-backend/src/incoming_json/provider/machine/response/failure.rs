struct FailureResponse {
    expected: Option<ResponseExpectation>,
    phase: FailurePhase,
    error: ErrorObject,
    actual_id: Option<u64>,
    root_depth: u16,
    root_complete: bool,
    malformed: bool,
}

enum FailurePhase {
    Error,
    ExpectIdName,
    IdName(ClassifierProbe),
    ExpectIdValue,
    IdNumber(NumberBytes),
    IdDiscard(ValueTracker),
    AfterId,
    Trailing,
}

impl FailureResponse {
    fn new(expected: Option<ResponseExpectation>) -> Self {
        Self {
            expected,
            phase: FailurePhase::Error,
            error: ErrorObject::new(),
            actual_id: None,
            root_depth: 1,
            root_complete: false,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.phase {
            FailurePhase::Error => self.error.bytes(bytes),
            FailurePhase::IdName(probe) => {
                probe.push(bytes, &ID_NAME);
            }
            FailurePhase::IdNumber(number) => number.push(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        self.note_depth(event);
        match &mut self.phase {
            FailurePhase::Error => {
                self.error.event(event);
                if self.error.is_complete() {
                    self.phase = FailurePhase::ExpectIdName;
                }
            }
            FailurePhase::ExpectIdName => match event {
                Event::ScalarStart(ScalarKind::Name) => {
                    let mut probe = ClassifierProbe::new();
                    probe.reset(1);
                    self.phase = FailurePhase::IdName(probe);
                }
                _ => self.enter_trailing(),
            },
            FailurePhase::IdName(probe) => match event {
                Event::ScalarFragment(ScalarKind::Name) => {}
                Event::ScalarEnd(ScalarKind::Name) if probe.exact(0, ID_NAME[0].len()) => {
                    self.phase = FailurePhase::ExpectIdValue;
                }
                Event::ScalarEnd(ScalarKind::Name) => self.enter_trailing(),
                _ => self.enter_trailing(),
            },
            FailurePhase::ExpectIdValue => match event {
                Event::ScalarStart(ScalarKind::Number) => {
                    self.phase = FailurePhase::IdNumber(NumberBytes::new());
                }
                Event::ContainerStart(_) | Event::ScalarStart(_) | Event::Boolean(_) | Event::Null => {
                    let mut value = ValueTracker::new();
                    if !value.event(event) {
                        self.malformed = true;
                    }
                    if value.is_complete() {
                        self.phase = FailurePhase::AfterId;
                    } else {
                        self.phase = FailurePhase::IdDiscard(value);
                    }
                    self.malformed = true;
                }
                _ => self.enter_trailing(),
            },
            FailurePhase::IdNumber(number) => match event {
                Event::ScalarFragment(ScalarKind::Number) => {}
                Event::ScalarEnd(ScalarKind::Number) => {
                    self.actual_id = number.parse_u64();
                    if self.actual_id.is_none() {
                        self.malformed = true;
                    }
                    self.phase = FailurePhase::AfterId;
                }
                _ => self.enter_trailing(),
            },
            FailurePhase::IdDiscard(value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    self.phase = FailurePhase::AfterId;
                }
            }
            FailurePhase::AfterId => {
                if matches!(event, Event::ContainerEnd(ContainerKind::Object))
                    && self.root_depth == 0
                {
                    self.root_complete = true;
                } else {
                    self.enter_trailing();
                }
            }
            FailurePhase::Trailing => {
                if self.root_depth == 0 {
                    self.root_complete = true;
                }
            }
        }
    }

    fn note_depth(&mut self, event: Event) {
        match event {
            Event::ContainerStart(_) => self.root_depth = self.root_depth.saturating_add(1),
            Event::ContainerEnd(_) if self.root_depth > 0 => self.root_depth -= 1,
            _ => {}
        }
    }

    fn enter_trailing(&mut self) {
        self.malformed = true;
        self.phase = FailurePhase::Trailing;
        if self.root_depth == 0 {
            self.root_complete = true;
        }
    }

    fn finish(&mut self) -> Result<DecodedIncoming, ForegroundIngressError> {
        if self.malformed || self.error.malformed || !self.root_complete {
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
        self.error
            .take_error(expected.family)
            .map(|error| DecodedIncoming::Rejection {
                id: expected.id,
                error,
            })
            .ok_or(ForegroundIngressError::MalformedResponse)
    }
}

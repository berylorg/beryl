fn new_effort_probe() -> ClassifierProbe {
    let mut probe = ClassifierProbe::new();
    probe.reset((1_u16 << REASONING_EFFORT_WIRES.len()) - 1);
    probe
}

fn effort_from_probe(probe: ClassifierProbe) -> Option<crate::ReasoningEffort> {
    REASONING_EFFORT_WIRES
        .iter()
        .enumerate()
        .find_map(|(index, wire)| {
            probe
                .exact(index, wire.len())
                .then_some(REASONING_EFFORT_VALUES[index])
        })
}

const REASONING_EFFORT_WIRES: [&[u8]; 8] = [
    b"none", b"minimal", b"low", b"medium", b"high", b"xhigh", b"max", b"ultra",
];

const REASONING_EFFORT_VALUES: [crate::ReasoningEffort; 8] = [
    crate::ReasoningEffort::None,
    crate::ReasoningEffort::Minimal,
    crate::ReasoningEffort::Low,
    crate::ReasoningEffort::Medium,
    crate::ReasoningEffort::High,
    crate::ReasoningEffort::XHigh,
    crate::ReasoningEffort::Max,
    crate::ReasoningEffort::Ultra,
];

struct EffortRecordMachine {
    state: EffortRecordState,
    target: bool,
    seen_target: bool,
    effort: Option<crate::ReasoningEffort>,
    malformed: bool,
}

enum EffortRecordState {
    Name,
    NameScalar(ClassifierProbe),
    Value,
    Effort(ClassifierProbe),
    Discard(ValueTracker),
    Remainder(u16),
    Complete,
}

impl EffortRecordMachine {
    const fn started() -> Self {
        Self {
            state: EffortRecordState::Name,
            target: false,
            seen_target: false,
            effort: None,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            EffortRecordState::NameScalar(probe) => {
                probe.push(bytes, &EFFORT_RECORD_TARGET_NAME);
            }
            EffortRecordState::Effort(probe) => {
                probe.push(bytes, &REASONING_EFFORT_WIRES);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, EffortRecordState::Complete);
        self.state = match state {
            EffortRecordState::Name => self.name(event),
            EffortRecordState::NameScalar(probe) => self.finish_name(probe, event),
            EffortRecordState::Value => self.value(event),
            EffortRecordState::Effort(probe) => self.finish_effort(probe, event),
            EffortRecordState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    EffortRecordState::Name
                } else {
                    EffortRecordState::Discard(value)
                }
            }
            EffortRecordState::Remainder(depth) => self.remainder(depth, event),
            EffortRecordState::Complete => EffortRecordState::Complete,
        };
    }

    fn name(&mut self, event: Event) -> EffortRecordState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => EffortRecordState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                let mut probe = ClassifierProbe::new();
                probe.reset(1);
                EffortRecordState::NameScalar(probe)
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(&mut self, probe: ClassifierProbe, event: Event) -> EffortRecordState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => EffortRecordState::NameScalar(probe),
            Event::ScalarEnd(ScalarKind::Name) => {
                self.target = probe.exact(0, EFFORT_RECORD_TARGET_NAME[0].len());
                if self.target {
                    self.malformed |= self.seen_target;
                    self.seen_target = true;
                }
                EffortRecordState::Value
            }
            _ => self.start_remainder(event),
        }
    }

    fn value(&mut self, event: Event) -> EffortRecordState {
        if self.target {
            return match event {
                Event::ScalarStart(ScalarKind::String) => {
                    EffortRecordState::Effort(new_effort_probe())
                }
                _ => {
                    self.malformed = true;
                    self.start_discard(event)
                }
            };
        }
        self.start_discard(event)
    }

    fn finish_effort(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> EffortRecordState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => EffortRecordState::Effort(probe),
            Event::ScalarEnd(ScalarKind::String) => {
                if probe.length == 0 {
                    self.malformed = true;
                }
                self.effort = effort_from_probe(probe);
                EffortRecordState::Name
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_discard(&mut self, event: Event) -> EffortRecordState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return self.start_remainder(event);
        }
        if value.is_complete() {
            EffortRecordState::Name
        } else {
            EffortRecordState::Discard(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> EffortRecordState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> EffortRecordState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            EffortRecordState::Complete
        } else {
            EffortRecordState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, EffortRecordState::Complete)
    }

    const fn is_valid(&self) -> bool {
        self.is_complete() && self.seen_target && !self.malformed
    }
}

const EFFORT_RECORD_TARGET_NAME: [&[u8]; 1] = [b"reasoningEffort"];

struct SupportedEffortsMachine {
    state: SupportedEffortsState,
    efforts: crate::SupportedReasoningEfforts,
    malformed: bool,
}

enum SupportedEffortsState {
    Start,
    ArrayEntry,
    ArrayString(ClassifierProbe),
    ArrayRecord(EffortRecordMachine),
    ArrayDiscard(ValueTracker),
    MapName,
    MapNameScalar(ClassifierProbe),
    MapValue,
    MapDiscard(ValueTracker),
    Fallback(ValueTracker),
    Complete,
}

impl SupportedEffortsMachine {
    const fn new() -> Self {
        Self {
            state: SupportedEffortsState::Start,
            efforts: crate::SupportedReasoningEfforts::empty(),
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            SupportedEffortsState::ArrayString(probe)
            | SupportedEffortsState::MapNameScalar(probe) => {
                probe.push(bytes, &REASONING_EFFORT_WIRES);
            }
            SupportedEffortsState::ArrayRecord(record) => record.scratch_bytes(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, SupportedEffortsState::Complete);
        self.state = match state {
            SupportedEffortsState::Start => self.start(event),
            SupportedEffortsState::ArrayEntry => self.array_entry(event),
            SupportedEffortsState::ArrayString(probe) => self.array_string(probe, event),
            SupportedEffortsState::ArrayRecord(mut record) => {
                record.event(event);
                if record.is_complete() {
                    self.malformed |= !record.is_valid();
                    if let Some(effort) = record.effort {
                        self.efforts.insert(effort);
                    }
                    SupportedEffortsState::ArrayEntry
                } else {
                    SupportedEffortsState::ArrayRecord(record)
                }
            }
            SupportedEffortsState::ArrayDiscard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    SupportedEffortsState::ArrayEntry
                } else {
                    SupportedEffortsState::ArrayDiscard(value)
                }
            }
            SupportedEffortsState::MapName => self.map_name(event),
            SupportedEffortsState::MapNameScalar(probe) => self.map_name_scalar(probe, event),
            SupportedEffortsState::MapValue => self.map_value(event),
            SupportedEffortsState::MapDiscard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    SupportedEffortsState::MapName
                } else {
                    SupportedEffortsState::MapDiscard(value)
                }
            }
            SupportedEffortsState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    SupportedEffortsState::Complete
                } else {
                    SupportedEffortsState::Fallback(value)
                }
            }
            SupportedEffortsState::Complete => SupportedEffortsState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> SupportedEffortsState {
        match event {
            Event::ContainerStart(ContainerKind::Array) => SupportedEffortsState::ArrayEntry,
            Event::ContainerStart(ContainerKind::Object) => SupportedEffortsState::MapName,
            _ => {
                self.malformed = true;
                let mut value = ValueTracker::new();
                if !value.event(event) || value.is_complete() {
                    SupportedEffortsState::Complete
                } else {
                    SupportedEffortsState::Fallback(value)
                }
            }
        }
    }

    fn array_entry(&mut self, event: Event) -> SupportedEffortsState {
        match event {
            Event::ContainerEnd(ContainerKind::Array) => SupportedEffortsState::Complete,
            Event::ScalarStart(ScalarKind::String) => {
                SupportedEffortsState::ArrayString(new_effort_probe())
            }
            Event::ContainerStart(ContainerKind::Object) => {
                SupportedEffortsState::ArrayRecord(EffortRecordMachine::started())
            }
            _ => {
                self.malformed = true;
                self.start_array_discard(event)
            }
        }
    }

    fn array_string(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> SupportedEffortsState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => {
                SupportedEffortsState::ArrayString(probe)
            }
            Event::ScalarEnd(ScalarKind::String) => {
                if probe.length == 0 {
                    self.malformed = true;
                }
                if let Some(effort) = effort_from_probe(probe) {
                    self.efforts.insert(effort);
                }
                SupportedEffortsState::ArrayEntry
            }
            _ => {
                self.malformed = true;
                SupportedEffortsState::ArrayEntry
            }
        }
    }

    fn start_array_discard(&mut self, event: Event) -> SupportedEffortsState {
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            SupportedEffortsState::ArrayEntry
        } else {
            SupportedEffortsState::ArrayDiscard(value)
        }
    }

    fn map_name(&mut self, event: Event) -> SupportedEffortsState {
        match event {
            Event::ContainerEnd(ContainerKind::Object) => SupportedEffortsState::Complete,
            Event::ScalarStart(ScalarKind::Name) => {
                SupportedEffortsState::MapNameScalar(new_effort_probe())
            }
            _ => {
                self.malformed = true;
                SupportedEffortsState::MapName
            }
        }
    }

    fn map_name_scalar(
        &mut self,
        probe: ClassifierProbe,
        event: Event,
    ) -> SupportedEffortsState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => {
                SupportedEffortsState::MapNameScalar(probe)
            }
            Event::ScalarEnd(ScalarKind::Name) => {
                if probe.length == 0 {
                    self.malformed = true;
                }
                if let Some(effort) = effort_from_probe(probe) {
                    self.efforts.insert(effort);
                }
                SupportedEffortsState::MapValue
            }
            _ => {
                self.malformed = true;
                SupportedEffortsState::MapName
            }
        }
    }

    fn map_value(&mut self, event: Event) -> SupportedEffortsState {
        let mut value = ValueTracker::new();
        if !value.event(event) {
            self.malformed = true;
            return SupportedEffortsState::MapName;
        }
        if value.is_complete() {
            SupportedEffortsState::MapName
        } else {
            SupportedEffortsState::MapDiscard(value)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, SupportedEffortsState::Complete)
    }

    fn take_efforts(&mut self) -> Option<crate::SupportedReasoningEfforts> {
        if self.malformed || !self.is_complete() {
            None
        } else {
            Some(std::mem::replace(
                &mut self.efforts,
                crate::SupportedReasoningEfforts::empty(),
            ))
        }
    }
}

struct OrderedObjectMachine {
    fields: &'static [OrderedField],
    index: usize,
    state: OrderedObjectState,
    malformed: bool,
}

enum OrderedObjectState {
    Start,
    Name,
    NameScalar(ExactName),
    Value,
    Discard(ValueTracker),
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

impl OrderedObjectMachine {
    const fn new(fields: &'static [OrderedField]) -> Self {
        Self {
            fields,
            index: 0,
            state: OrderedObjectState::Start,
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        if let OrderedObjectState::NameScalar(name) = &mut self.state {
            name.push(bytes);
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, OrderedObjectState::Complete);
        self.state = match state {
            OrderedObjectState::Start => self.start(event),
            OrderedObjectState::Name => self.name(event),
            OrderedObjectState::NameScalar(name) => self.name_scalar(name, event),
            OrderedObjectState::Value => self.value(event),
            OrderedObjectState::Discard(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    self.index += 1;
                    OrderedObjectState::Name
                } else {
                    OrderedObjectState::Discard(value)
                }
            }
            OrderedObjectState::Remainder(depth) => self.remainder(depth, event),
            OrderedObjectState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    OrderedObjectState::Complete
                } else {
                    OrderedObjectState::Fallback(value)
                }
            }
            OrderedObjectState::Complete => OrderedObjectState::Complete,
        };
    }

    fn start(&mut self, event: Event) -> OrderedObjectState {
        if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
            return OrderedObjectState::Name;
        }
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            OrderedObjectState::Complete
        } else {
            OrderedObjectState::Fallback(value)
        }
    }

    fn name(&mut self, event: Event) -> OrderedObjectState {
        if self.index == self.fields.len() {
            return if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
                OrderedObjectState::Complete
            } else {
                self.start_remainder(event)
            };
        }
        match event {
            Event::ScalarStart(ScalarKind::Name) => {
                OrderedObjectState::NameScalar(ExactName::new(self.fields[self.index].name))
            }
            _ => self.start_remainder(event),
        }
    }

    fn name_scalar(&mut self, name: ExactName, event: Event) -> OrderedObjectState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => OrderedObjectState::NameScalar(name),
            Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => OrderedObjectState::Value,
            _ => self.start_remainder(event),
        }
    }

    fn value(&mut self, event: Event) -> OrderedObjectState {
        let shape_matches = match self.fields[self.index].shape {
            RequiredValueShape::Any => matches!(
                event,
                Event::ContainerStart(_)
                    | Event::ScalarStart(_)
                    | Event::Boolean(_)
                    | Event::Null
            ),
            RequiredValueShape::String => {
                matches!(event, Event::ScalarStart(ScalarKind::String))
            }
            RequiredValueShape::Object => {
                matches!(event, Event::ContainerStart(ContainerKind::Object))
            }
        };
        self.malformed |= !shape_matches;
        let mut value = ValueTracker::new();
        if !value.event(event) {
            return self.start_remainder(event);
        }
        if value.is_complete() {
            self.index += 1;
            OrderedObjectState::Name
        } else {
            OrderedObjectState::Discard(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> OrderedObjectState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> OrderedObjectState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            OrderedObjectState::Complete
        } else {
            OrderedObjectState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, OrderedObjectState::Complete)
    }

    const fn is_valid(&self) -> bool {
        self.is_complete() && !self.malformed && self.index == self.fields.len()
    }
}

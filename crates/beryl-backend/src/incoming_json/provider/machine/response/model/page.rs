struct ModelPageMachine {
    state: ModelPageState,
    page: Option<Box<crate::ModelPage>>,
    cursor: FixedScalar<1024>,
    malformed: bool,
}

enum ModelPageState {
    Start,
    DataNameStart,
    DataName(ExactName),
    DataValue,
    DataEntry,
    Record(ModelRecordMachine),
    InvalidEntry(ValueTracker),
    CursorNameStart,
    CursorName(ExactName),
    CursorValue,
    CursorString,
    AfterCursor,
    Remainder(u16),
    Fallback(ValueTracker),
    Complete,
}

impl ModelPageMachine {
    fn new() -> Self {
        Self {
            state: ModelPageState::Start,
            page: Some(Box::new(crate::ModelPage::new())),
            cursor: FixedScalar::new(),
            malformed: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) {
        match &mut self.state {
            ModelPageState::DataName(name) | ModelPageState::CursorName(name) => name.push(bytes),
            ModelPageState::Record(record) => record.scratch_bytes(bytes),
            ModelPageState::CursorString => self.cursor.push(bytes),
            _ => {}
        }
    }

    fn event(&mut self, event: Event) {
        let state = std::mem::replace(&mut self.state, ModelPageState::Complete);
        self.state = match state {
            ModelPageState::Start => {
                if matches!(event, Event::ContainerStart(ContainerKind::Object)) {
                    ModelPageState::DataNameStart
                } else {
                    self.start_fallback(event)
                }
            }
            ModelPageState::DataNameStart => self.start_name(event, true),
            ModelPageState::DataName(name) => {
                self.finish_name(name, event, ModelPageState::DataValue)
            }
            ModelPageState::DataValue => {
                if matches!(event, Event::ContainerStart(ContainerKind::Array)) {
                    ModelPageState::DataEntry
                } else {
                    self.start_remainder(event)
                }
            }
            ModelPageState::DataEntry => self.data_entry(event),
            ModelPageState::Record(mut record) => {
                record.event(event);
                if record.is_complete() {
                    if let Some(record) = record.take_record() {
                        if self
                            .page
                            .as_mut()
                            .is_none_or(|page| page.try_push(record).is_err())
                        {
                            self.malformed = true;
                        }
                    } else {
                        self.malformed = true;
                    }
                    ModelPageState::DataEntry
                } else {
                    ModelPageState::Record(record)
                }
            }
            ModelPageState::InvalidEntry(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ModelPageState::DataEntry
                } else {
                    ModelPageState::InvalidEntry(value)
                }
            }
            ModelPageState::CursorNameStart => self.start_name(event, false),
            ModelPageState::CursorName(name) => {
                self.finish_name(name, event, ModelPageState::CursorValue)
            }
            ModelPageState::CursorValue => self.cursor_value(event),
            ModelPageState::CursorString => self.cursor_string(event),
            ModelPageState::AfterCursor => {
                if matches!(event, Event::ContainerEnd(ContainerKind::Object)) {
                    ModelPageState::Complete
                } else {
                    self.start_remainder(event)
                }
            }
            ModelPageState::Remainder(depth) => self.remainder(depth, event),
            ModelPageState::Fallback(mut value) => {
                if !value.event(event) {
                    self.malformed = true;
                }
                if value.is_complete() {
                    ModelPageState::Complete
                } else {
                    ModelPageState::Fallback(value)
                }
            }
            ModelPageState::Complete => ModelPageState::Complete,
        };
    }

    fn start_name(&mut self, event: Event, is_data: bool) -> ModelPageState {
        match event {
            Event::ScalarStart(ScalarKind::Name) if is_data => {
                ModelPageState::DataName(ExactName::new(b"data"))
            }
            Event::ScalarStart(ScalarKind::Name) => {
                ModelPageState::CursorName(ExactName::new(b"nextCursor"))
            }
            _ => self.start_remainder(event),
        }
    }

    fn finish_name(
        &mut self,
        name: ExactName,
        event: Event,
        next: ModelPageState,
    ) -> ModelPageState {
        match event {
            Event::ScalarFragment(ScalarKind::Name) => match next {
                ModelPageState::DataValue => ModelPageState::DataName(name),
                ModelPageState::CursorValue => ModelPageState::CursorName(name),
                _ => unreachable!("model page name has one known successor"),
            },
            Event::ScalarEnd(ScalarKind::Name) if name.is_exact() => next,
            _ => self.start_remainder(event),
        }
    }

    fn data_entry(&mut self, event: Event) -> ModelPageState {
        match event {
            Event::ContainerEnd(ContainerKind::Array) => ModelPageState::CursorNameStart,
            Event::ContainerStart(ContainerKind::Object)
                if self
                    .page
                    .as_ref()
                    .is_some_and(|page| page.len() < crate::MODEL_PAGE_MAX_RECORDS) =>
            {
                ModelPageState::Record(ModelRecordMachine::started())
            }
            _ => {
                self.malformed = true;
                let mut value = ValueTracker::new();
                if !value.event(event) || value.is_complete() {
                    ModelPageState::DataEntry
                } else {
                    ModelPageState::InvalidEntry(value)
                }
            }
        }
    }

    fn cursor_value(&mut self, event: Event) -> ModelPageState {
        match event {
            Event::Null => {
                if let Some(page) = &mut self.page {
                    page.set_next_cursor(None);
                }
                ModelPageState::AfterCursor
            }
            Event::ScalarStart(ScalarKind::String) => ModelPageState::CursorString,
            _ => self.start_remainder(event),
        }
    }

    fn cursor_string(&mut self, event: Event) -> ModelPageState {
        match event {
            Event::ScalarFragment(ScalarKind::String) => ModelPageState::CursorString,
            Event::ScalarEnd(ScalarKind::String) => {
                let cursor = self
                    .cursor
                    .as_str()
                    .and_then(|value| crate::ModelPageCursor::try_new(value).ok());
                self.malformed |= cursor.is_none();
                if let Some(page) = &mut self.page {
                    page.set_next_cursor(cursor);
                }
                ModelPageState::AfterCursor
            }
            _ => self.start_remainder(event),
        }
    }

    fn start_fallback(&mut self, event: Event) -> ModelPageState {
        self.malformed = true;
        let mut value = ValueTracker::new();
        if !value.event(event) || value.is_complete() {
            ModelPageState::Complete
        } else {
            ModelPageState::Fallback(value)
        }
    }

    fn start_remainder(&mut self, event: Event) -> ModelPageState {
        self.malformed = true;
        self.remainder(1, event)
    }

    fn remainder(&mut self, mut depth: u16, event: Event) -> ModelPageState {
        match event {
            Event::ContainerStart(_) => depth = depth.saturating_add(1),
            Event::ContainerEnd(_) if depth > 0 => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            ModelPageState::Complete
        } else {
            ModelPageState::Remainder(depth)
        }
    }

    const fn is_complete(&self) -> bool {
        matches!(self.state, ModelPageState::Complete)
    }

    fn take_page(&mut self) -> Option<Box<crate::ModelPage>> {
        if self.malformed || !self.is_complete() {
            None
        } else {
            self.page.take()
        }
    }
}

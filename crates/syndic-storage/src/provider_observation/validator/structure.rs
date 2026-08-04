impl ProviderObservationValidatorState {
    fn begin_container(
        &mut self,
        context: ProviderValueContext,
        container: ProviderContainer,
        value: ValueKind,
    ) -> Result<(), ProviderObservationValidatorError> {
        if self.frames.len() >= PROVIDER_OBSERVATION_MAX_FRAME_DEPTH {
            return Err(ProviderObservationValidatorError::StructuredDepthExceeded);
        }
        let frame = match value {
            ValueKind::List(kind) if container == ProviderContainer::List => {
                ProviderObservationFrame::List {
                    context,
                    kind,
                    next: 0,
                }
            }
            ValueKind::Object(schema) if container == ProviderContainer::Object => {
                ProviderObservationFrame::Object {
                    context,
                    schema,
                    seen: [0; 2],
                    variant: None,
                }
            }
            ValueKind::AgentStates if container == ProviderContainer::Object => {
                ProviderObservationFrame::AgentStates { context, next: 0 }
            }
            ValueKind::Structured => {
                let depth = match context {
                    ProviderValueContext::Field(_) => 1,
                    ProviderValueContext::Structured { depth, .. } => depth
                        .checked_add(1)
                        .ok_or(ProviderObservationValidatorError::StructuredDepthExceeded)?,
                };
                if usize::from(depth) > MAX_STRUCTURED_DEPTH {
                    return Err(ProviderObservationValidatorError::StructuredDepthExceeded);
                }
                ProviderObservationFrame::Structured {
                    context,
                    container,
                    next: 0,
                    depth,
                }
            }
            _ => return Err(ProviderObservationValidatorError::ValueMismatch),
        };
        self.frames.push(frame);
        Ok(())
    }

    fn end_container(
        &mut self,
        context: ProviderValueContext,
        container: ProviderContainer,
    ) -> Result<(), ProviderObservationValidatorError> {
        let Some(frame) = self.frames.last().copied() else {
            return Err(ProviderObservationValidatorError::StructureMismatch);
        };
        let valid = match frame {
            ProviderObservationFrame::List {
                context: expected, ..
            } => expected == context && container == ProviderContainer::List,
            ProviderObservationFrame::AgentStates {
                context: expected, ..
            } => expected == context && container == ProviderContainer::Object,
            ProviderObservationFrame::Structured {
                context: expected,
                container: expected_container,
                ..
            } => expected == context && expected_container == container,
            ProviderObservationFrame::Object {
                context: expected,
                schema: object,
                seen,
                variant,
            } if expected == context && container == ProviderContainer::Object => {
                let fields = if let Some((_, _)) = schema::discriminant(object) {
                    let variant =
                        variant.ok_or(ProviderObservationValidatorError::MissingRequiredField)?;
                    schema::variant_fields(object, variant)
                        .ok_or(ProviderObservationValidatorError::EnumMismatch)?
                } else {
                    schema::object_fields(object)
                        .ok_or(ProviderObservationValidatorError::StructureMismatch)?
                };
                if !schema::required_fields_present(fields, seen)
                    || !schema::only_fields_seen(fields, seen)
                {
                    return Err(ProviderObservationValidatorError::MissingRequiredField);
                }
                true
            }
            _ => false,
        };
        if !valid {
            return Err(ProviderObservationValidatorError::StructureMismatch);
        }
        self.frames.pop();
        self.complete_value()
    }

    fn begin_element(
        &mut self,
        context: ProviderValueContext,
        index: u64,
    ) -> Result<(), ProviderObservationValidatorError> {
        if self.frames.len() >= PROVIDER_OBSERVATION_MAX_FRAME_DEPTH {
            return Err(ProviderObservationValidatorError::StructuredDepthExceeded);
        }
        let kind = match self.frames.last_mut() {
            Some(ProviderObservationFrame::List {
                context: owner,
                kind,
                next,
            }) if *owner == context && *next == index => {
                *next = next
                    .checked_add(1)
                    .ok_or(ProviderObservationValidatorError::FrontierOverflow)?;
                ProviderObservationElementKind::Typed(*kind)
            }
            Some(ProviderObservationFrame::Structured {
                context: owner,
                container: ProviderContainer::List,
                next,
                depth,
            }) if *owner == context && *next == index => {
                *next = next
                    .checked_add(1)
                    .ok_or(ProviderObservationValidatorError::FrontierOverflow)?;
                ProviderObservationElementKind::Structured {
                    root: context.root(),
                    depth: *depth,
                }
            }
            Some(
                ProviderObservationFrame::List { .. }
                | ProviderObservationFrame::Structured {
                    container: ProviderContainer::List,
                    ..
                },
            ) => return Err(ProviderObservationValidatorError::IndexMismatch),
            _ => return Err(ProviderObservationValidatorError::StructureMismatch),
        };
        self.frames.push(ProviderObservationFrame::Element {
            context,
            index,
            kind,
            started: false,
            complete: false,
        });
        Ok(())
    }

    fn end_element(
        &mut self,
        context: ProviderValueContext,
        index: u64,
    ) -> Result<(), ProviderObservationValidatorError> {
        match self.frames.pop() {
            Some(ProviderObservationFrame::Element {
                context: expected,
                index: expected_index,
                started: true,
                complete: true,
                ..
            }) if expected == context && expected_index == index => Ok(()),
            Some(frame) => {
                self.frames.push(frame);
                Err(ProviderObservationValidatorError::StructureMismatch)
            }
            None => Err(ProviderObservationValidatorError::StructureMismatch),
        }
    }

    fn begin_object_entry(
        &mut self,
        root: ProviderField,
        depth: u8,
        entry: u64,
    ) -> Result<(), ProviderObservationValidatorError> {
        if self.frames.len() >= PROVIDER_OBSERVATION_MAX_FRAME_DEPTH {
            return Err(ProviderObservationValidatorError::StructuredDepthExceeded);
        }
        let frame = match self.frames.last_mut() {
            Some(ProviderObservationFrame::Structured {
                context,
                container: ProviderContainer::Object,
                next,
                depth: expected_depth,
            }) if context.root() == root && *expected_depth == depth && *next == entry => {
                *next = next
                    .checked_add(1)
                    .ok_or(ProviderObservationValidatorError::FrontierOverflow)?;
                ProviderObservationFrame::StructuredEntry {
                    root,
                    depth,
                    entry,
                    key_started: false,
                    key_complete: false,
                    value_started: false,
                    value_complete: false,
                }
            }
            Some(ProviderObservationFrame::AgentStates { context, next })
                if context.root() == ProviderField::CollabAgentStates
                    && root == ProviderField::CollabAgentStates
                    && depth == 0
                    && *next == entry =>
            {
                *next = next
                    .checked_add(1)
                    .ok_or(ProviderObservationValidatorError::FrontierOverflow)?;
                ProviderObservationFrame::AgentStateEntry {
                    entry,
                    key_started: false,
                    key_complete: false,
                    seen: [0; 2],
                }
            }
            Some(
                ProviderObservationFrame::Structured {
                    container: ProviderContainer::Object,
                    ..
                }
                | ProviderObservationFrame::AgentStates { .. },
            ) => return Err(ProviderObservationValidatorError::IndexMismatch),
            _ => return Err(ProviderObservationValidatorError::StructureMismatch),
        };
        self.frames.push(frame);
        Ok(())
    }

    fn end_object_entry(
        &mut self,
        root: ProviderField,
        depth: u8,
        entry: u64,
    ) -> Result<(), ProviderObservationValidatorError> {
        let Some(frame) = self.frames.last().copied() else {
            return Err(ProviderObservationValidatorError::StructureMismatch);
        };
        match frame {
            ProviderObservationFrame::StructuredEntry {
                root: expected_root,
                depth: expected_depth,
                entry: expected_entry,
                key_started: true,
                key_complete: true,
                value_started: true,
                value_complete: true,
            } if expected_root == root && expected_depth == depth && expected_entry == entry => {}
            ProviderObservationFrame::AgentStateEntry {
                entry: expected_entry,
                key_started: true,
                key_complete: true,
                seen,
            } if root == ProviderField::CollabAgentStates
                && depth == 0
                && expected_entry == entry =>
            {
                let fields = schema::object_fields(schema::ObjectSchema::CollabAgentState)
                    .expect("collab agent state has a fixed schema");
                if !schema::required_fields_present(fields, seen) {
                    return Err(ProviderObservationValidatorError::MissingRequiredField);
                }
            }
            ProviderObservationFrame::StructuredEntry { .. }
            | ProviderObservationFrame::AgentStateEntry { .. } => {
                return Err(ProviderObservationValidatorError::StructureMismatch);
            }
            _ => return Err(ProviderObservationValidatorError::StructureMismatch),
        }
        self.frames.pop();
        Ok(())
    }
}

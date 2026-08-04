impl ProviderObservationValidatorState {
    fn claim_value(
        &mut self,
        begin: ProviderObservationBegin,
        context: ProviderValueContext,
        control: ValueControl,
    ) -> Result<ValueKind, ProviderObservationValidatorError> {
        if matches!(
            self.frames.last(),
            Some(ProviderObservationFrame::Element { .. })
        ) {
            return self.claim_element_value(context, control);
        }
        if matches!(
            self.frames.last(),
            Some(
                ProviderObservationFrame::StructuredEntry { .. }
                    | ProviderObservationFrame::AgentStateEntry { .. }
            )
        ) && matches!(context, ProviderValueContext::Structured { .. })
        {
            return self.claim_entry_value(context, control);
        }
        let ProviderValueContext::Field(field) = context else {
            return Err(ProviderObservationValidatorError::StructureMismatch);
        };
        let last = self.frames.last().copied();
        match last {
            Some(ProviderObservationFrame::Object { .. }) => {
                self.claim_object_field(field, control)
            }
            Some(ProviderObservationFrame::AgentStateEntry {
                key_complete: true, ..
            }) => self.claim_agent_state_field(field, control),
            Some(_) => Err(ProviderObservationValidatorError::StructureMismatch),
            None => {
                let spec = schema::top_field(begin, field)
                    .ok_or(ProviderObservationValidatorError::FieldNotAllowed)?;
                if schema::mark_field(&mut self.seen_fields, field) {
                    return Err(ProviderObservationValidatorError::DuplicateField);
                }
                validate_value(spec, control)
            }
        }
    }

    fn claim_element_value(
        &mut self,
        context: ProviderValueContext,
        control: ValueControl,
    ) -> Result<ValueKind, ProviderObservationValidatorError> {
        let Some(ProviderObservationFrame::Element {
            context: owner,
            index,
            kind,
            started: false,
            complete: false,
        }) = self.frames.last().copied()
        else {
            return Err(ProviderObservationValidatorError::StructureMismatch);
        };
        let (expected_context, value) = match kind {
            ProviderObservationElementKind::Typed(list) => {
                let (field, value) = schema::list_value(list);
                let context = if matches!(value, ValueKind::Object(_)) {
                    owner
                } else {
                    ProviderValueContext::Field(field)
                };
                (context, value)
            }
            ProviderObservationElementKind::Structured { root, depth } => (
                ProviderValueContext::Structured {
                    root,
                    depth,
                    position: ProviderStructuredPosition::ListElement { index },
                },
                ValueKind::Structured,
            ),
        };
        if context != expected_context {
            return Err(ProviderObservationValidatorError::StructureMismatch);
        }
        let value = validate_value(
            FieldSpec {
                field: context.root(),
                value,
                required: true,
                nullable: false,
            },
            control,
        )?;
        let Some(ProviderObservationFrame::Element { started, .. }) = self.frames.last_mut() else {
            unreachable!("element frame was inspected above")
        };
        *started = true;
        Ok(value)
    }

    fn claim_entry_value(
        &mut self,
        context: ProviderValueContext,
        control: ValueControl,
    ) -> Result<ValueKind, ProviderObservationValidatorError> {
        match self.frames.last().copied() {
            Some(ProviderObservationFrame::StructuredEntry {
                root,
                depth,
                entry,
                key_started,
                key_complete,
                value_started,
                value_complete,
            }) => {
                let expected = if !key_started {
                    ProviderValueContext::Structured {
                        root,
                        depth,
                        position: ProviderStructuredPosition::ObjectKey { entry },
                    }
                } else if key_complete && !value_started && !value_complete {
                    ProviderValueContext::Structured {
                        root,
                        depth,
                        position: ProviderStructuredPosition::ObjectValue { entry },
                    }
                } else {
                    return Err(ProviderObservationValidatorError::StructureMismatch);
                };
                if context != expected {
                    return Err(ProviderObservationValidatorError::StructureMismatch);
                }
                let key = !key_started;
                let value = validate_value(
                    FieldSpec {
                        field: root,
                        value: ValueKind::Structured,
                        required: true,
                        nullable: false,
                    },
                    control,
                )?;
                if key && !matches!(control, ValueControl::Text) {
                    return Err(ProviderObservationValidatorError::ValueMismatch);
                }
                let Some(ProviderObservationFrame::StructuredEntry {
                    key_started,
                    value_started,
                    ..
                }) = self.frames.last_mut()
                else {
                    unreachable!("structured entry frame was inspected above")
                };
                if key {
                    *key_started = true;
                } else {
                    *value_started = true;
                }
                Ok(value)
            }
            Some(ProviderObservationFrame::AgentStateEntry {
                entry,
                key_started: false,
                key_complete: false,
                ..
            }) => {
                let expected = ProviderValueContext::Structured {
                    root: ProviderField::CollabAgentStates,
                    depth: 0,
                    position: ProviderStructuredPosition::ObjectKey { entry },
                };
                if context != expected || !matches!(control, ValueControl::Text) {
                    return Err(ProviderObservationValidatorError::StructureMismatch);
                }
                let Some(ProviderObservationFrame::AgentStateEntry { key_started, .. }) =
                    self.frames.last_mut()
                else {
                    unreachable!("agent-state entry frame was inspected above")
                };
                *key_started = true;
                Ok(ValueKind::Text)
            }
            _ => Err(ProviderObservationValidatorError::StructureMismatch),
        }
    }

    fn claim_object_field(
        &mut self,
        field: ProviderField,
        control: ValueControl,
    ) -> Result<ValueKind, ProviderObservationValidatorError> {
        let Some(ProviderObservationFrame::Object {
            schema: object,
            mut seen,
            variant,
            ..
        }) = self.frames.last().copied()
        else {
            unreachable!("object field requires an object frame")
        };
        if let Some((discriminant, domain)) = schema::discriminant(object) {
            if field == discriminant {
                if variant.is_some() {
                    return Err(ProviderObservationValidatorError::DuplicateField);
                }
                let ValueControl::Enum(value) = control else {
                    return Err(ProviderObservationValidatorError::ValueMismatch);
                };
                if value == ProviderEnumValue::Other
                    && object != schema::ObjectSchema::WebSearchAction
                {
                    return Err(ProviderObservationValidatorError::OtherMarkerMismatch);
                }
                if !schema::enum_allowed(domain, value)
                    || schema::variant_fields(object, value).is_none()
                {
                    return Err(ProviderObservationValidatorError::EnumMismatch);
                }
                let fields = schema::variant_fields(object, value)
                    .ok_or(ProviderObservationValidatorError::EnumMismatch)?;
                if !schema::only_fields_seen(fields, seen) {
                    return Err(ProviderObservationValidatorError::FieldNotAllowed);
                }
                let Some(ProviderObservationFrame::Object { variant, .. }) = self.frames.last_mut()
                else {
                    unreachable!("object frame was inspected above")
                };
                *variant = Some(value);
                return Ok(ValueKind::Enum(domain));
            }
            let spec = match variant {
                Some(variant) => schema::variant_fields(object, variant)
                    .ok_or(ProviderObservationValidatorError::EnumMismatch)?
                    .iter()
                    .copied()
                    .find(|spec| spec.field == field),
                None => schema::variant_field(object, field),
            }
            .ok_or(ProviderObservationValidatorError::FieldNotAllowed)?;
            if schema::mark_field(&mut seen, field) {
                return Err(ProviderObservationValidatorError::DuplicateField);
            }
            let value = validate_value(spec, control)?;
            let Some(ProviderObservationFrame::Object {
                seen: frame_seen, ..
            }) = self.frames.last_mut()
            else {
                unreachable!("object frame was inspected above")
            };
            *frame_seen = seen;
            return Ok(value);
        }
        let fields = schema::object_fields(object)
            .ok_or(ProviderObservationValidatorError::FieldNotAllowed)?;
        self.claim_local_field(fields, seen, field, control)
    }

    fn claim_agent_state_field(
        &mut self,
        field: ProviderField,
        control: ValueControl,
    ) -> Result<ValueKind, ProviderObservationValidatorError> {
        let Some(ProviderObservationFrame::AgentStateEntry { seen, .. }) =
            self.frames.last().copied()
        else {
            unreachable!("agent-state field requires an entry frame")
        };
        let fields = schema::object_fields(schema::ObjectSchema::CollabAgentState)
            .expect("collab agent state has a fixed schema");
        self.claim_local_field(fields, seen, field, control)
    }

    fn claim_local_field(
        &mut self,
        fields: &'static [FieldSpec],
        mut seen: [u64; 2],
        field: ProviderField,
        control: ValueControl,
    ) -> Result<ValueKind, ProviderObservationValidatorError> {
        let spec = fields
            .iter()
            .copied()
            .find(|spec| spec.field == field)
            .ok_or(ProviderObservationValidatorError::FieldNotAllowed)?;
        if schema::mark_field(&mut seen, field) {
            return Err(ProviderObservationValidatorError::DuplicateField);
        }
        let value = validate_value(spec, control)?;
        match self.frames.last_mut() {
            Some(ProviderObservationFrame::Object {
                seen: frame_seen, ..
            })
            | Some(ProviderObservationFrame::AgentStateEntry {
                seen: frame_seen, ..
            }) => *frame_seen = seen,
            _ => unreachable!("local fields have one typed owner"),
        }
        Ok(value)
    }

    fn end_text(
        &mut self,
        context: ProviderValueContext,
    ) -> Result<(), ProviderObservationValidatorError> {
        if self.active_text != Some(context) {
            return Err(ProviderObservationValidatorError::TextContextMismatch);
        }
        if self.utf8.remaining != 0 {
            return Err(ProviderObservationValidatorError::InvalidUtf8);
        }
        if let Some(identity) = self.active_identity {
            identity.finish()?;
        }
        self.active_text = None;
        self.active_identity = None;
        self.utf8 = Utf8ValidatorState::new();
        match (context, self.frames.last_mut()) {
            (
                ProviderValueContext::Structured {
                    position: ProviderStructuredPosition::ObjectKey { .. },
                    ..
                },
                Some(ProviderObservationFrame::StructuredEntry {
                    key_started: true,
                    key_complete,
                    ..
                }),
            )
            | (
                ProviderValueContext::Structured {
                    position: ProviderStructuredPosition::ObjectKey { .. },
                    ..
                },
                Some(ProviderObservationFrame::AgentStateEntry {
                    key_started: true,
                    key_complete,
                    ..
                }),
            ) => {
                *key_complete = true;
                Ok(())
            }
            _ => self.complete_value(),
        }
    }

    fn complete_value(&mut self) -> Result<(), ProviderObservationValidatorError> {
        match self.frames.last_mut() {
            Some(ProviderObservationFrame::Element {
                started: true,
                complete,
                ..
            }) if !*complete => *complete = true,
            Some(ProviderObservationFrame::StructuredEntry {
                key_complete: true,
                value_started: true,
                value_complete,
                ..
            }) if !*value_complete => *value_complete = true,
            Some(
                ProviderObservationFrame::Element { .. }
                | ProviderObservationFrame::StructuredEntry { .. },
            ) => {
                return Err(ProviderObservationValidatorError::StructureMismatch);
            }
            _ => {}
        }
        Ok(())
    }
}

fn validate_value(
    spec: FieldSpec,
    control: ValueControl,
) -> Result<ValueKind, ProviderObservationValidatorError> {
    if matches!(control, ValueControl::Scalar(ProviderScalar::Null)) && spec.nullable {
        return Ok(spec.value);
    }
    let valid = match (spec.value, control) {
        (ValueKind::Text | ValueKind::Identity, ValueControl::Text) => true,
        (ValueKind::Enum(domain), ValueControl::Enum(value)) => {
            if value == ProviderEnumValue::Other && domain != schema::EnumDomain::WebAction {
                return Err(ProviderObservationValidatorError::OtherMarkerMismatch);
            }
            if !schema::enum_allowed(domain, value) {
                return Err(ProviderObservationValidatorError::EnumMismatch);
            }
            true
        }
        (ValueKind::Unsigned, ValueControl::Scalar(ProviderScalar::Unsigned(_))) => true,
        (ValueKind::Unsigned32, ValueControl::Scalar(ProviderScalar::Unsigned(value))) => {
            u32::try_from(value).is_ok()
        }
        (ValueKind::Signed, ValueControl::Scalar(ProviderScalar::Signed(_))) => true,
        (ValueKind::Signed32, ValueControl::Scalar(ProviderScalar::Signed(value))) => {
            i32::try_from(value).is_ok()
        }
        (ValueKind::Boolean, ValueControl::Scalar(ProviderScalar::Boolean(_))) => true,
        (
            ValueKind::Structured,
            ValueControl::Text | ValueControl::Scalar(_) | ValueControl::Container(_),
        ) => true,
        (ValueKind::Object(_), ValueControl::Container(ProviderContainer::Object))
        | (ValueKind::AgentStates, ValueControl::Container(ProviderContainer::Object))
        | (ValueKind::List(_), ValueControl::Container(ProviderContainer::List)) => true,
        _ => false,
    };
    if valid {
        Ok(spec.value)
    } else {
        Err(ProviderObservationValidatorError::ValueMismatch)
    }
}

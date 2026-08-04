impl TargetMachine<'_> {
    fn start_schema_string(
        &mut self,
        spec: schema::FieldSpec,
        after: After,
    ) -> Result<(), MachineError> {
        match spec.value {
            schema::ValueKind::ItemId => {
                let context = ProviderValueContext::Field(ProviderField::ItemId);
                self.capture_mut()?.begin_text(context)?;
                self.scalar = ScalarHandler::Fixed {
                    purpose: FixedPurpose::Identity(RouteValue::ItemId),
                    bytes: FixedBytes::new(),
                    after,
                };
                Ok(())
            }
            schema::ValueKind::Text => {
                let context = ProviderValueContext::Field(spec.field);
                let end = StreamEnd::Value(after);
                if matches!(
                    spec.field,
                    ProviderField::CollabSenderThreadId
                        | ProviderField::CollabReceiverThreadId
                        | ProviderField::SubAgentThreadId
                ) {
                    self.capture_mut()?.begin_text(context)?;
                    self.scalar = ScalarHandler::ThreadId {
                        context,
                        end,
                        bytes: FixedBytes::new(),
                    };
                    Ok(())
                } else {
                    self.start_stream(context, end)
                }
            }
            schema::ValueKind::Enum(values) => {
                self.scalar = ScalarHandler::Fixed {
                    purpose: FixedPurpose::Enum {
                        context: ProviderValueContext::Field(spec.field),
                        values,
                    },
                    bytes: FixedBytes::new(),
                    after,
                };
                Ok(())
            }
            schema::ValueKind::DiscardString => {
                self.scalar = ScalarHandler::Discard {
                    reason: DiscardReason::ImageResult,
                    after,
                };
                Ok(())
            }
            _ => Err(ProviderObservationSchemaError::WrongType.into()),
        }
    }

    fn start_schema_number(
        &mut self,
        spec: schema::FieldSpec,
        after: After,
    ) -> Result<(), MachineError> {
        let route = match spec.value {
            schema::ValueKind::Unsigned => RouteValue::Unsigned(spec.field, IntegerWidth::Any),
            schema::ValueKind::Signed => RouteValue::Signed(spec.field, IntegerWidth::Any),
            schema::ValueKind::Signed32 => RouteValue::Signed(spec.field, IntegerWidth::Bits32),
            schema::ValueKind::Unsigned32 => RouteValue::Unsigned(spec.field, IntegerWidth::Bits32),
            _ => return Err(ProviderObservationSchemaError::WrongType.into()),
        };
        self.scalar = ScalarHandler::Number {
            purpose: NumberPurpose::Route(route),
            number: NumberAccumulator::new(),
            after,
        };
        Ok(())
    }

    fn literal(&mut self, value: ProviderScalar) -> Result<(), MachineError> {
        self.prepare_sequence_value()?;
        let expected = self
            .expected
            .take()
            .ok_or(ProviderObservationSchemaError::WrongType)?;
        match expected {
            Expected::Schema(spec, after)
                if value == ProviderScalar::Null
                    && spec.nullable
                    && matches!(spec.value, schema::ValueKind::DiscardString) =>
            {
                self.finish_after(after)
            }
            Expected::Schema(spec, after) if value == ProviderScalar::Null && spec.nullable => {
                self.capture_mut()?
                    .control(ProviderObservationControl::Scalar {
                        context: ProviderValueContext::Field(spec.field),
                        value,
                    })?;
                self.finish_after(after)
            }
            Expected::Schema(spec, after)
                if matches!(spec.value, schema::ValueKind::Boolean)
                    && matches!(value, ProviderScalar::Boolean(_)) =>
            {
                self.capture_mut()?
                    .control(ProviderObservationControl::Scalar {
                        context: ProviderValueContext::Field(spec.field),
                        value,
                    })?;
                self.finish_after(after)
            }
            Expected::Structured { context, after, .. } => {
                self.capture_mut()?
                    .control(ProviderObservationControl::Scalar { context, value })?;
                self.finish_after(after)
            }
            Expected::OtherValue => Ok(()),
            Expected::UserClientId(after)
                if value == ProviderScalar::Null && self.request_scoped_user_message() =>
            {
                self.finish_after(after)
            }
            Expected::UserClientId(_) if self.request_scoped_user_message() => {
                Err(StreamedUserMessageCorrelationError::ClientIdPresent.into())
            }
            Expected::UserClientId(_) => {
                Err(SteeringUserMessageError::MissingOrMalformedCorrelation.into())
            }
            Expected::UserDetail { index, after } if value == ProviderScalar::Null => {
                let expected = self.expected_user_image_detail(index)?;
                if expected.is_some() {
                    return Err(StreamedUserMessageCorrelationError::ImageDetailMismatch {
                        item_index: index,
                    }
                    .into());
                }
                self.finish_after(after)
            }
            _ => Err(ProviderObservationSchemaError::WrongType.into()),
        }
    }

    fn start_container(&mut self, kind: ContainerKind) -> Result<(), MachineError> {
        self.prepare_sequence_value()?;
        let expected = self
            .expected
            .take()
            .ok_or(ProviderObservationSchemaError::WrongType)?;
        match expected {
            Expected::Params(method) if kind == ContainerKind::Object => match method {
                TargetMethod::Lifecycle(lifecycle) => self.push_frame(Frame::LifecycleParams {
                    lifecycle,
                    seen: 0,
                    after: After::None,
                }),
                TargetMethod::Delta(delta) => {
                    let sink = self
                        .sink
                        .take()
                        .ok_or(crate::OrderedTurnStreamSubmitCause::Unavailable)?;
                    self.capture = Some(ObservationCapture::begin(
                        sink,
                        ProviderObservationBegin::Delta { kind: delta },
                    )?);
                    self.push_frame(Frame::DeltaParams {
                        kind: delta,
                        common: 0,
                        payload: 0,
                        after: After::None,
                    })
                }
            },
            Expected::Item(lifecycle, after) if kind == ContainerKind::Object => {
                self.push_frame(Frame::ItemSelect { lifecycle, after })
            }
            Expected::Schema(spec, after) => self.start_schema_container(spec, after, kind),
            Expected::DeltaChanges(after) if kind == ContainerKind::Array => self.start_list(
                ProviderField::DeltaChanges,
                schema::ListKind::Object(schema::ObjectSchema::FileChange),
                after,
            ),
            Expected::Structured {
                root,
                context,
                depth,
                mcp,
                after,
            } => self.start_structured_container(root, context, depth, mcp, after, kind),
            Expected::FixedObject {
                owner,
                schema,
                after,
            } if kind == ContainerKind::Object => self.start_fixed_object(owner, schema, after),
            Expected::OtherValue => self.start_other_container(kind),
            Expected::AgentStates(after) if kind == ContainerKind::Object => {
                let context = ProviderValueContext::Field(ProviderField::CollabAgentStates);
                self.capture_mut()?
                    .control(ProviderObservationControl::BeginContainer {
                        context,
                        container: ProviderContainer::Object,
                    })?;
                self.push_frame(Frame::AgentStates {
                    context,
                    next: 0,
                    after,
                })
            }
            Expected::AgentStateValue { entry } if kind == ContainerKind::Object => {
                let fields = schema::object_fields(schema::ObjectSchema::CollabAgentState)
                    .expect("collab agent state is a fixed schema");
                self.push_frame(Frame::FixedObject {
                    owner: ProviderField::CollabAgentStates,
                    fields,
                    seen: 0,
                    emit_container: false,
                    after: After::ObjectEntry {
                        root: ProviderField::CollabAgentStates,
                        depth: 0,
                        entry,
                    },
                })
            }
            Expected::UserContent(after) if kind == ContainerKind::Array => {
                self.push_frame(Frame::UserContent { next: 0, after })
            }
            Expected::UserInput { index, after } if kind == ContainerKind::Object => self
                .push_frame(Frame::UserInput {
                    index,
                    kind: None,
                    seen: 0,
                    after,
                }),
            Expected::EmptyUserList { index, after } if kind == ContainerKind::Array => self
                .push_frame(Frame::EmptyUserList {
                    item_index: index,
                    after,
                    had_value: false,
                }),
            _ => Err(ProviderObservationSchemaError::WrongType.into()),
        }
    }

    fn start_schema_container(
        &mut self,
        spec: schema::FieldSpec,
        after: After,
        kind: ContainerKind,
    ) -> Result<(), MachineError> {
        match spec.value {
            schema::ValueKind::Structured => self.start_structured_container(
                spec.field,
                ProviderValueContext::Field(spec.field),
                0,
                false,
                after,
                kind,
            ),
            schema::ValueKind::Object(object) if kind == ContainerKind::Object => {
                self.start_fixed_object(spec.field, object, after)
            }
            schema::ValueKind::List(schema::ListKind::DiscardText)
                if kind == ContainerKind::Array =>
            {
                self.push_frame(Frame::DiscardTextList { after })
            }
            schema::ValueKind::List(list) if kind == ContainerKind::Array => {
                self.start_list(spec.field, list, after)
            }
            schema::ValueKind::AgentStates if kind == ContainerKind::Object => {
                self.expected = Some(Expected::AgentStates(after));
                self.start_container(kind)
            }
            _ => Err(ProviderObservationSchemaError::WrongType.into()),
        }
    }

    fn start_list(
        &mut self,
        owner: ProviderField,
        kind: schema::ListKind,
        after: After,
    ) -> Result<(), MachineError> {
        let context = ProviderValueContext::Field(owner);
        self.capture_mut()?
            .control(ProviderObservationControl::BeginContainer {
                context,
                container: ProviderContainer::List,
            })?;
        self.push_frame(Frame::List {
            context,
            kind,
            next: 0,
            after,
        })
    }

    fn start_fixed_object(
        &mut self,
        owner: ProviderField,
        object: schema::ObjectSchema,
        after: After,
    ) -> Result<(), MachineError> {
        let context = ProviderValueContext::Field(owner);
        self.capture_mut()?
            .control(ProviderObservationControl::BeginContainer {
                context,
                container: ProviderContainer::Object,
            })?;
        if let Some(fields) = schema::object_fields(object) {
            self.push_frame(Frame::FixedObject {
                owner,
                fields,
                seen: 0,
                emit_container: true,
                after,
            })
        } else {
            self.push_frame(Frame::DiscriminatedObject {
                schema: object,
                owner,
                variant: None,
                seen: 0,
                after,
            })
        }
    }
}

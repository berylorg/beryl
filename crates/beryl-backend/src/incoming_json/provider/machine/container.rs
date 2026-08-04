impl TargetMachine<'_> {
    fn end_container(&mut self, kind: ContainerKind) -> Result<(), MachineError> {
        if self.expected.is_some() || !matches!(self.scalar, ScalarHandler::None) {
            return Err(ProviderObservationSchemaError::MissingField.into());
        }
        let frame = self.pop_frame();
        match frame {
            Frame::Root { params_seen } if kind == ContainerKind::Object => {
                if !params_seen || self.depth != 0 {
                    return Err(ProviderObservationSchemaError::EnvelopeShape.into());
                }
                self.root_complete = true;
                Ok(())
            }
            Frame::LifecycleParams {
                seen,
                lifecycle: _,
                after,
            } if kind == ContainerKind::Object => {
                if seen != 15 {
                    self.mark_route_failure();
                    return Err(ProviderObservationSchemaError::MissingOrMalformedRoute.into());
                }
                if self.capture.is_some() {
                    let timestamp = self
                        .timestamp
                        .ok_or(ProviderObservationSchemaError::MissingOrMalformedRoute)?;
                    self.capture_mut()?
                        .control(ProviderObservationControl::Scalar {
                            context: ProviderValueContext::Field(
                                ProviderField::LifecycleObservedAt,
                            ),
                            value: ProviderScalar::Unsigned(timestamp),
                        })?;
                }
                self.finish_after(after)
            }
            Frame::DeltaParams {
                kind: delta,
                common,
                payload,
                after,
            } if kind == ContainerKind::Object => {
                let required = required_delta_payload(delta);
                if common != 7 {
                    self.mark_route_failure();
                    return Err(ProviderObservationSchemaError::MissingOrMalformedRoute.into());
                }
                if payload != required {
                    return Err(ProviderObservationSchemaError::MissingField.into());
                }
                self.finish_after(after)
            }
            Frame::ItemProvider {
                fields,
                seen,
                after,
            } if kind == ContainerKind::Object => {
                require_fields(fields, seen)?;
                if self.item_id.is_none() {
                    return Err(ProviderObservationSchemaError::MissingField.into());
                }
                self.finish_after(after)
            }
            Frame::ItemUser { seen, after, .. } if kind == ContainerKind::Object => {
                if seen != 7 || self.item_id.is_none() {
                    return Err(unsupported("required user-message fields").into());
                }
                self.finish_after(after)
            }
            Frame::ItemSelect { .. } if kind == ContainerKind::Object => {
                Err(ProviderObservationSchemaError::MissingField.into())
            }
            Frame::FixedObject {
                owner,
                fields,
                seen,
                emit_container,
                after,
            } if kind == ContainerKind::Object => {
                require_fields(fields, seen)?;
                if emit_container {
                    self.capture_mut()?
                        .control(ProviderObservationControl::EndContainer {
                            context: ProviderValueContext::Field(owner),
                            container: ProviderContainer::Object,
                        })?;
                }
                self.finish_after(after)
            }
            Frame::DiscriminatedObject {
                schema: schema_id,
                owner,
                variant,
                seen,
                after,
            } if kind == ContainerKind::Object => {
                let variant = variant.ok_or(ProviderObservationSchemaError::MissingField)?;
                require_fields(schema::variant_fields(schema_id, variant), seen)?;
                self.capture_mut()?
                    .control(ProviderObservationControl::EndContainer {
                        context: ProviderValueContext::Field(owner),
                        container: ProviderContainer::Object,
                    })?;
                self.finish_after(after)
            }
            Frame::WebOther { owner, after } if kind == ContainerKind::Object => {
                self.capture_mut()?
                    .control(ProviderObservationControl::EndContainer {
                        context: ProviderValueContext::Field(owner),
                        container: ProviderContainer::Object,
                    })?;
                self.finish_after(after)
            }
            Frame::List { context, after, .. } if kind == ContainerKind::Array => {
                self.capture_mut()?
                    .control(ProviderObservationControl::EndContainer {
                        context,
                        container: ProviderContainer::List,
                    })?;
                self.finish_after(after)
            }
            Frame::DiscardTextList { after } if kind == ContainerKind::Array => {
                self.finish_after(after)
            }
            Frame::AgentStates { context, after, .. } if kind == ContainerKind::Object => {
                self.capture_mut()?
                    .control(ProviderObservationControl::EndContainer {
                        context,
                        container: ProviderContainer::Object,
                    })?;
                self.finish_after(after)
            }
            Frame::Structured {
                context,
                container,
                mcp,
                after,
                ..
            } if provider_container(kind) == container => {
                if mcp == McpState::Unsafe {
                    return Err(ProviderObservationSchemaError::AmbiguousSchema.into());
                }
                self.capture_mut()?
                    .control(ProviderObservationControl::EndContainer { context, container })?;
                self.finish_after(after)
            }
            Frame::UserContent { next, after } if kind == ContainerKind::Array => {
                let expected = self.expected_user_item_count()?;
                if next != expected {
                    return Err(StreamedUserMessageCorrelationError::InputCountMismatch {
                        expected,
                        actual: next,
                    }
                    .into());
                }
                self.finish_user_content(next)?;
                self.finish_after(after)
            }
            Frame::UserInput {
                index,
                kind: Some(input_kind),
                seen,
                after,
            } if kind == ContainerKind::Object => {
                if seen != 3 {
                    return Err(unsupported(if matches!(input_kind, UserInputKind::Text) {
                        "required text and text_elements fields"
                    } else {
                        "required localImage detail and path fields"
                    })
                    .into());
                }
                self.finish_user_input(index)?;
                self.finish_after(after)
            }
            Frame::UserInput { .. } if kind == ContainerKind::Object => {
                Err(unsupported("the input type discriminator first").into())
            }
            Frame::EmptyUserList {
                item_index: _,
                after,
                had_value: false,
            } if kind == ContainerKind::Array => self.finish_after(after),
            Frame::EmptyUserList { item_index, .. } if kind == ContainerKind::Array => {
                Err(StreamedUserMessageCorrelationError::TextElementsMismatch { item_index }.into())
            }
            _ => Err(ProviderObservationSchemaError::WrongType.into()),
        }
    }

    fn finish_after(&mut self, after: After) -> Result<(), MachineError> {
        match after {
            After::None => Ok(()),
            After::Element { context, index } => self
                .capture_mut()?
                .control(ProviderObservationControl::EndElement { context, index })
                .map_err(Into::into),
            After::ObjectEntry { root, depth, entry } => self
                .capture_mut()?
                .control(ProviderObservationControl::EndObjectEntry { root, depth, entry })
                .map_err(Into::into),
        }
    }

    fn mark_route_failure(&mut self) {
        if let Some(capture) = self.capture.as_mut() {
            capture.mark_route_failure();
        }
        self.mark_steering_route_failure();
    }

    pub(super) fn mark_transport_lost(&mut self) {
        if let Some(capture) = self.capture.as_mut() {
            capture.mark_transport_lost();
        }
        if let Some(capture) = self.steering_capture.as_mut() {
            capture.mark_transport_lost();
        }
    }
}

impl TargetMachine<'_> {
    fn start_structured_container(
        &mut self,
        root: ProviderField,
        context: ProviderValueContext,
        parent_depth: u8,
        mcp: bool,
        after: After,
        kind: ContainerKind,
    ) -> Result<(), MachineError> {
        let structured_depth = parent_depth
            .checked_add(1)
            .filter(|depth| *depth <= STRUCTURED_DEPTH_LIMIT)
            .ok_or(ProviderObservationSchemaError::StructuredDepthExceeded)?;
        let container = provider_container(kind);
        self.capture_mut()?
            .control(ProviderObservationControl::BeginContainer { context, container })?;
        self.push_frame(Frame::Structured {
            root,
            context,
            container,
            next: 0,
            structured_depth,
            mcp: if mcp && container == ProviderContainer::Object {
                McpState::Unsafe
            } else {
                McpState::None
            },
            after,
        })
    }

    fn prepare_sequence_value(&mut self) -> Result<(), MachineError> {
        if self.expected.is_some() {
            return Ok(());
        }
        match self.top() {
            Frame::List {
                context,
                kind,
                next,
                after,
            } => {
                let index = next;
                let next = next
                    .checked_add(1)
                    .ok_or(ProviderObservationSchemaError::InvalidIndex)?;
                self.set_top(Frame::List {
                    context,
                    kind,
                    next,
                    after,
                });
                self.capture_mut()?
                    .control(ProviderObservationControl::BeginElement { context, index })?;
                let finish = After::Element { context, index };
                self.expected = Some(match kind {
                    schema::ListKind::Text(field) => {
                        Expected::Schema(schema::FieldSpec::required_text(field), finish)
                    }
                    schema::ListKind::Object(object) => Expected::FixedObject {
                        owner: context_field(context),
                        schema: object,
                        after: finish,
                    },
                    schema::ListKind::Structured(root) => Expected::Structured {
                        root,
                        context: ProviderValueContext::Field(root),
                        depth: 0,
                        mcp: root == ProviderField::McpResultContent,
                        after: finish,
                    },
                    schema::ListKind::DiscardText => {
                        Expected::DeltaText(ProviderField::ReasoningSummary, true, finish)
                    }
                });
            }
            Frame::DiscardTextList { .. } => {
                self.expected = Some(Expected::DeltaText(
                    ProviderField::ReasoningSummary,
                    true,
                    After::None,
                ));
            }
            Frame::Structured {
                root,
                context,
                container: ProviderContainer::List,
                next,
                structured_depth,
                mcp,
                after,
            } => {
                let index = next;
                let next = next
                    .checked_add(1)
                    .ok_or(ProviderObservationSchemaError::InvalidIndex)?;
                self.set_top(Frame::Structured {
                    root,
                    context,
                    container: ProviderContainer::List,
                    next,
                    structured_depth,
                    mcp,
                    after,
                });
                self.capture_mut()?
                    .control(ProviderObservationControl::BeginElement { context, index })?;
                self.expected = Some(Expected::Structured {
                    root,
                    context: ProviderValueContext::Structured {
                        root,
                        depth: structured_depth,
                        position: ProviderStructuredPosition::ListElement { index },
                    },
                    depth: structured_depth,
                    mcp: false,
                    after: After::Element { context, index },
                });
            }
            Frame::UserContent { next, after } => {
                let expected = self.expected_user_item_count()?;
                if next >= expected {
                    return Err(StreamedUserMessageCorrelationError::InputCountMismatch {
                        expected,
                        actual: next.saturating_add(1),
                    }
                    .into());
                }
                let index = next;
                let next = next.checked_add(1).ok_or(
                    StreamedUserMessageCorrelationError::InputCountMismatch {
                        expected,
                        actual: u64::MAX,
                    },
                )?;
                self.set_top(Frame::UserContent { next, after });
                self.expected = Some(Expected::UserInput {
                    index,
                    after: After::None,
                });
            }
            Frame::EmptyUserList { item_index, .. } => {
                return Err(StreamedUserMessageCorrelationError::TextElementsMismatch {
                    item_index,
                }
                .into());
            }
            _ => {}
        }
        Ok(())
    }
}

fn provider_container(kind: ContainerKind) -> ProviderContainer {
    match kind {
        ContainerKind::Object => ProviderContainer::Object,
        ContainerKind::Array => ProviderContainer::List,
    }
}

fn context_field(context: ProviderValueContext) -> ProviderField {
    match context {
        ProviderValueContext::Field(field) => field,
        ProviderValueContext::Structured { root, .. } => root,
    }
}

impl TargetMachine<'_> {
    fn end_scalar(&mut self, kind: ScalarKind) -> Result<(), MachineError> {
        let handler = std::mem::replace(&mut self.scalar, ScalarHandler::None);
        match (kind, handler) {
            (
                ScalarKind::Name | ScalarKind::String,
                ScalarHandler::Fixed {
                    purpose,
                    bytes,
                    after,
                },
            ) => self.finish_fixed(purpose, bytes, after),
            (ScalarKind::String, ScalarHandler::Stream { context, end })
            | (ScalarKind::Name, ScalarHandler::Stream { context, end }) => {
                self.capture_mut()?.end_text(context)?;
                self.finish_stream_end(end)
            }
            (
                ScalarKind::String,
                ScalarHandler::ThreadId {
                    context,
                    end,
                    bytes,
                },
            ) => {
                CasThreadId::new(bytes.as_str()?)
                    .map_err(|_| ProviderObservationSchemaError::InvalidIdentity)?;
                self.capture_mut()?.end_text(context)?;
                self.finish_stream_end(end)
            }
            (
                ScalarKind::Number,
                ScalarHandler::Number {
                    purpose,
                    number,
                    after,
                },
            ) => {
                let value = number.finish()?;
                match purpose {
                    NumberPurpose::Route(route) => self.finish_route_number(route, value)?,
                    NumberPurpose::Structured(context) => self
                        .capture_mut()?
                        .control(ProviderObservationControl::Scalar { context, value })?,
                }
                self.finish_after(after)
            }
            (
                ScalarKind::Name | ScalarKind::String | ScalarKind::Number,
                ScalarHandler::Discard { after, .. },
            ) => self.finish_after(after),
            (ScalarKind::String, ScalarHandler::WebAction(probe)) => {
                self.finish_web_action(probe, After::None)
            }
            (ScalarKind::Name, ScalarHandler::OtherName(probe)) => self.finish_other_name(probe),
            (
                ScalarKind::String,
                ScalarHandler::UserText { index, after },
            ) => {
                self.finish_user_text(index)?;
                self.finish_after(after)
            }
            (
                ScalarKind::String,
                ScalarHandler::UserPath { index, after },
            ) => {
                self.finish_user_image_path(index)?;
                self.finish_after(after)
            }
            (
                ScalarKind::Name,
                ScalarHandler::McpKey {
                    root,
                    depth,
                    entry,
                    bytes,
                    streaming,
                },
            ) => self.finish_mcp_key(root, depth, entry, bytes, streaming),
            (
                ScalarKind::String,
                ScalarHandler::McpType {
                    root,
                    depth,
                    entry,
                    bytes,
                    streaming,
                },
            ) => self.finish_mcp_type(root, depth, entry, bytes, streaming),
            _ => Err(ProviderObservationSchemaError::WrongType.into()),
        }
    }

    fn finish_fixed(
        &mut self,
        purpose: FixedPurpose,
        bytes: FixedBytes,
        after: After,
    ) -> Result<(), MachineError> {
        let value = bytes.as_str()?;
        match purpose {
            FixedPurpose::Name => self.finish_name(value),
            FixedPurpose::ItemType(lifecycle) => self.finish_item_type(lifecycle, value),
            FixedPurpose::Enum { context, values } => {
                let selected = values
                    .iter()
                    .find_map(|(wire, selected)| (*wire == value).then_some(*selected))
                    .ok_or(ProviderObservationSchemaError::UnknownOrLateVariant)?;
                if matches!(self.top(), Frame::DiscriminatedObject { variant: None, .. }) {
                    self.finish_discriminant(context, selected, after)
                } else {
                    let begin = self.capture_mut()?.begin_kind();
                    validate_lifecycle_status(begin, context, selected)?;
                    self.capture_mut()?
                        .control(ProviderObservationControl::Enum {
                            context,
                            value: selected,
                        })?;
                    self.finish_after(after)
                }
            }
            FixedPurpose::Identity(route) => {
                match route {
                    RouteValue::ThreadId => {
                        self.thread_id = Some(
                            CasThreadId::new(value)
                                .map_err(|_| ProviderObservationSchemaError::InvalidIdentity)?,
                        );
                    }
                    RouteValue::TurnId => {
                        self.turn_id = Some(
                            CasTurnId::new(value)
                                .map_err(|_| ProviderObservationSchemaError::InvalidIdentity)?,
                        );
                    }
                    RouteValue::ItemId => {
                        if self.capture.is_some() {
                            let context = ProviderValueContext::Field(ProviderField::ItemId);
                            self.capture_mut()?.end_text(context)?;
                        }
                        self.item_id = Some(
                            CasItemId::new(value)
                                .map_err(|_| ProviderObservationSchemaError::InvalidIdentity)?,
                        );
                    }
                    _ => return Err(ProviderObservationSchemaError::WrongType.into()),
                }
                self.finish_after(after)
            }
            FixedPurpose::UserClientId => self.begin_steering_user_message(value),
            FixedPurpose::UserType { index } => self.finish_user_type(index, value),
            FixedPurpose::UserDetail { index } => {
                let actual = match value {
                    "auto" => Some(ImageDetail::Auto),
                    "low" => Some(ImageDetail::Low),
                    "high" => Some(ImageDetail::High),
                    "original" => Some(ImageDetail::Original),
                    _ => return Err(unsupported("the pinned image detail").into()),
                };
                let expected = self.expected_user_image_detail(index)?;
                if actual != expected {
                    return Err(StreamedUserMessageCorrelationError::ImageDetailMismatch {
                        item_index: index,
                    }
                    .into());
                }
                self.finish_after(after)
            }
        }
    }

    fn finish_item_type(
        &mut self,
        lifecycle: ProviderItemLifecycle,
        value: &str,
    ) -> Result<(), MachineError> {
        let Frame::ItemSelect { after, .. } = self.top() else {
            return Err(ProviderObservationSchemaError::AmbiguousSchema.into());
        };
        if value == "userMessage" {
            let lifecycle = user_lifecycle(lifecycle);
            if self.request_scoped_user_message() {
                self.verifier_ref()?.begin_lifecycle(lifecycle)?;
            }
            self.set_top(Frame::ItemUser {
                lifecycle,
                seen: 0,
                after,
            });
            return Ok(());
        }
        let kind = item_kind(value).ok_or(ProviderObservationSchemaError::UnknownOrLateVariant)?;
        let sink = self
            .sink
            .take()
            .ok_or(crate::OrderedTurnStreamSubmitCause::Unavailable)?;
        self.capture = Some(ObservationCapture::begin(
            sink,
            ProviderObservationBegin::Item { lifecycle, kind },
        )?);
        self.set_top(Frame::ItemProvider {
            fields: schema::item_fields(kind),
            seen: 0,
            after,
        });
        Ok(())
    }

    fn finish_user_type(&mut self, index: u64, value: &str) -> Result<(), MachineError> {
        let expected = self.begin_user_input(index)?;
        if value != expected {
            return Err(StreamedUserMessageCorrelationError::InputVariantMismatch {
                item_index: index,
                expected,
                actual: known_input_type(value),
            }
            .into());
        }
        let Frame::UserInput {
            index, seen, after, ..
        } = self.top()
        else {
            return Err(ProviderObservationSchemaError::AmbiguousSchema.into());
        };
        self.set_top(Frame::UserInput {
            index,
            kind: Some(if expected == "text" {
                UserInputKind::Text
            } else {
                UserInputKind::LocalImage
            }),
            seen,
            after,
        });
        Ok(())
    }
}

fn validate_lifecycle_status(
    begin: ProviderObservationBegin,
    context: ProviderValueContext,
    value: ProviderEnumValue,
) -> Result<(), ProviderObservationSchemaError> {
    if value != ProviderEnumValue::InProgress {
        return Ok(());
    }
    let invalid = matches!(
        (begin, context),
        (
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Completed,
                kind: ProviderItemKind::CommandExecution,
            },
            ProviderValueContext::Field(ProviderField::CommandStatus),
        ) | (
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Completed,
                kind: ProviderItemKind::FileChange,
            },
            ProviderValueContext::Field(ProviderField::FileChangeStatus),
        ) | (
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Completed,
                kind: ProviderItemKind::McpToolCall,
            },
            ProviderValueContext::Field(ProviderField::McpStatus),
        ) | (
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Completed,
                kind: ProviderItemKind::DynamicToolCall,
            },
            ProviderValueContext::Field(ProviderField::DynamicStatus),
        ) | (
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Completed,
                kind: ProviderItemKind::CollabAgentToolCall,
            },
            ProviderValueContext::Field(ProviderField::CollabStatus),
        ) | (
            ProviderObservationBegin::Item {
                lifecycle: ProviderItemLifecycle::Completed,
                kind: ProviderItemKind::StandaloneImageGeneration,
            },
            ProviderValueContext::Field(ProviderField::ImageGenerationStatus),
        )
    );
    if invalid {
        Err(ProviderObservationSchemaError::InvalidLifecycle)
    } else {
        Ok(())
    }
}

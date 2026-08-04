impl TargetMachine<'_> {
    fn start_scalar(&mut self, kind: ScalarKind) -> Result<(), MachineError> {
        if !matches!(self.scalar, ScalarHandler::None) {
            return Err(ProviderObservationSchemaError::AmbiguousSchema.into());
        }
        if kind == ScalarKind::Name {
            return self.start_name();
        }
        self.prepare_sequence_value()?;
        let expected = self
            .expected
            .take()
            .ok_or(ProviderObservationSchemaError::EnvelopeShape)?;
        match (kind, expected) {
            (ScalarKind::String, Expected::Schema(spec, after)) => {
                self.start_schema_string(spec, after)
            }
            (ScalarKind::Number, Expected::Schema(spec, after)) => {
                self.start_schema_number(spec, after)
            }
            (ScalarKind::String, Expected::Route(value, after)) => {
                if matches!(
                    value,
                    RouteValue::Timestamp | RouteValue::Unsigned(_, _) | RouteValue::Signed(_, _)
                ) {
                    return Err(ProviderObservationSchemaError::WrongType.into());
                }
                if matches!(value, RouteValue::ItemId) && self.capture.is_some() {
                    let context = ProviderValueContext::Field(ProviderField::ItemId);
                    self.capture_mut()?.begin_text(context)?;
                }
                self.scalar = ScalarHandler::Fixed {
                    purpose: FixedPurpose::Identity(value),
                    bytes: FixedBytes::new(),
                    after,
                };
                Ok(())
            }
            (ScalarKind::Number, Expected::Route(value, after)) => {
                self.scalar = ScalarHandler::Number {
                    purpose: NumberPurpose::Route(value),
                    number: NumberAccumulator::new(),
                    after,
                };
                Ok(())
            }
            (ScalarKind::String, Expected::DeltaText(field, discard, after)) => {
                if discard {
                    self.scalar = ScalarHandler::Discard {
                        reason: DiscardReason::ReasoningText,
                        after,
                    };
                    return Ok(());
                }
                self.start_stream(ProviderValueContext::Field(field), StreamEnd::Value(after))
            }
            (ScalarKind::String, Expected::ItemType(lifecycle)) => {
                self.scalar = ScalarHandler::Fixed {
                    purpose: FixedPurpose::ItemType(lifecycle),
                    bytes: FixedBytes::new(),
                    after: After::None,
                };
                Ok(())
            }
            (ScalarKind::String, Expected::Discriminant { schema, owner }) => {
                if schema == schema::ObjectSchema::WebSearchAction {
                    self.scalar = ScalarHandler::WebAction(WebActionProbe::new());
                    let _ = owner;
                    return Ok(());
                }
                let (field, values) = schema::discriminant(schema);
                self.scalar = ScalarHandler::Fixed {
                    purpose: FixedPurpose::Enum {
                        context: ProviderValueContext::Field(field),
                        values,
                    },
                    bytes: FixedBytes::new(),
                    after: After::None,
                };
                let _ = (schema, owner);
                Ok(())
            }
            (ScalarKind::String | ScalarKind::Number, Expected::OtherValue) => {
                self.start_other_scalar()
            }
            (ScalarKind::String, Expected::Structured { context, after, .. }) => {
                self.start_stream(context, StreamEnd::Value(after))
            }
            (ScalarKind::Number, Expected::Structured { context, after, .. }) => {
                self.scalar = ScalarHandler::Number {
                    purpose: NumberPurpose::Structured(context),
                    number: NumberAccumulator::new(),
                    after,
                };
                Ok(())
            }
            (ScalarKind::String, Expected::UserType { index }) => {
                self.scalar = ScalarHandler::Fixed {
                    purpose: FixedPurpose::UserType { index },
                    bytes: FixedBytes::new(),
                    after: After::None,
                };
                Ok(())
            }
            (ScalarKind::String, Expected::UserText { index, after }) => {
                self.scalar = ScalarHandler::UserText { index, after };
                Ok(())
            }
            (ScalarKind::String, Expected::UserPath { index, after }) => {
                self.scalar = ScalarHandler::UserPath { index, after };
                Ok(())
            }
            (ScalarKind::String, Expected::UserDetail { index, after }) => {
                self.scalar = ScalarHandler::Fixed {
                    purpose: FixedPurpose::UserDetail { index },
                    bytes: FixedBytes::new(),
                    after,
                };
                Ok(())
            }
            (ScalarKind::String, Expected::UserClientId(after))
                if !self.request_scoped_user_message() =>
            {
                self.scalar = ScalarHandler::Fixed {
                    purpose: FixedPurpose::UserClientId,
                    bytes: FixedBytes::new(),
                    after,
                };
                Ok(())
            }
            (_, Expected::UserClientId(_)) if self.request_scoped_user_message() => {
                Err(StreamedUserMessageCorrelationError::ClientIdPresent.into())
            }
            (_, Expected::UserClientId(_)) => {
                Err(SteeringUserMessageError::MissingOrMalformedCorrelation.into())
            }
            (ScalarKind::String, Expected::McpType { root, depth, entry }) => {
                self.scalar = ScalarHandler::McpType {
                    root,
                    depth,
                    entry,
                    bytes: FixedBytes::new(),
                    streaming: false,
                };
                Ok(())
            }
            _ => Err(ProviderObservationSchemaError::WrongType.into()),
        }
    }

    fn start_name(&mut self) -> Result<(), MachineError> {
        if self.expected.is_some() {
            return Err(ProviderObservationSchemaError::WrongType.into());
        }
        match self.top() {
            Frame::WebOther { .. } => {
                self.scalar = ScalarHandler::OtherName(OtherNameProbe::new());
                Ok(())
            }
            Frame::AgentStates {
                context,
                next,
                after,
            } => {
                let entry = next;
                let next = next
                    .checked_add(1)
                    .ok_or(ProviderObservationSchemaError::InvalidIndex)?;
                self.set_top(Frame::AgentStates {
                    context,
                    next,
                    after,
                });
                self.capture_mut()?
                    .control(ProviderObservationControl::BeginObjectEntry {
                        root: ProviderField::CollabAgentStates,
                        depth: 0,
                        entry,
                    })?;
                let key_context = ProviderValueContext::Structured {
                    root: ProviderField::CollabAgentStates,
                    depth: 0,
                    position: ProviderStructuredPosition::ObjectKey { entry },
                };
                self.start_stream(key_context, StreamEnd::AgentStateKey { entry })
            }
            Frame::Structured {
                root,
                context,
                container: ProviderContainer::Object,
                next,
                structured_depth,
                mcp,
                after,
            } => {
                let entry = next;
                let next = next
                    .checked_add(1)
                    .ok_or(ProviderObservationSchemaError::InvalidIndex)?;
                self.set_top(Frame::Structured {
                    root,
                    context,
                    container: ProviderContainer::Object,
                    next,
                    structured_depth,
                    mcp,
                    after,
                });
                self.capture_mut()?
                    .control(ProviderObservationControl::BeginObjectEntry {
                        root,
                        depth: structured_depth,
                        entry,
                    })?;
                if mcp != McpState::None {
                    self.scalar = ScalarHandler::McpKey {
                        root,
                        depth: structured_depth,
                        entry,
                        bytes: FixedBytes::new(),
                        streaming: false,
                    };
                    Ok(())
                } else {
                    let key_context = ProviderValueContext::Structured {
                        root,
                        depth: structured_depth,
                        position: ProviderStructuredPosition::ObjectKey { entry },
                    };
                    self.start_stream(
                        key_context,
                        StreamEnd::StructuredKey {
                            root,
                            depth: structured_depth,
                            entry,
                        },
                    )
                }
            }
            Frame::Structured { .. } => Err(ProviderObservationSchemaError::WrongType.into()),
            _ => {
                self.scalar = ScalarHandler::Fixed {
                    purpose: FixedPurpose::Name,
                    bytes: FixedBytes::new(),
                    after: After::None,
                };
                Ok(())
            }
        }
    }

    fn start_stream(
        &mut self,
        context: ProviderValueContext,
        end: StreamEnd,
    ) -> Result<(), MachineError> {
        self.capture_mut()?.begin_text(context)?;
        self.scalar = ScalarHandler::Stream { context, end };
        Ok(())
    }

    fn verifier_ref(
        &self,
    ) -> Result<crate::turn::StreamedUserMessageVerifierGuard<'_>, MachineError>
    {
        let verifier = self
            .verifier
            .as_ref()
            .ok_or(StreamedUserMessageCorrelationError::VerifierUnavailable)?;
        verifier.lock().map_err(Into::into)
    }
}

impl TargetMachine<'_> {
    fn finish_stream_end(&mut self, end: StreamEnd) -> Result<(), MachineError> {
        match end {
            StreamEnd::Value(after) => self.finish_after(after),
            StreamEnd::StructuredKey { root, depth, entry } => {
                self.expected = Some(Expected::Structured {
                    root,
                    context: ProviderValueContext::Structured {
                        root,
                        depth,
                        position: ProviderStructuredPosition::ObjectValue { entry },
                    },
                    depth,
                    mcp: false,
                    after: After::ObjectEntry { root, depth, entry },
                });
                Ok(())
            }
            StreamEnd::AgentStateKey { entry } => {
                self.expected = Some(Expected::AgentStateValue { entry });
                Ok(())
            }
        }
    }

    fn mcp_key_bytes(
        &mut self,
        root: ProviderField,
        depth: u8,
        entry: u64,
        mut fixed: FixedBytes,
        mut streaming: bool,
        bytes: &[u8],
    ) -> Result<ScalarHandler, MachineError> {
        let context = structured_key_context(root, depth, entry);
        if streaming {
            self.capture_mut()?.write_fixed(bytes)?;
        } else {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| ProviderObservationSchemaError::InvalidString)?;
            for (offset, character) in text.char_indices() {
                let end = offset + character.len_utf8();
                fixed.push(&bytes[offset..end])?;
                if !mcp_key_has_special_prefix(fixed.as_str()?) {
                    self.capture_mut()?.begin_text(context)?;
                    self.capture_mut()?
                        .write_fixed(fixed.as_str()?.as_bytes())?;
                    self.capture_mut()?.write_fixed(&bytes[end..])?;
                    streaming = true;
                    break;
                }
            }
        }
        Ok(ScalarHandler::McpKey {
            root,
            depth,
            entry,
            bytes: fixed,
            streaming,
        })
    }

    fn finish_mcp_key(
        &mut self,
        root: ProviderField,
        depth: u8,
        entry: u64,
        fixed: FixedBytes,
        streaming: bool,
    ) -> Result<(), MachineError> {
        let context = structured_key_context(root, depth, entry);
        let mcp_state = match self.top() {
            Frame::Structured { mcp, .. } => mcp,
            _ => return Err(ProviderObservationSchemaError::AmbiguousSchema.into()),
        };
        if streaming {
            self.capture_mut()?.end_text(context)?;
            return self.finish_stream_end(StreamEnd::StructuredKey { root, depth, entry });
        }
        match fixed.as_str()? {
            "type" if mcp_state == McpState::Unsafe => {
                self.expected = Some(Expected::McpType { root, depth, entry });
                Ok(())
            }
            "type" => Err(ProviderObservationSchemaError::DuplicateField.into()),
            "data" | "image_url" | "imageUrl" if mcp_state == McpState::Unsafe => {
                Err(ProviderObservationSchemaError::InlineImageRequiresAsset.into())
            }
            key => {
                self.capture_mut()?.begin_text(context)?;
                self.capture_mut()?.write_fixed(key.as_bytes())?;
                self.capture_mut()?.end_text(context)?;
                self.finish_stream_end(StreamEnd::StructuredKey { root, depth, entry })
            }
        }
    }

    fn mcp_type_bytes(
        &mut self,
        root: ProviderField,
        depth: u8,
        entry: u64,
        mut fixed: FixedBytes,
        mut streaming: bool,
        bytes: &[u8],
    ) -> Result<ScalarHandler, MachineError> {
        let context = structured_value_context(root, depth, entry);
        if streaming {
            self.capture_mut()?.write_fixed(bytes)?;
        } else {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| ProviderObservationSchemaError::InvalidString)?;
            for (offset, character) in text.char_indices() {
                let end = offset + character.len_utf8();
                fixed.push(&bytes[offset..end])?;
                if !b"image".starts_with(fixed.as_str()?.as_bytes()) {
                    self.mark_mcp_safe()?;
                    self.emit_fixed_text(structured_key_context(root, depth, entry), b"type")?;
                    self.capture_mut()?.begin_text(context)?;
                    self.capture_mut()?
                        .write_fixed(fixed.as_str()?.as_bytes())?;
                    self.capture_mut()?.write_fixed(&bytes[end..])?;
                    streaming = true;
                    break;
                }
            }
        }
        Ok(ScalarHandler::McpType {
            root,
            depth,
            entry,
            bytes: fixed,
            streaming,
        })
    }

    fn finish_mcp_type(
        &mut self,
        root: ProviderField,
        depth: u8,
        entry: u64,
        fixed: FixedBytes,
        streaming: bool,
    ) -> Result<(), MachineError> {
        let context = structured_value_context(root, depth, entry);
        if streaming {
            self.capture_mut()?.end_text(context)?;
            return self.finish_after(After::ObjectEntry { root, depth, entry });
        }
        if fixed.as_str()? == "image" {
            return Err(ProviderObservationSchemaError::InlineImageRequiresAsset.into());
        }
        self.mark_mcp_safe()?;
        self.emit_fixed_text(structured_key_context(root, depth, entry), b"type")?;
        self.capture_mut()?.begin_text(context)?;
        self.capture_mut()?
            .write_fixed(fixed.as_str()?.as_bytes())?;
        self.capture_mut()?.end_text(context)?;
        self.finish_after(After::ObjectEntry { root, depth, entry })
    }

    fn mark_mcp_safe(&mut self) -> Result<(), MachineError> {
        let Frame::Structured {
            root,
            context,
            container,
            next,
            structured_depth,
            mcp: McpState::Unsafe,
            after,
        } = self.top()
        else {
            return Err(ProviderObservationSchemaError::DuplicateField.into());
        };
        self.set_top(Frame::Structured {
            root,
            context,
            container,
            next,
            structured_depth,
            mcp: McpState::Safe,
            after,
        });
        Ok(())
    }

    fn emit_fixed_text(
        &mut self,
        context: ProviderValueContext,
        bytes: &[u8],
    ) -> Result<(), MachineError> {
        self.capture_mut()?.begin_text(context)?;
        self.capture_mut()?.write_fixed(bytes)?;
        self.capture_mut()?.end_text(context)?;
        Ok(())
    }
}

fn structured_key_context(root: ProviderField, depth: u8, entry: u64) -> ProviderValueContext {
    ProviderValueContext::Structured {
        root,
        depth,
        position: ProviderStructuredPosition::ObjectKey { entry },
    }
}

fn structured_value_context(root: ProviderField, depth: u8, entry: u64) -> ProviderValueContext {
    ProviderValueContext::Structured {
        root,
        depth,
        position: ProviderStructuredPosition::ObjectValue { entry },
    }
}

fn mcp_key_has_special_prefix(value: &str) -> bool {
    ["type", "data", "image_url", "imageUrl"]
        .iter()
        .any(|candidate| candidate.starts_with(value))
}

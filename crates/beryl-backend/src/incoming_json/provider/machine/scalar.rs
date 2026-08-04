impl TargetMachine<'_> {
    fn finish_name(&mut self, name: &str) -> Result<(), MachineError> {
        match self.top() {
            Frame::Root { params_seen } => {
                if params_seen || name != "params" {
                    return Err(ProviderObservationSchemaError::EnvelopeShape.into());
                }
                self.set_top(Frame::Root { params_seen: true });
                self.expected = Some(Expected::Params(self.method));
            }
            Frame::LifecycleParams {
                lifecycle,
                mut seen,
                after,
            } => {
                if seen == 0 && name != "item" {
                    return Err(ProviderObservationSchemaError::EnvelopeShape.into());
                }
                let expected = match name {
                    "item" if seen & 8 == 0 => {
                        seen |= 8;
                        Expected::Item(lifecycle, After::None)
                    }
                    "threadId" if seen & 1 == 0 => {
                        seen |= 1;
                        Expected::Route(RouteValue::ThreadId, After::None)
                    }
                    "turnId" if seen & 2 == 0 => {
                        seen |= 2;
                        Expected::Route(RouteValue::TurnId, After::None)
                    }
                    "startedAtMs"
                        if lifecycle == ProviderItemLifecycle::Started && seen & 4 == 0 =>
                    {
                        seen |= 4;
                        Expected::Route(RouteValue::Timestamp, After::None)
                    }
                    "completedAtMs"
                        if lifecycle == ProviderItemLifecycle::Completed && seen & 4 == 0 =>
                    {
                        seen |= 4;
                        Expected::Route(RouteValue::Timestamp, After::None)
                    }
                    _ => return Err(ProviderObservationSchemaError::UnknownField.into()),
                };
                self.set_top(Frame::LifecycleParams {
                    lifecycle,
                    seen,
                    after,
                });
                self.expected = Some(expected);
            }
            Frame::DeltaParams {
                kind,
                mut common,
                mut payload,
                after,
            } => {
                let expected = match name {
                    "threadId" if common & 1 == 0 => {
                        common |= 1;
                        Expected::Route(RouteValue::ThreadId, After::None)
                    }
                    "turnId" if common & 2 == 0 => {
                        common |= 2;
                        Expected::Route(RouteValue::TurnId, After::None)
                    }
                    "itemId" if common & 4 == 0 => {
                        common |= 4;
                        Expected::Route(RouteValue::ItemId, After::None)
                    }
                    "delta" if delta_has_text(kind) && payload & 1 == 0 => {
                        payload |= 1;
                        Expected::DeltaText(
                            ProviderField::DeltaText,
                            kind == ProviderDeltaKind::ReasoningTextObserved,
                            After::None,
                        )
                    }
                    "summaryIndex"
                        if matches!(
                            kind,
                            ProviderDeltaKind::ReasoningSummaryPartAdded
                                | ProviderDeltaKind::ReasoningSummaryText
                        ) && payload & 2 == 0 =>
                    {
                        payload |= 2;
                        Expected::Route(
                            RouteValue::Unsigned(
                                ProviderField::DeltaSummaryIndex,
                                IntegerWidth::Any,
                            ),
                            After::None,
                        )
                    }
                    "contentIndex"
                        if kind == ProviderDeltaKind::ReasoningTextObserved && payload & 2 == 0 =>
                    {
                        payload |= 2;
                        Expected::Route(
                            RouteValue::Unsigned(
                                ProviderField::DeltaContentIndex,
                                IntegerWidth::Any,
                            ),
                            After::None,
                        )
                    }
                    "changes"
                        if kind == ProviderDeltaKind::FileChangePatchUpdated
                            && payload & 4 == 0 =>
                    {
                        payload |= 4;
                        Expected::DeltaChanges(After::None)
                    }
                    "message"
                        if kind == ProviderDeltaKind::McpToolCallProgress && payload & 8 == 0 =>
                    {
                        payload |= 8;
                        Expected::DeltaText(ProviderField::McpProgressMessage, false, After::None)
                    }
                    _ => return Err(ProviderObservationSchemaError::UnknownField.into()),
                };
                self.set_top(Frame::DeltaParams {
                    kind,
                    common,
                    payload,
                    after,
                });
                self.expected = Some(expected);
            }
            Frame::ItemSelect { lifecycle, .. } => {
                if name != "type" {
                    return Err(ProviderObservationSchemaError::UnknownOrLateVariant.into());
                }
                self.expected = Some(Expected::ItemType(lifecycle));
            }
            Frame::ItemProvider {
                fields,
                mut seen,
                after,
            } => {
                let (index, spec) = find_field(fields, name)?;
                let bit = 1_u64 << index;
                if seen & bit != 0 {
                    return Err(ProviderObservationSchemaError::DuplicateField.into());
                }
                seen |= bit;
                self.set_top(Frame::ItemProvider {
                    fields,
                    seen,
                    after,
                });
                self.expected = Some(Expected::Schema(spec, After::None));
            }
            Frame::ItemUser {
                lifecycle,
                mut seen,
                after,
            } => {
                let expected = if self.request_scoped_user_message() {
                    match name {
                        "id" if seen & 1 == 0 => {
                            seen |= 1;
                            Expected::Route(RouteValue::ItemId, After::None)
                        }
                        "clientId" if seen & 2 == 0 => {
                            seen |= 2;
                            Expected::UserClientId(After::None)
                        }
                        "content" if seen & 4 == 0 => {
                            seen |= 4;
                            Expected::UserContent(After::None)
                        }
                        _ => {
                            return Err(
                                unsupported("the pinned user-message item field set").into()
                            );
                        }
                    }
                } else {
                    match (seen, name) {
                        (0, "id") => {
                            seen = 1;
                            Expected::Route(RouteValue::ItemId, After::None)
                        }
                        (1, "clientId") => {
                            seen = 3;
                            Expected::UserClientId(After::None)
                        }
                        (3, "content") => {
                            seen = 7;
                            Expected::UserContent(After::None)
                        }
                        _ => {
                            return Err(
                                SteeringUserMessageError::MissingOrMalformedCorrelation.into()
                            );
                        }
                    }
                };
                self.set_top(Frame::ItemUser {
                    lifecycle,
                    seen,
                    after,
                });
                self.expected = Some(expected);
            }
            Frame::FixedObject {
                owner,
                fields,
                mut seen,
                emit_container,
                after,
            } => {
                let (index, spec) = find_field(fields, name)?;
                let bit = 1_u64 << index;
                if seen & bit != 0 {
                    return Err(ProviderObservationSchemaError::DuplicateField.into());
                }
                seen |= bit;
                self.set_top(Frame::FixedObject {
                    owner,
                    fields,
                    seen,
                    emit_container,
                    after,
                });
                self.expected = Some(Expected::Schema(spec, After::None));
            }
            Frame::DiscriminatedObject {
                schema: schema_id,
                owner,
                variant,
                mut seen,
                after,
            } => {
                if let Some(variant) = variant {
                    let fields = schema::variant_fields(schema_id, variant);
                    let (index, spec) = find_field(fields, name)?;
                    let bit = 1_u64 << index;
                    if seen & bit != 0 {
                        return Err(ProviderObservationSchemaError::DuplicateField.into());
                    }
                    seen |= bit;
                    self.set_top(Frame::DiscriminatedObject {
                        schema: schema_id,
                        owner,
                        variant: Some(variant),
                        seen,
                        after,
                    });
                    self.expected = Some(Expected::Schema(spec, After::None));
                } else {
                    if name != "type" {
                        return Err(ProviderObservationSchemaError::UnknownOrLateVariant.into());
                    }
                    self.expected = Some(Expected::Discriminant {
                        schema: schema_id,
                        owner,
                    });
                }
            }
            Frame::UserInput {
                index,
                kind,
                mut seen,
                after,
            } => {
                let expected = match kind {
                    None if name == "type" => Expected::UserType { index },
                    None => return Err(unsupported("the input type discriminator first").into()),
                    Some(UserInputKind::Text) => match name {
                        "text" if seen & 1 == 0 => {
                            seen |= 1;
                            Expected::UserText {
                                index,
                                after: After::None,
                            }
                        }
                        "text_elements" if seen & 2 == 0 => {
                            seen |= 2;
                            Expected::EmptyUserList {
                                index,
                                after: After::None,
                            }
                        }
                        _ => return Err(unsupported("the pinned text input field set").into()),
                    },
                    Some(UserInputKind::LocalImage) => match name {
                        "detail" if seen & 1 == 0 => {
                            seen |= 1;
                            Expected::UserDetail {
                                index,
                                after: After::None,
                            }
                        }
                        "path" if seen & 2 == 0 => {
                            seen |= 2;
                            Expected::UserPath {
                                index,
                                after: After::None,
                            }
                        }
                        _ => {
                            return Err(unsupported("the pinned localImage input field set").into());
                        }
                    },
                };
                self.set_top(Frame::UserInput {
                    index,
                    kind,
                    seen,
                    after,
                });
                self.expected = Some(expected);
            }
            _ => return Err(ProviderObservationSchemaError::WrongType.into()),
        }
        Ok(())
    }
}

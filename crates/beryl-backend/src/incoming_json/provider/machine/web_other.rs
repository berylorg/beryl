#[derive(Clone, Copy)]
struct WebActionProbe {
    candidates: u8,
    length: usize,
}

impl WebActionProbe {
    const SEARCH: u8 = 1;
    const OPEN_PAGE: u8 = 2;
    const FIND_IN_PAGE: u8 = 4;
    const ALL: u8 = Self::SEARCH | Self::OPEN_PAGE | Self::FIND_IN_PAGE;

    const fn new() -> Self {
        Self {
            candidates: Self::ALL,
            length: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        const WIRES: [&[u8]; 3] = [b"search", b"openPage", b"findInPage"];
        for byte in bytes {
            for (index, wire) in WIRES.iter().enumerate() {
                let bit = 1_u8 << index;
                if self.candidates & bit != 0 && wire.get(self.length) != Some(byte) {
                    self.candidates &= !bit;
                }
            }
            self.length = self.length.saturating_add(1);
        }
    }

    fn finish(self) -> Result<ProviderEnumValue, ProviderObservationSchemaError> {
        if self.length == 0 {
            return Err(ProviderObservationSchemaError::UnknownOrLateVariant);
        }
        let known = [
            (
                Self::SEARCH,
                b"search".as_slice(),
                ProviderEnumValue::Search,
            ),
            (
                Self::OPEN_PAGE,
                b"openPage".as_slice(),
                ProviderEnumValue::OpenPage,
            ),
            (
                Self::FIND_IN_PAGE,
                b"findInPage".as_slice(),
                ProviderEnumValue::FindInPage,
            ),
        ];
        Ok(known
            .into_iter()
            .find_map(|(bit, wire, value)| {
                (self.candidates & bit != 0 && self.length == wire.len()).then_some(value)
            })
            .unwrap_or(ProviderEnumValue::Other))
    }
}

#[derive(Clone, Copy)]
struct OtherNameProbe {
    matches_type: bool,
    length: usize,
}

impl OtherNameProbe {
    const fn new() -> Self {
        Self {
            matches_type: true,
            length: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for byte in bytes {
            if self.matches_type && b"type".get(self.length) != Some(byte) {
                self.matches_type = false;
            }
            self.length = self.length.saturating_add(1);
        }
    }

    const fn is_type(self) -> bool {
        self.matches_type && self.length == 4
    }
}

impl TargetMachine<'_> {
    fn finish_web_action(
        &mut self,
        probe: WebActionProbe,
        after: After,
    ) -> Result<(), MachineError> {
        let selected = probe.finish()?;
        self.finish_discriminant(
            ProviderValueContext::Field(ProviderField::WebSearchActionKind),
            selected,
            after,
        )
    }

    fn finish_discriminant(
        &mut self,
        context: ProviderValueContext,
        selected: ProviderEnumValue,
        after: After,
    ) -> Result<(), MachineError> {
        let Frame::DiscriminatedObject {
            schema,
            owner,
            variant: None,
            seen,
            after: container_after,
        } = self.top()
        else {
            return Err(ProviderObservationSchemaError::AmbiguousSchema.into());
        };
        if selected == ProviderEnumValue::Other {
            if schema != schema::ObjectSchema::WebSearchAction {
                return Err(ProviderObservationSchemaError::UnknownOrLateVariant.into());
            }
            self.set_top(Frame::WebOther {
                owner,
                after: container_after,
            });
        } else {
            if schema == schema::ObjectSchema::DynamicContent
                && selected == ProviderEnumValue::InputImage
            {
                return Err(ProviderObservationSchemaError::InlineImageRequiresAsset.into());
            }
            self.set_top(Frame::DiscriminatedObject {
                schema,
                owner,
                variant: Some(selected),
                seen,
                after: container_after,
            });
        }
        self.capture_mut()?
            .control(ProviderObservationControl::Enum {
                context,
                value: selected,
            })?;
        self.finish_after(after)
    }

    fn finish_other_name(&mut self, probe: OtherNameProbe) -> Result<(), MachineError> {
        if probe.is_type() {
            return Err(ProviderObservationSchemaError::DuplicateField.into());
        }
        self.expected = Some(Expected::OtherValue);
        Ok(())
    }

    fn start_other_scalar(&mut self) -> Result<(), MachineError> {
        self.scalar = ScalarHandler::Discard {
            reason: DiscardReason::OtherPayload,
            after: After::None,
        };
        Ok(())
    }

    fn start_other_container(&mut self, kind: ContainerKind) -> Result<(), MachineError> {
        self.push_frame(Frame::OtherDiscard {
            container: kind,
            structured_depth: 1,
        })
    }

    fn other_discard_event(&mut self, event: Event) -> Result<(), MachineError> {
        let Frame::OtherDiscard {
            container: _,
            structured_depth,
        } = self.top()
        else {
            return Err(ProviderObservationSchemaError::AmbiguousSchema.into());
        };
        match event {
            Event::ContainerStart(container) => {
                let structured_depth = structured_depth
                    .checked_add(1)
                    .filter(|depth| *depth <= STRUCTURED_DEPTH_LIMIT)
                    .ok_or(ProviderObservationSchemaError::StructuredDepthExceeded)?;
                self.push_frame(Frame::OtherDiscard {
                    container,
                    structured_depth,
                })
            }
            Event::ContainerEnd(actual) => {
                if !matches!(self.scalar, ScalarHandler::None) {
                    return Err(ProviderObservationSchemaError::WrongType.into());
                }
                let Frame::OtherDiscard { container, .. } = self.pop_frame() else {
                    unreachable!("discard frame checked before pop")
                };
                if actual != container {
                    return Err(ProviderObservationSchemaError::WrongType.into());
                }
                Ok(())
            }
            Event::ScalarStart(_) if matches!(self.scalar, ScalarHandler::None) => {
                self.start_other_scalar()
            }
            Event::ScalarFragment(kind) => self.scalar_fragment(kind),
            Event::ScalarEnd(kind) => {
                let handler = std::mem::replace(&mut self.scalar, ScalarHandler::None);
                if matches!(
                    (kind, handler),
                    (
                        ScalarKind::Name | ScalarKind::String | ScalarKind::Number,
                        ScalarHandler::Discard {
                            reason: DiscardReason::OtherPayload,
                            ..
                        }
                    )
                ) {
                    Ok(())
                } else {
                    Err(ProviderObservationSchemaError::WrongType.into())
                }
            }
            Event::Boolean(_) | Event::Null if matches!(self.scalar, ScalarHandler::None) => Ok(()),
            _ => Err(ProviderObservationSchemaError::WrongType.into()),
        }
    }
}

impl<'a> NormalTerminalMachine<'a> {
    fn expected_event(&mut self, event: Event) -> Result<(), MachineError> {
        match self.expected {
            Expected::RootParamsName => {
                self.start_name(event, b"params", Expected::ParamsObject)
            }
            Expected::ParamsObject => {
                self.start_object(event, Expected::ParamsThreadName)
            }
            Expected::ParamsThreadName => {
                self.start_name(event, b"threadId", Expected::ThreadValue)
            }
            Expected::ThreadValue => self.start_identity(
                event,
                IdentityKind::Thread,
                Expected::ParamsTurnName,
            ),
            Expected::ParamsTurnName => {
                self.start_name(event, b"turn", Expected::TurnObject)
            }
            Expected::TurnObject => self.start_object(event, Expected::TurnIdName),
            Expected::TurnIdName => {
                self.start_name(event, b"id", Expected::TurnIdValue)
            }
            Expected::TurnIdValue => {
                self.start_identity(event, IdentityKind::Turn, Expected::TurnItemsName)
            }
            Expected::TurnItemsName => {
                self.start_name(event, b"items", Expected::TurnItemsArray)
            }
            Expected::TurnItemsArray => {
                self.start_array(event, Expected::TurnItemsEnd)
            }
            Expected::TurnItemsEnd => {
                self.end_array(event, Expected::TurnItemsViewName)
            }
            Expected::TurnItemsViewName => self.start_name(
                event,
                b"itemsView",
                Expected::TurnItemsViewValue,
            ),
            Expected::TurnItemsViewValue => self.start_choice(
                event,
                ChoiceKind::ItemsView,
                Expected::TurnStatusName,
            ),
            Expected::TurnStatusName => {
                self.start_name(event, b"status", Expected::TurnStatusValue)
            }
            Expected::TurnStatusValue => self.start_choice(
                event,
                ChoiceKind::Status,
                Expected::TurnErrorName,
            ),
            Expected::TurnErrorName => {
                self.start_name(event, b"error", Expected::TurnErrorValue)
            }
            Expected::TurnErrorValue => self.start_error(event),
            Expected::ErrorMessageName => {
                self.start_name(event, b"message", Expected::ErrorMessageValue)
            }
            Expected::ErrorMessageValue => self.start_diagnostic(
                event,
                NormalTurnTerminalDiagnosticField::Message,
                Expected::ErrorCodexInfoName,
            ),
            Expected::ErrorCodexInfoName => self.start_name(
                event,
                b"codexErrorInfo",
                Expected::ErrorCodexInfoValue,
            ),
            Expected::ErrorCodexInfoValue => self.start_codex_info(event),
            Expected::CodexObjectName => self.start_choice(
                event,
                ChoiceKind::CodexObject,
                Expected::CodexPayloadObject,
            ),
            Expected::CodexPayloadObject => {
                self.start_object(event, Expected::CodexPayloadName)
            }
            Expected::CodexPayloadName => {
                let expected = match self.pending_codex_object {
                    Some(CodexObjectVariant::ActiveTurnNotSteerable) => b"turnKind".as_slice(),
                    Some(_) => b"httpStatusCode".as_slice(),
                    None => return Err(malformed()),
                };
                self.start_name(event, expected, Expected::CodexPayloadValue)
            }
            Expected::CodexPayloadValue => self.start_codex_payload(event),
            Expected::CodexPayloadEnd => {
                self.end_object(event, Expected::CodexObjectEnd)
            }
            Expected::CodexObjectEnd => {
                self.end_object(event, Expected::ErrorAdditionalDetailsName)
            }
            Expected::ErrorAdditionalDetailsName => self.start_name(
                event,
                b"additionalDetails",
                Expected::ErrorAdditionalDetailsValue,
            ),
            Expected::ErrorAdditionalDetailsValue => match event {
                Event::Null => {
                    self.expected = Expected::ErrorEnd;
                    Ok(())
                }
                _ => self.start_diagnostic(
                    event,
                    NormalTurnTerminalDiagnosticField::AdditionalDetails,
                    Expected::ErrorEnd,
                ),
            },
            Expected::ErrorEnd => self.end_object(event, Expected::StartedAtName),
            Expected::StartedAtName => {
                self.start_name(event, b"startedAt", Expected::StartedAtValue)
            }
            Expected::StartedAtValue => {
                self.start_optional_i64(event, Expected::CompletedAtName)
            }
            Expected::CompletedAtName => {
                self.start_name(event, b"completedAt", Expected::CompletedAtValue)
            }
            Expected::CompletedAtValue => {
                self.start_optional_i64(event, Expected::DurationMsName)
            }
            Expected::DurationMsName => {
                self.start_name(event, b"durationMs", Expected::DurationMsValue)
            }
            Expected::DurationMsValue => self.start_optional_i64(event, Expected::TurnEnd),
            Expected::TurnEnd => self.end_object(event, Expected::ParamsEnd),
            Expected::ParamsEnd => self.end_object(event, Expected::RootEnd),
            Expected::RootEnd => self.end_object(event, Expected::Done),
            Expected::Done => Err(malformed()),
        }
    }

    fn start_name(
        &mut self,
        event: Event,
        expected: &'static [u8],
        next: Expected,
    ) -> Result<(), MachineError> {
        if event != Event::ScalarStart(ScalarKind::Name) {
            return Err(malformed());
        }
        self.scalar = TerminalScalar::Name {
            probe: ExactProbe::new(),
            expected,
            next,
        };
        Ok(())
    }

    fn start_identity(
        &mut self,
        event: Event,
        kind: IdentityKind,
        next: Expected,
    ) -> Result<(), MachineError> {
        if event != Event::ScalarStart(ScalarKind::String) {
            return Err(malformed());
        }
        self.scalar = TerminalScalar::Identity {
            bytes: IdentityBytes::new(),
            kind,
            next,
        };
        Ok(())
    }

    fn start_choice(
        &mut self,
        event: Event,
        kind: ChoiceKind,
        next: Expected,
    ) -> Result<(), MachineError> {
        if event != Event::ScalarStart(ScalarKind::String)
            && !(matches!(kind, ChoiceKind::CodexObject)
                && event == Event::ScalarStart(ScalarKind::Name))
        {
            return Err(malformed());
        }
        self.scalar = TerminalScalar::Choice {
            probe: ChoiceProbe::new(choice_wires(kind)),
            kind,
            next,
        };
        Ok(())
    }

    fn start_diagnostic(
        &mut self,
        event: Event,
        field: NormalTurnTerminalDiagnosticField,
        next: Expected,
    ) -> Result<(), MachineError> {
        if event != Event::ScalarStart(ScalarKind::String) || !self.diagnostic.begin(field) {
            return Err(malformed());
        }
        self.scalar = TerminalScalar::Diagnostic { field, next };
        Ok(())
    }

    fn start_optional_i64(
        &mut self,
        event: Event,
        next: Expected,
    ) -> Result<(), MachineError> {
        match event {
            Event::Null => {
                self.expected = next;
                Ok(())
            }
            Event::ScalarStart(ScalarKind::Number) => {
                self.scalar = TerminalScalar::Integer {
                    accumulator: IntegerAccumulator::new(),
                    kind: IntegerKind::DiscardSigned,
                    next,
                };
                Ok(())
            }
            _ => Err(malformed()),
        }
    }

    fn start_error(&mut self, event: Event) -> Result<(), MachineError> {
        match (self.status, event) {
            (
                Some(NormalTurnTerminalStatus::Completed | NormalTurnTerminalStatus::Interrupted),
                Event::Null,
            ) => {
                self.expected = Expected::StartedAtName;
                Ok(())
            }
            (Some(NormalTurnTerminalStatus::Failed), Event::ContainerStart(ContainerKind::Object)) => {
                self.expected = Expected::ErrorMessageName;
                Ok(())
            }
            _ => Err(malformed()),
        }
    }

    fn start_codex_info(&mut self, event: Event) -> Result<(), MachineError> {
        match event {
            Event::Null => {
                self.expected = Expected::ErrorAdditionalDetailsName;
                Ok(())
            }
            Event::ScalarStart(ScalarKind::String) => self.start_choice(
                event,
                ChoiceKind::CodexUnit,
                Expected::ErrorAdditionalDetailsName,
            ),
            Event::ContainerStart(ContainerKind::Object) => {
                self.expected = Expected::CodexObjectName;
                Ok(())
            }
            _ => Err(malformed()),
        }
    }

    fn start_codex_payload(&mut self, event: Event) -> Result<(), MachineError> {
        match self.pending_codex_object {
            Some(CodexObjectVariant::ActiveTurnNotSteerable) => self.start_choice(
                event,
                ChoiceKind::TurnKind,
                Expected::CodexPayloadEnd,
            ),
            Some(_) => match event {
                Event::Null => {
                    self.finish_http_status(None)?;
                    self.expected = Expected::CodexPayloadEnd;
                    Ok(())
                }
                Event::ScalarStart(ScalarKind::Number) => {
                    self.scalar = TerminalScalar::Integer {
                        accumulator: IntegerAccumulator::new(),
                        kind: IntegerKind::HttpStatus,
                        next: Expected::CodexPayloadEnd,
                    };
                    Ok(())
                }
                _ => Err(malformed()),
            },
            None => Err(malformed()),
        }
    }

    fn start_object(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::ContainerStart(ContainerKind::Object) {
            return Err(malformed());
        }
        self.expected = next;
        Ok(())
    }

    fn end_object(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::ContainerEnd(ContainerKind::Object) {
            return Err(malformed());
        }
        self.expected = next;
        Ok(())
    }

    fn start_array(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::ContainerStart(ContainerKind::Array) {
            return Err(malformed());
        }
        self.expected = next;
        Ok(())
    }

    fn end_array(&mut self, event: Event, next: Expected) -> Result<(), MachineError> {
        if event != Event::ContainerEnd(ContainerKind::Array) {
            return Err(malformed());
        }
        self.expected = next;
        Ok(())
    }
}

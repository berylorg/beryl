include!("dynamic_tool/state.rs");

impl<'a> DynamicToolMachine<'a> {
    const fn new(
        sink: &'a mut dyn OrderedTurnStreamSink,
        response_authority_generation: u64,
    ) -> Self {
        Self {
            sink: Some(sink),
            capture: None,
            response_authority_generation,
            location: DynamicLocation::Root,
            expected: None,
            scalar: DynamicScalar::None,
            root_seen: 0,
            params_seen: 0,
            params_order: 0,
            argument_depth: 0,
            argument_scalar_root: false,
            argument_complete: false,
            request_id: None,
            thread_id: None,
            turn_id: None,
            call_id: None,
            namespace: None,
            tool: None,
            root_complete: false,
        }
    }

    const fn uses_capture_output(&self) -> bool {
        matches!(self.scalar, DynamicScalar::Argument(_))
    }

    fn capture_output_window(&mut self) -> Result<&mut [u8], MachineError> {
        Ok(self.capture_mut()?.output_window()?)
    }

    fn commit_capture_output(&mut self, produced: usize) -> Result<(), MachineError> {
        Ok(self.capture_mut()?.commit_output(produced)?)
    }

    fn flush_full_page(&mut self) -> Result<(), MachineError> {
        if self.uses_capture_output() {
            self.capture_mut()?.flush_if_full()?;
        }
        Ok(())
    }

    fn flush_capture_output(&mut self) -> Result<(), MachineError> {
        if self.uses_capture_output() {
            self.capture_mut()?.flush_nonempty()?;
        }
        Ok(())
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        match &mut self.scalar {
            DynamicScalar::Name(probe) => {
                probe.push(bytes);
                Ok(())
            }
            DynamicScalar::RequestString(fixed)
            | DynamicScalar::RequestNumber(fixed)
            | DynamicScalar::Identity(_, fixed) => Ok(fixed.push(bytes)?),
            DynamicScalar::Argument(_) if bytes.is_empty() => Ok(()),
            DynamicScalar::Argument(_) => Err(DynamicToolCallSchemaError::EnvelopeShape.into()),
            DynamicScalar::None if bytes.is_empty() => Ok(()),
            DynamicScalar::None => Err(DynamicToolCallSchemaError::EnvelopeShape.into()),
        }
    }

    fn event(&mut self, event: Event) -> Result<(), MachineError> {
        match event {
            Event::ContainerStart(kind) => self.start_container(kind),
            Event::ContainerEnd(kind) => self.end_container(kind),
            Event::ScalarStart(kind) => self.start_scalar(kind),
            Event::ScalarFragment(kind) => self.scalar_fragment(kind),
            Event::ScalarEnd(kind) => self.end_scalar(kind),
            Event::Boolean(value) => self.literal(Some(value)),
            Event::Null => self.literal(None),
        }
    }

    fn start_scalar(&mut self, kind: ScalarKind) -> Result<(), MachineError> {
        if !matches!(self.scalar, DynamicScalar::None) {
            return Err(DynamicToolCallSchemaError::EnvelopeShape.into());
        }
        if self.location == DynamicLocation::Arguments {
            let kind = dynamic_scalar_kind(kind)?;
            self.capture_mut()?.begin_scalar(kind)?;
            self.scalar = DynamicScalar::Argument(kind);
            return Ok(());
        }
        if kind == ScalarKind::Name {
            if self.expected.is_some() {
                return Err(DynamicToolCallSchemaError::WrongType.into());
            }
            self.scalar = DynamicScalar::Name(DynamicNameProbe::new(self.location));
            return Ok(());
        }
        let expected = self
            .expected
            .take()
            .ok_or(DynamicToolCallSchemaError::EnvelopeShape)?;
        self.scalar = match (kind, expected) {
            (ScalarKind::String, DynamicExpected::RequestId) => {
                DynamicScalar::RequestString(DynamicFixedBytes::new())
            }
            (ScalarKind::Number, DynamicExpected::RequestId) => {
                DynamicScalar::RequestNumber(DynamicFixedBytes::new())
            }
            (ScalarKind::String, DynamicExpected::Identity(field)) => {
                DynamicScalar::Identity(field, DynamicFixedBytes::new())
            }
            (ScalarKind::String | ScalarKind::Number, DynamicExpected::Arguments) => {
                let kind = dynamic_scalar_kind(kind)?;
                self.capture_mut()?.begin_scalar(kind)?;
                self.argument_scalar_root = true;
                self.location = DynamicLocation::Arguments;
                DynamicScalar::Argument(kind)
            }
            _ => return Err(DynamicToolCallSchemaError::WrongType.into()),
        };
        Ok(())
    }

    fn scalar_fragment(&self, kind: ScalarKind) -> Result<(), MachineError> {
        let valid = matches!(
            (&self.scalar, kind),
            (DynamicScalar::Name(_), ScalarKind::Name)
                | (DynamicScalar::RequestString(_), ScalarKind::String)
                | (DynamicScalar::RequestNumber(_), ScalarKind::Number)
                | (DynamicScalar::Identity(_, _), ScalarKind::String)
                | (DynamicScalar::Argument(DynamicToolArgumentScalarKind::ObjectName), ScalarKind::Name)
                | (DynamicScalar::Argument(DynamicToolArgumentScalarKind::String), ScalarKind::String)
                | (DynamicScalar::Argument(DynamicToolArgumentScalarKind::Number), ScalarKind::Number)
        );
        valid
            .then_some(())
            .ok_or_else(|| DynamicToolCallSchemaError::WrongType.into())
    }

    fn end_scalar(&mut self, kind: ScalarKind) -> Result<(), MachineError> {
        let scalar = std::mem::replace(&mut self.scalar, DynamicScalar::None);
        match (kind, scalar) {
            (ScalarKind::Name, DynamicScalar::Name(probe)) => self.finish_name(probe.finish()),
            (ScalarKind::String, DynamicScalar::RequestString(bytes)) => {
                let value = bytes.as_str()?;
                if value.is_empty() {
                    return Err(DynamicToolCallSchemaError::InvalidRequestIdentity.into());
                }
                self.request_id = Some(DynamicToolCallRequestId::String(value.into()));
                Ok(())
            }
            (ScalarKind::Number, DynamicScalar::RequestNumber(bytes)) => {
                self.request_id = Some(parse_dynamic_request_number(bytes.as_str()?)?);
                Ok(())
            }
            (ScalarKind::String, DynamicScalar::Identity(field, bytes)) => {
                self.finish_identity(field, bytes.as_str()?)
            }
            (_, DynamicScalar::Argument(argument_kind))
                if dynamic_scalar_kind(kind)? == argument_kind =>
            {
                self.capture_mut()?.end_scalar(argument_kind)?;
                if self.argument_scalar_root {
                    self.argument_scalar_root = false;
                    self.argument_complete = true;
                    self.location = DynamicLocation::Params;
                }
                Ok(())
            }
            _ => Err(DynamicToolCallSchemaError::WrongType.into()),
        }
    }

    fn start_container(&mut self, kind: ContainerKind) -> Result<(), MachineError> {
        if !matches!(self.scalar, DynamicScalar::None) {
            return Err(DynamicToolCallSchemaError::WrongType.into());
        }
        if self.location == DynamicLocation::Arguments {
            return self.start_argument_container(kind);
        }
        let expected = self
            .expected
            .take()
            .ok_or(DynamicToolCallSchemaError::EnvelopeShape)?;
        match (expected, kind) {
            (DynamicExpected::Params, ContainerKind::Object) => {
                self.location = DynamicLocation::Params;
                Ok(())
            }
            (DynamicExpected::Arguments, _) => {
                self.location = DynamicLocation::Arguments;
                self.start_argument_container(kind)
            }
            _ => Err(DynamicToolCallSchemaError::WrongType.into()),
        }
    }

    fn start_argument_container(&mut self, kind: ContainerKind) -> Result<(), MachineError> {
        self.argument_depth = self
            .argument_depth
            .checked_add(1)
            .filter(|depth| *depth <= STRUCTURED_DEPTH_LIMIT)
            .ok_or(DynamicToolCallSchemaError::StructuredDepthExceeded)?;
        self.capture_mut()?.control(
            DynamicToolArgumentControl::ContainerStart(dynamic_container(kind)),
        )?;
        Ok(())
    }

    fn end_container(&mut self, kind: ContainerKind) -> Result<(), MachineError> {
        if self.expected.is_some() || !matches!(self.scalar, DynamicScalar::None) {
            return Err(DynamicToolCallSchemaError::WrongType.into());
        }
        if self.location == DynamicLocation::Arguments {
            if self.argument_depth == 0 {
                return Err(DynamicToolCallSchemaError::EnvelopeShape.into());
            }
            self.capture_mut()?.control(
                DynamicToolArgumentControl::ContainerEnd(dynamic_container(kind)),
            )?;
            self.argument_depth -= 1;
            if self.argument_depth == 0 {
                self.argument_complete = true;
                self.location = DynamicLocation::Params;
            }
            return Ok(());
        }
        match (self.location, kind) {
            (DynamicLocation::Params, ContainerKind::Object) => {
                if !self.argument_complete || self.params_order != DYNAMIC_ARGUMENTS {
                    return Err(DynamicToolCallSchemaError::MissingField.into());
                }
                self.location = DynamicLocation::Root;
                Ok(())
            }
            (DynamicLocation::Root, ContainerKind::Object) => {
                if self.root_seen != (DYNAMIC_ROOT_ID | DYNAMIC_ROOT_PARAMS) {
                    return Err(DynamicToolCallSchemaError::MissingField.into());
                }
                self.root_complete = true;
                Ok(())
            }
            _ => Err(DynamicToolCallSchemaError::EnvelopeShape.into()),
        }
    }

    fn literal(&mut self, value: Option<bool>) -> Result<(), MachineError> {
        if self.location == DynamicLocation::Arguments {
            self.capture_mut()?.control(match value {
                Some(value) => DynamicToolArgumentControl::Boolean(value),
                None => DynamicToolArgumentControl::Null,
            })?;
            return Ok(());
        }
        match self.expected.take() {
            Some(DynamicExpected::Arguments) => {
                self.capture_mut()?.control(match value {
                    Some(value) => DynamicToolArgumentControl::Boolean(value),
                    None => DynamicToolArgumentControl::Null,
                })?;
                self.argument_complete = true;
                Ok(())
            }
            _ => Err(DynamicToolCallSchemaError::WrongType.into()),
        }
    }

    fn finish_name(&mut self, name: Option<DynamicName>) -> Result<(), MachineError> {
        match self.location {
            DynamicLocation::Root => match name {
                Some(DynamicName::Id) if self.root_seen == 0 => {
                    self.root_seen = DYNAMIC_ROOT_ID;
                    self.expected = Some(DynamicExpected::RequestId);
                    Ok(())
                }
                Some(DynamicName::Params) if self.root_seen == DYNAMIC_ROOT_ID => {
                    self.root_seen |= DYNAMIC_ROOT_PARAMS;
                    self.expected = Some(DynamicExpected::Params);
                    Ok(())
                }
                Some(DynamicName::Id | DynamicName::Params | DynamicName::Method | DynamicName::JsonRpc) => {
                    Err(DynamicToolCallSchemaError::DuplicateField.into())
                }
                _ => Err(DynamicToolCallSchemaError::EnvelopeShape.into()),
            },
            DynamicLocation::Params => self.finish_params_name(name),
            DynamicLocation::Arguments => {
                Err(DynamicToolCallSchemaError::EnvelopeShape.into())
            }
        }
    }

    fn finish_params_name(&mut self, name: Option<DynamicName>) -> Result<(), MachineError> {
        let (bit, expected_order, next_order, expected) = match name {
            Some(DynamicName::ThreadId) => (
                DYNAMIC_THREAD,
                0,
                DYNAMIC_THREAD,
                DynamicExpected::Identity(DynamicIdentityField::Thread),
            ),
            Some(DynamicName::TurnId) => (
                DYNAMIC_TURN,
                DYNAMIC_THREAD,
                DYNAMIC_TURN,
                DynamicExpected::Identity(DynamicIdentityField::Turn),
            ),
            Some(DynamicName::CallId) => (
                DYNAMIC_CALL,
                DYNAMIC_TURN,
                DYNAMIC_CALL,
                DynamicExpected::Identity(DynamicIdentityField::Call),
            ),
            Some(DynamicName::Namespace) => (
                DYNAMIC_NAMESPACE,
                DYNAMIC_CALL,
                DYNAMIC_NAMESPACE,
                DynamicExpected::Identity(DynamicIdentityField::Namespace),
            ),
            Some(DynamicName::Tool) if matches!(self.params_order, DYNAMIC_CALL | DYNAMIC_NAMESPACE) => (
                DYNAMIC_TOOL,
                self.params_order,
                DYNAMIC_TOOL,
                DynamicExpected::Identity(DynamicIdentityField::Tool),
            ),
            Some(DynamicName::Arguments) => (
                DYNAMIC_ARGUMENTS,
                DYNAMIC_TOOL,
                DYNAMIC_ARGUMENTS,
                DynamicExpected::Arguments,
            ),
            Some(DynamicName::Tool) => return self.reject_duplicate_or_reordered(name),
            _ => return Err(DynamicToolCallSchemaError::EnvelopeShape.into()),
        };
        if self.params_seen & bit != 0 {
            return Err(DynamicToolCallSchemaError::DuplicateField.into());
        }
        if self.params_order != expected_order {
            return Err(DynamicToolCallSchemaError::ReorderedField.into());
        }
        self.params_seen |= bit;
        self.params_order = next_order;
        if matches!(expected, DynamicExpected::Arguments) {
            self.begin_capture()?;
        }
        self.expected = Some(expected);
        Ok(())
    }

    fn reject_duplicate_or_reordered(
        &self,
        name: Option<DynamicName>,
    ) -> Result<(), MachineError> {
        let bit = match name {
            Some(DynamicName::ThreadId) => DYNAMIC_THREAD,
            Some(DynamicName::TurnId) => DYNAMIC_TURN,
            Some(DynamicName::CallId) => DYNAMIC_CALL,
            Some(DynamicName::Namespace) => DYNAMIC_NAMESPACE,
            Some(DynamicName::Tool) => DYNAMIC_TOOL,
            Some(DynamicName::Arguments) => DYNAMIC_ARGUMENTS,
            _ => return Err(DynamicToolCallSchemaError::EnvelopeShape.into()),
        };
        if self.params_seen & bit != 0 {
            Err(DynamicToolCallSchemaError::DuplicateField.into())
        } else {
            Err(DynamicToolCallSchemaError::ReorderedField.into())
        }
    }

    fn finish_identity(
        &mut self,
        field: DynamicIdentityField,
        value: &str,
    ) -> Result<(), MachineError> {
        if value.is_empty() {
            return Err(DynamicToolCallSchemaError::InvalidIdentity.into());
        }
        match field {
            DynamicIdentityField::Thread => {
                self.thread_id = Some(
                    CasThreadId::new(value)
                        .map_err(|_| DynamicToolCallSchemaError::InvalidIdentity)?,
                );
            }
            DynamicIdentityField::Turn => {
                self.turn_id = Some(
                    CasTurnId::new(value)
                        .map_err(|_| DynamicToolCallSchemaError::InvalidIdentity)?,
                );
            }
            DynamicIdentityField::Call => {
                self.call_id = Some(
                    DynamicToolCallId::new(value)
                        .map_err(|_| DynamicToolCallSchemaError::InvalidIdentity)?,
                );
            }
            DynamicIdentityField::Namespace => {
                if value.len() > DYNAMIC_TOOL_NAMESPACE_MAX_BYTES {
                    return Err(DynamicToolCallSchemaError::IdentityTooLong.into());
                }
                self.namespace = Some(value.into());
            }
            DynamicIdentityField::Tool => {
                self.tool = Some(
                    DynamicToolName::new(value)
                        .map_err(|_| DynamicToolCallSchemaError::InvalidIdentity)?,
                );
            }
        }
        Ok(())
    }

    fn begin_capture(&mut self) -> Result<(), MachineError> {
        let call = DynamicToolCall::decoded(
            self.request_id
                .take()
                .ok_or(DynamicToolCallSchemaError::MissingField)?,
            self.thread_id
                .take()
                .ok_or(DynamicToolCallSchemaError::MissingField)?,
            self.turn_id
                .take()
                .ok_or(DynamicToolCallSchemaError::MissingField)?,
            self.call_id
                .take()
                .ok_or(DynamicToolCallSchemaError::MissingField)?,
            self.namespace.take(),
            self.tool
                .take()
                .ok_or(DynamicToolCallSchemaError::MissingField)?,
            self.response_authority_generation,
        );
        let sink = self
            .sink
            .take()
            .ok_or(DynamicToolCallSchemaError::OrderedSinkUnbound)?;
        self.capture = Some(DynamicToolCapture::begin(sink, call.0, call.1)?);
        Ok(())
    }

    fn capture_mut(&mut self) -> Result<&mut DynamicToolCapture<'a>, MachineError> {
        self.capture
            .as_mut()
            .ok_or_else(|| DynamicToolCallSchemaError::ReorderedField.into())
    }

    fn mark_transport_lost(&mut self) {
        if let Some(capture) = self.capture.as_mut() {
            capture.mark_transport_lost();
        }
    }

    fn map_parse_failure(&self, failure: ParseFailure) -> DecodeReaderError {
        use bounded_json::ErrorKind;
        let source = match failure.error().kind() {
            ErrorKind::DepthExceeded => DynamicToolCallSchemaError::StructuredDepthExceeded,
            ErrorKind::InvalidUtf8 | ErrorKind::InvalidEscape | ErrorKind::UnpairedSurrogate => {
                DynamicToolCallSchemaError::InvalidIdentity
            }
            ErrorKind::UnexpectedByte
            | ErrorKind::InvalidNumber
            | ErrorKind::IncompleteDocument
            | ErrorKind::TrailingContent
            | ErrorKind::PositionOverflow
            | ErrorKind::InvalidApiUse => DynamicToolCallSchemaError::EnvelopeShape,
        };
        DecodeReaderError::DynamicTool(source.into())
    }

    fn finish(&mut self) -> Result<DecodedIncoming, MachineError> {
        if !self.root_complete
            || self.location != DynamicLocation::Root
            || self.expected.is_some()
            || !matches!(self.scalar, DynamicScalar::None)
            || !self.argument_complete
            || self.argument_depth != 0
        {
            return Err(DynamicToolCallSchemaError::EnvelopeShape.into());
        }
        let capture = self
            .capture
            .take()
            .ok_or(DynamicToolCallSchemaError::MissingField)?;
        capture.seal()?;
        Ok(DecodedIncoming::OrderedHandled)
    }
}

include!("dynamic_tool/helpers.rs");

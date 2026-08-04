const APPROVAL_ROOT_ID: u8 = 1;
const APPROVAL_ROOT_PARAMS: u8 = 2;
const APPROVAL_ROUTE_THREAD: u8 = 1;
const APPROVAL_ROUTE_TURN: u8 = 2;
const APPROVAL_ROUTE_ITEM: u8 = 4;

struct ApprovalMachine {
    kind: ApprovalRequestKind,
    location: ApprovalLocation,
    expected: Option<ApprovalExpected>,
    scalar: ApprovalScalar,
    discard_depth: u8,
    root_seen: u8,
    route_seen: u8,
    route_order: u8,
    payload_started: bool,
    request_id: Option<ApprovalRequestId>,
    thread_id: Option<CasThreadId>,
    turn_id: Option<CasTurnId>,
    item_id: Option<CasItemId>,
    root_complete: bool,
}

#[derive(Clone, Copy)]
enum ApprovalLocation {
    Root,
    Params,
}

#[derive(Clone, Copy)]
enum ApprovalExpected {
    RequestId,
    Params,
    Route(ApprovalRouteField),
    Discard,
}

enum ApprovalScalar {
    None,
    Name(ApprovalNameProbe),
    RequestString(ApprovalFixedBytes),
    RequestNumber(ApprovalFixedBytes),
    Route(ApprovalRouteField, ApprovalFixedBytes),
    Discard,
}

#[derive(Clone, Copy)]
enum ApprovalRouteField {
    Thread,
    Turn,
    Item,
}

#[derive(Clone, Copy)]
enum ApprovalName {
    Id,
    Params,
    Method,
    JsonRpc,
    ThreadId,
    TurnId,
    ItemId,
}

struct ApprovalFixedBytes {
    bytes: [u8; APPROVAL_REQUEST_ID_MAX_BYTES],
    len: usize,
}

impl ApprovalFixedBytes {
    const fn new() -> Self {
        Self {
            bytes: [0; APPROVAL_REQUEST_ID_MAX_BYTES],
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Result<(), ApprovalRequestSchemaError> {
        let end = self
            .len
            .checked_add(bytes.len())
            .filter(|end| *end <= self.bytes.len())
            .ok_or(ApprovalRequestSchemaError::IdentityTooLong)?;
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }

    fn as_str(&self) -> Result<&str, ApprovalRequestSchemaError> {
        std::str::from_utf8(&self.bytes[..self.len])
            .map_err(|_| ApprovalRequestSchemaError::InvalidRequestIdentity)
    }
}

struct ApprovalNameProbe {
    candidates: u8,
    len: usize,
}

impl ApprovalNameProbe {
    const fn new(location: ApprovalLocation) -> Self {
        Self {
            candidates: match location {
                ApprovalLocation::Root => 0b0000_1111,
                ApprovalLocation::Params => 0b0111_0000,
            },
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for byte in bytes {
            for (index, wire) in APPROVAL_NAMES.iter().enumerate() {
                let bit = 1_u8 << index;
                if self.candidates & bit != 0 && wire.get(self.len) != Some(byte) {
                    self.candidates &= !bit;
                }
            }
            self.len = self.len.saturating_add(1);
        }
    }

    fn finish(self) -> Option<ApprovalName> {
        APPROVAL_NAMES
            .iter()
            .enumerate()
            .find_map(|(index, wire)| {
                (self.candidates & (1_u8 << index) != 0 && self.len == wire.len())
                    .then_some(APPROVAL_NAME_VALUES[index])
            })
    }
}

const APPROVAL_NAMES: [&[u8]; 7] = [
    b"id",
    b"params",
    b"method",
    b"jsonrpc",
    b"threadId",
    b"turnId",
    b"itemId",
];
const APPROVAL_NAME_VALUES: [ApprovalName; 7] = [
    ApprovalName::Id,
    ApprovalName::Params,
    ApprovalName::Method,
    ApprovalName::JsonRpc,
    ApprovalName::ThreadId,
    ApprovalName::TurnId,
    ApprovalName::ItemId,
];

impl ApprovalMachine {
    fn new(kind: ApprovalRequestKind) -> Self {
        Self {
            kind,
            location: ApprovalLocation::Root,
            expected: None,
            scalar: ApprovalScalar::None,
            discard_depth: 0,
            root_seen: 0,
            route_seen: 0,
            route_order: 0,
            payload_started: false,
            request_id: None,
            thread_id: None,
            turn_id: None,
            item_id: None,
            root_complete: false,
        }
    }

    fn scratch_bytes(&mut self, bytes: &[u8]) -> Result<(), ApprovalRequestSchemaError> {
        match &mut self.scalar {
            ApprovalScalar::Name(probe) => {
                probe.push(bytes);
                Ok(())
            }
            ApprovalScalar::RequestString(fixed)
            | ApprovalScalar::RequestNumber(fixed)
            | ApprovalScalar::Route(_, fixed) => fixed.push(bytes),
            ApprovalScalar::Discard => Ok(()),
            ApprovalScalar::None if bytes.is_empty() => Ok(()),
            ApprovalScalar::None => Err(ApprovalRequestSchemaError::EnvelopeShape),
        }
    }

    fn event(&mut self, event: Event) -> Result<(), ApprovalRequestSchemaError> {
        if self.discard_depth != 0 {
            return self.discard_event(event);
        }
        match event {
            Event::ContainerStart(kind) => self.start_container(kind),
            Event::ContainerEnd(kind) => self.end_container(kind),
            Event::ScalarStart(kind) => self.start_scalar(kind),
            Event::ScalarFragment(kind) => self.scalar_fragment(kind),
            Event::ScalarEnd(kind) => self.end_scalar(kind),
            Event::Boolean(_) | Event::Null => self.literal(),
        }
    }

    fn start_scalar(&mut self, kind: ScalarKind) -> Result<(), ApprovalRequestSchemaError> {
        if !matches!(self.scalar, ApprovalScalar::None) {
            return Err(ApprovalRequestSchemaError::EnvelopeShape);
        }
        if kind == ScalarKind::Name {
            if self.expected.is_some() {
                return Err(ApprovalRequestSchemaError::WrongType);
            }
            self.scalar = ApprovalScalar::Name(ApprovalNameProbe::new(self.location));
            return Ok(());
        }
        let expected = self
            .expected
            .take()
            .ok_or(ApprovalRequestSchemaError::EnvelopeShape)?;
        self.scalar = match (kind, expected) {
            (ScalarKind::String, ApprovalExpected::RequestId) => {
                ApprovalScalar::RequestString(ApprovalFixedBytes::new())
            }
            (ScalarKind::Number, ApprovalExpected::RequestId) => {
                ApprovalScalar::RequestNumber(ApprovalFixedBytes::new())
            }
            (ScalarKind::String, ApprovalExpected::Route(field)) => {
                ApprovalScalar::Route(field, ApprovalFixedBytes::new())
            }
            (ScalarKind::String | ScalarKind::Number, ApprovalExpected::Discard) => {
                ApprovalScalar::Discard
            }
            _ => return Err(ApprovalRequestSchemaError::WrongType),
        };
        Ok(())
    }

    fn scalar_fragment(&self, kind: ScalarKind) -> Result<(), ApprovalRequestSchemaError> {
        let valid = matches!(
            (&self.scalar, kind),
            (ApprovalScalar::Name(_), ScalarKind::Name)
                | (ApprovalScalar::RequestString(_), ScalarKind::String)
                | (ApprovalScalar::RequestNumber(_), ScalarKind::Number)
                | (ApprovalScalar::Route(_, _), ScalarKind::String)
                | (ApprovalScalar::Discard, ScalarKind::String | ScalarKind::Number)
        );
        valid
            .then_some(())
            .ok_or(ApprovalRequestSchemaError::WrongType)
    }

    fn end_scalar(&mut self, kind: ScalarKind) -> Result<(), ApprovalRequestSchemaError> {
        let scalar = std::mem::replace(&mut self.scalar, ApprovalScalar::None);
        match (kind, scalar) {
            (ScalarKind::Name, ApprovalScalar::Name(probe)) => self.finish_name(probe.finish()),
            (ScalarKind::String, ApprovalScalar::RequestString(bytes)) => {
                let value = bytes.as_str()?;
                if value.is_empty() {
                    return Err(ApprovalRequestSchemaError::InvalidRequestIdentity);
                }
                self.request_id = Some(ApprovalRequestId::String(value.into()));
                Ok(())
            }
            (ScalarKind::Number, ApprovalScalar::RequestNumber(bytes)) => {
                self.request_id = Some(parse_approval_number(bytes.as_str()?)?);
                Ok(())
            }
            (ScalarKind::String, ApprovalScalar::Route(field, bytes)) => {
                self.finish_route(field, bytes.as_str()?)
            }
            (
                ScalarKind::String | ScalarKind::Number,
                ApprovalScalar::Discard,
            ) => Ok(()),
            _ => Err(ApprovalRequestSchemaError::WrongType),
        }
    }

    fn start_container(&mut self, kind: ContainerKind) -> Result<(), ApprovalRequestSchemaError> {
        if !matches!(self.scalar, ApprovalScalar::None) {
            return Err(ApprovalRequestSchemaError::WrongType);
        }
        let expected = self
            .expected
            .take()
            .ok_or(ApprovalRequestSchemaError::EnvelopeShape)?;
        match (expected, kind) {
            (ApprovalExpected::Params, ContainerKind::Object) => {
                self.location = ApprovalLocation::Params;
                Ok(())
            }
            (ApprovalExpected::Discard, _) => {
                self.discard_depth = 1;
                Ok(())
            }
            _ => Err(ApprovalRequestSchemaError::WrongType),
        }
    }

    fn end_container(&mut self, kind: ContainerKind) -> Result<(), ApprovalRequestSchemaError> {
        if self.expected.is_some() || !matches!(self.scalar, ApprovalScalar::None) {
            return Err(ApprovalRequestSchemaError::WrongType);
        }
        match (self.location, kind) {
            (ApprovalLocation::Params, ContainerKind::Object) => {
                self.location = ApprovalLocation::Root;
                Ok(())
            }
            (ApprovalLocation::Root, ContainerKind::Object) => {
                if self.root_seen & APPROVAL_ROOT_ID == 0 {
                    return Err(ApprovalRequestSchemaError::MissingRequestIdentity);
                }
                if self.root_seen & APPROVAL_ROOT_PARAMS == 0 {
                    return Err(ApprovalRequestSchemaError::MissingParams);
                }
                self.root_complete = true;
                Ok(())
            }
            _ => Err(ApprovalRequestSchemaError::EnvelopeShape),
        }
    }

    fn literal(&mut self) -> Result<(), ApprovalRequestSchemaError> {
        match self.expected.take() {
            Some(ApprovalExpected::Discard) => Ok(()),
            _ => Err(ApprovalRequestSchemaError::WrongType),
        }
    }

    fn discard_event(&mut self, event: Event) -> Result<(), ApprovalRequestSchemaError> {
        match event {
            Event::ContainerStart(_) => {
                self.discard_depth = self
                    .discard_depth
                    .checked_add(1)
                    .ok_or(ApprovalRequestSchemaError::StructuredDepthExceeded)?;
            }
            Event::ContainerEnd(_) => {
                self.discard_depth -= 1;
            }
            Event::ScalarStart(_) => {
                if !matches!(self.scalar, ApprovalScalar::None) {
                    return Err(ApprovalRequestSchemaError::EnvelopeShape);
                }
                self.scalar = ApprovalScalar::Discard;
            }
            Event::ScalarFragment(_) => {
                if !matches!(self.scalar, ApprovalScalar::Discard) {
                    return Err(ApprovalRequestSchemaError::EnvelopeShape);
                }
            }
            Event::ScalarEnd(_) => {
                if !matches!(self.scalar, ApprovalScalar::Discard) {
                    return Err(ApprovalRequestSchemaError::EnvelopeShape);
                }
                self.scalar = ApprovalScalar::None;
            }
            Event::Boolean(_) | Event::Null => {}
        }
        Ok(())
    }

    fn finish_name(
        &mut self,
        name: Option<ApprovalName>,
    ) -> Result<(), ApprovalRequestSchemaError> {
        match self.location {
            ApprovalLocation::Root => match name {
                Some(ApprovalName::Id) if self.root_seen == 0 => {
                    self.root_seen |= APPROVAL_ROOT_ID;
                    self.expected = Some(ApprovalExpected::RequestId);
                }
                Some(ApprovalName::Params) if self.root_seen == APPROVAL_ROOT_ID => {
                    self.root_seen |= APPROVAL_ROOT_PARAMS;
                    self.expected = Some(ApprovalExpected::Params);
                }
                Some(
                    ApprovalName::Id
                    | ApprovalName::Params
                    | ApprovalName::Method
                    | ApprovalName::JsonRpc,
                ) => return Err(ApprovalRequestSchemaError::DuplicateDiscriminant),
                _ => return Err(ApprovalRequestSchemaError::EnvelopeShape),
            },
            ApprovalLocation::Params => match name {
                Some(ApprovalName::ThreadId) => self.prepare_route(
                    APPROVAL_ROUTE_THREAD,
                    ApprovalRouteField::Thread,
                )?,
                Some(ApprovalName::TurnId) => {
                    self.prepare_route(APPROVAL_ROUTE_TURN, ApprovalRouteField::Turn)?
                }
                Some(ApprovalName::ItemId) => {
                    self.prepare_route(APPROVAL_ROUTE_ITEM, ApprovalRouteField::Item)?
                }
                _ => {
                    self.payload_started = true;
                    self.expected = Some(ApprovalExpected::Discard);
                }
            },
        }
        Ok(())
    }

    fn prepare_route(
        &mut self,
        bit: u8,
        field: ApprovalRouteField,
    ) -> Result<(), ApprovalRequestSchemaError> {
        if self.route_seen & bit != 0 {
            return Err(ApprovalRequestSchemaError::DuplicateRoute);
        }
        if self.payload_started || bit <= self.route_order {
            return Err(ApprovalRequestSchemaError::EnvelopeShape);
        }
        self.route_seen |= bit;
        self.route_order = bit;
        self.expected = Some(ApprovalExpected::Route(field));
        Ok(())
    }

    fn finish_route(
        &mut self,
        field: ApprovalRouteField,
        value: &str,
    ) -> Result<(), ApprovalRequestSchemaError> {
        if value.is_empty() {
            return Err(ApprovalRequestSchemaError::InvalidRouteIdentity);
        }
        match field {
            ApprovalRouteField::Thread => {
                self.thread_id = Some(
                    CasThreadId::new(value)
                        .map_err(|_| ApprovalRequestSchemaError::InvalidRouteIdentity)?,
                );
            }
            ApprovalRouteField::Turn => {
                self.turn_id = Some(
                    CasTurnId::new(value)
                        .map_err(|_| ApprovalRequestSchemaError::InvalidRouteIdentity)?,
                );
            }
            ApprovalRouteField::Item => {
                self.item_id = Some(
                    CasItemId::new(value)
                        .map_err(|_| ApprovalRequestSchemaError::InvalidRouteIdentity)?,
                );
            }
        }
        Ok(())
    }

    fn map_parse_failure(&self, failure: ParseFailure) -> DecodeReaderError {
        use bounded_json::ErrorKind;
        let source = match failure.error().kind() {
            ErrorKind::DepthExceeded => ApprovalRequestSchemaError::StructuredDepthExceeded,
            ErrorKind::InvalidUtf8 | ErrorKind::InvalidEscape | ErrorKind::UnpairedSurrogate => {
                ApprovalRequestSchemaError::InvalidRequestIdentity
            }
            ErrorKind::UnexpectedByte
            | ErrorKind::InvalidNumber
            | ErrorKind::IncompleteDocument
            | ErrorKind::TrailingContent
            | ErrorKind::PositionOverflow
            | ErrorKind::InvalidApiUse => ApprovalRequestSchemaError::EnvelopeShape,
        };
        DecodeReaderError::Approval {
            kind: self.kind,
            source,
        }
    }

    fn finish(&mut self) -> Result<DecodedIncoming, ApprovalRequestSchemaError> {
        if !self.root_complete
            || !matches!(self.location, ApprovalLocation::Root)
            || self.expected.is_some()
            || !matches!(self.scalar, ApprovalScalar::None)
            || self.discard_depth != 0
        {
            return Err(ApprovalRequestSchemaError::EnvelopeShape);
        }
        let request_id = self
            .request_id
            .take()
            .ok_or(ApprovalRequestSchemaError::MissingRequestIdentity)?;
        Ok(DecodedIncoming::Approval(ApprovalRequest::decoded(
            request_id,
            self.kind,
            self.thread_id.take(),
            self.turn_id.take(),
            self.item_id.take(),
        )))
    }
}

fn parse_approval_number(
    value: &str,
) -> Result<ApprovalRequestId, ApprovalRequestSchemaError> {
    value
        .parse::<i64>()
        .map(ApprovalRequestId::Integer)
        .map_err(|_| ApprovalRequestSchemaError::InvalidRequestIdentity)
}

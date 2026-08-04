const DYNAMIC_ROOT_ID: u8 = 1;
const DYNAMIC_ROOT_PARAMS: u8 = 2;
const DYNAMIC_THREAD: u8 = 1;
const DYNAMIC_TURN: u8 = 2;
const DYNAMIC_CALL: u8 = 4;
const DYNAMIC_NAMESPACE: u8 = 8;
const DYNAMIC_TOOL: u8 = 16;
const DYNAMIC_ARGUMENTS: u8 = 32;

struct DynamicToolMachine<'a> {
    sink: Option<&'a mut dyn OrderedTurnStreamSink>,
    capture: Option<DynamicToolCapture<'a>>,
    response_authority_generation: u64,
    location: DynamicLocation,
    expected: Option<DynamicExpected>,
    scalar: DynamicScalar,
    root_seen: u8,
    params_seen: u8,
    params_order: u8,
    argument_depth: u8,
    argument_scalar_root: bool,
    argument_complete: bool,
    request_id: Option<DynamicToolCallRequestId>,
    thread_id: Option<CasThreadId>,
    turn_id: Option<CasTurnId>,
    call_id: Option<DynamicToolCallId>,
    namespace: Option<Box<str>>,
    tool: Option<DynamicToolName>,
    root_complete: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum DynamicLocation {
    Root,
    Params,
    Arguments,
}

#[derive(Clone, Copy)]
enum DynamicExpected {
    RequestId,
    Params,
    Identity(DynamicIdentityField),
    Arguments,
}

enum DynamicScalar {
    None,
    Name(DynamicNameProbe),
    RequestString(DynamicFixedBytes),
    RequestNumber(DynamicFixedBytes),
    Identity(DynamicIdentityField, DynamicFixedBytes),
    Argument(DynamicToolArgumentScalarKind),
}

#[derive(Clone, Copy)]
enum DynamicIdentityField {
    Thread,
    Turn,
    Call,
    Namespace,
    Tool,
}

#[derive(Clone, Copy)]
enum DynamicName {
    Id,
    Params,
    Method,
    JsonRpc,
    ThreadId,
    TurnId,
    CallId,
    Namespace,
    Tool,
    Arguments,
}

struct DynamicFixedBytes {
    bytes: [u8; DYNAMIC_TOOL_CALL_REQUEST_ID_MAX_BYTES],
    len: usize,
}

impl DynamicFixedBytes {
    const fn new() -> Self {
        Self {
            bytes: [0; DYNAMIC_TOOL_CALL_REQUEST_ID_MAX_BYTES],
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Result<(), DynamicToolCallSchemaError> {
        let end = self
            .len
            .checked_add(bytes.len())
            .filter(|end| *end <= self.bytes.len())
            .ok_or(DynamicToolCallSchemaError::IdentityTooLong)?;
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }

    fn as_str(&self) -> Result<&str, DynamicToolCallSchemaError> {
        std::str::from_utf8(&self.bytes[..self.len])
            .map_err(|_| DynamicToolCallSchemaError::InvalidIdentity)
    }
}

struct DynamicNameProbe {
    candidates: u16,
    len: usize,
}

impl DynamicNameProbe {
    const fn new(location: DynamicLocation) -> Self {
        Self {
            candidates: match location {
                DynamicLocation::Root => 0b0000_0000_0000_1111,
                DynamicLocation::Params => 0b0000_0011_1111_0000,
                DynamicLocation::Arguments => 0,
            },
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for byte in bytes {
            for (index, wire) in DYNAMIC_NAMES.iter().enumerate() {
                let bit = 1_u16 << index;
                if self.candidates & bit != 0 && wire.get(self.len) != Some(byte) {
                    self.candidates &= !bit;
                }
            }
            self.len = self.len.saturating_add(1);
        }
    }

    fn finish(self) -> Option<DynamicName> {
        DYNAMIC_NAMES.iter().enumerate().find_map(|(index, wire)| {
            (self.candidates & (1_u16 << index) != 0 && self.len == wire.len())
                .then_some(DYNAMIC_NAME_VALUES[index])
        })
    }
}

const DYNAMIC_NAMES: [&[u8]; 10] = [
    b"id",
    b"params",
    b"method",
    b"jsonrpc",
    b"threadId",
    b"turnId",
    b"callId",
    b"namespace",
    b"tool",
    b"arguments",
];
const DYNAMIC_NAME_VALUES: [DynamicName; 10] = [
    DynamicName::Id,
    DynamicName::Params,
    DynamicName::Method,
    DynamicName::JsonRpc,
    DynamicName::ThreadId,
    DynamicName::TurnId,
    DynamicName::CallId,
    DynamicName::Namespace,
    DynamicName::Tool,
    DynamicName::Arguments,
];

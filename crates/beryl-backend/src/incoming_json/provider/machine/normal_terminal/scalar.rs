const STATUS_WIRES: [&[u8]; 3] = [b"completed", b"interrupted", b"failed"];
const CODEX_UNIT_WIRES: [&[u8]; 11] = [
    b"contextWindowExceeded",
    b"sessionBudgetExceeded",
    b"usageLimitExceeded",
    b"serverOverloaded",
    b"cyberPolicy",
    b"internalServerError",
    b"unauthorized",
    b"badRequest",
    b"threadRollbackFailed",
    b"sandboxError",
    b"other",
];
const CODEX_OBJECT_WIRES: [&[u8]; 5] = [
    b"httpConnectionFailed",
    b"responseStreamConnectionFailed",
    b"responseStreamDisconnected",
    b"responseTooManyFailedAttempts",
    b"activeTurnNotSteerable",
];
const TURN_KIND_WIRES: [&[u8]; 2] = [b"review", b"compact"];

#[derive(Clone, Copy, Eq, PartialEq)]
enum Expected {
    RootParamsName,
    ParamsObject,
    ParamsThreadName,
    ThreadValue,
    ParamsTurnName,
    TurnObject,
    TurnIdName,
    TurnIdValue,
    TurnItemsName,
    TurnItemsArray,
    TurnItemsEnd,
    TurnItemsViewName,
    TurnItemsViewValue,
    TurnStatusName,
    TurnStatusValue,
    TurnErrorName,
    TurnErrorValue,
    ErrorMessageName,
    ErrorMessageValue,
    ErrorCodexInfoName,
    ErrorCodexInfoValue,
    CodexObjectName,
    CodexPayloadObject,
    CodexPayloadName,
    CodexPayloadValue,
    CodexPayloadEnd,
    CodexObjectEnd,
    ErrorAdditionalDetailsName,
    ErrorAdditionalDetailsValue,
    ErrorEnd,
    StartedAtName,
    StartedAtValue,
    CompletedAtName,
    CompletedAtValue,
    DurationMsName,
    DurationMsValue,
    TurnEnd,
    ParamsEnd,
    RootEnd,
    Done,
}

enum TerminalScalar {
    None,
    Name {
        probe: ExactProbe,
        expected: &'static [u8],
        next: Expected,
    },
    Identity {
        bytes: IdentityBytes,
        kind: IdentityKind,
        next: Expected,
    },
    Choice {
        probe: ChoiceProbe,
        kind: ChoiceKind,
        next: Expected,
    },
    Diagnostic {
        field: NormalTurnTerminalDiagnosticField,
        next: Expected,
    },
    Integer {
        accumulator: IntegerAccumulator,
        kind: IntegerKind,
        next: Expected,
    },
}

#[derive(Clone, Copy)]
enum IdentityKind {
    Thread,
    Turn,
}

#[derive(Clone, Copy)]
enum ChoiceKind {
    ItemsView,
    Status,
    CodexUnit,
    CodexObject,
    TurnKind,
}

#[derive(Clone, Copy)]
enum IntegerKind {
    DiscardSigned,
    HttpStatus,
}

#[derive(Clone, Copy)]
enum CodexObjectVariant {
    HttpConnectionFailed,
    ResponseStreamConnectionFailed,
    ResponseStreamDisconnected,
    ResponseTooManyFailedAttempts,
    ActiveTurnNotSteerable,
}

struct IdentityBytes {
    bytes: [u8; crate::PROTOCOL_IDENTITY_MAX_BYTES],
    len: usize,
}

impl IdentityBytes {
    const fn new() -> Self {
        Self {
            bytes: [0; crate::PROTOCOL_IDENTITY_MAX_BYTES],
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) -> bool {
        let Some(end) = self.len.checked_add(bytes.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
        true
    }

    fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes[..self.len]).ok()
    }
}

struct ExactProbe {
    matched: bool,
    len: usize,
}

impl ExactProbe {
    const fn new() -> Self {
        Self {
            matched: true,
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8], expected: &[u8]) {
        for byte in bytes {
            if expected.get(self.len) != Some(byte) {
                self.matched = false;
            }
            self.len = self.len.saturating_add(1);
        }
    }

    fn exact(&self, expected: &[u8]) -> bool {
        self.matched && self.len == expected.len()
    }
}

struct ChoiceProbe {
    candidates: u16,
    len: usize,
}

impl ChoiceProbe {
    fn new(wires: &[&[u8]]) -> Self {
        Self {
            candidates: (1_u16 << wires.len()) - 1,
            len: 0,
        }
    }

    fn push(&mut self, bytes: &[u8], wires: &[&[u8]]) {
        for byte in bytes {
            for (index, wire) in wires.iter().enumerate() {
                let bit = 1_u16 << index;
                if self.candidates & bit != 0 && wire.get(self.len) != Some(byte) {
                    self.candidates &= !bit;
                }
            }
            self.len = self.len.saturating_add(1);
        }
    }

    fn finish(&self, wires: &[&[u8]]) -> Option<usize> {
        wires.iter().enumerate().find_map(|(index, wire)| {
            (self.candidates & (1_u16 << index) != 0 && self.len == wire.len()).then_some(index)
        })
    }
}

struct IntegerAccumulator {
    magnitude: u64,
    negative: bool,
    saw_digit: bool,
    invalid: bool,
}

impl IntegerAccumulator {
    const fn new() -> Self {
        Self {
            magnitude: 0,
            negative: false,
            saw_digit: false,
            invalid: false,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for byte in bytes {
            match *byte {
                b'-' if !self.saw_digit && self.magnitude == 0 && !self.negative => {
                    self.negative = true;
                }
                digit @ b'0'..=b'9' => {
                    self.saw_digit = true;
                    self.magnitude = match self
                        .magnitude
                        .checked_mul(10)
                        .and_then(|value| value.checked_add(u64::from(digit - b'0')))
                    {
                        Some(value) => value,
                        None => {
                            self.invalid = true;
                            self.magnitude
                        }
                    };
                }
                _ => self.invalid = true,
            }
        }
    }

    fn is_i64(&self) -> bool {
        if self.invalid || !self.saw_digit {
            return false;
        }
        if self.negative {
            self.magnitude <= (i64::MAX as u64) + 1
        } else {
            self.magnitude <= i64::MAX as u64
        }
    }

    fn as_u16(&self) -> Option<u16> {
        (!self.invalid && self.saw_digit && !self.negative)
            .then(|| u16::try_from(self.magnitude).ok())
            .flatten()
    }
}

fn choice_wires(kind: ChoiceKind) -> &'static [&'static [u8]] {
    match kind {
        ChoiceKind::ItemsView => &[b"notLoaded"],
        ChoiceKind::Status => &STATUS_WIRES,
        ChoiceKind::CodexUnit => &CODEX_UNIT_WIRES,
        ChoiceKind::CodexObject => &CODEX_OBJECT_WIRES,
        ChoiceKind::TurnKind => &TURN_KIND_WIRES,
    }
}

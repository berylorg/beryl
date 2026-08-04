use std::fmt;

use super::{CompatibilityProbe, InlineUtf8};

pub const JSON_RPC_DIAGNOSTIC_MAX_BYTES: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonRpcErrorVerdict {
    ActiveTurnNotSteerable {
        turn_kind: JsonRpcTurnKind,
    },
    CompatibilityProbeRecognized {
        probe: CompatibilityProbe,
    },
    /// Pinned `turn/interrupt` handling did not enqueue the core interrupt.
    RejectedBeforeCoreInterrupt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonRpcTurnKind {
    Review,
    Compact,
}

#[derive(Debug, PartialEq, Eq)]
pub struct JsonRpcError {
    code: i64,
    diagnostic: InlineUtf8<JSON_RPC_DIAGNOSTIC_MAX_BYTES>,
    diagnostic_was_truncated: bool,
    data_was_present: bool,
    verdict: Option<JsonRpcErrorVerdict>,
}

impl JsonRpcError {
    pub(crate) fn projected(
        code: i64,
        diagnostic: &str,
        diagnostic_was_truncated: bool,
        data_was_present: bool,
        verdict: Option<JsonRpcErrorVerdict>,
    ) -> Self {
        let (diagnostic, constructor_truncated) = InlineUtf8::projected(diagnostic);
        Self {
            code,
            diagnostic,
            diagnostic_was_truncated: diagnostic_was_truncated || constructor_truncated,
            data_was_present,
            verdict,
        }
    }

    #[must_use]
    pub const fn code(&self) -> i64 {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        self.diagnostic.as_str()
    }

    #[must_use]
    pub const fn message_was_truncated(&self) -> bool {
        self.diagnostic_was_truncated
    }

    #[must_use]
    pub const fn data_was_present(&self) -> bool {
        self.data_was_present
    }

    #[must_use]
    pub const fn verdict(&self) -> Option<JsonRpcErrorVerdict> {
        self.verdict
    }
}

impl fmt::Display for JsonRpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for JsonRpcError {}

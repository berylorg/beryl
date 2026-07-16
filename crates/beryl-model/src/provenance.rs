use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{CommandId, IdempotencyKey, JobId, ValueError, runtime::bounded_text};

const MAX_EXTERNAL_ID_BYTES: usize = 256;
const MAX_TOOL_NAME_BYTES: usize = 128;

macro_rules! bounded_external_value {
    ($(#[$meta:meta])* $name:ident, $kind:literal, $max:expr) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(Box<str>);

        impl $name {
            /// Validates a bounded external value without interpreting provider syntax.
            pub fn new(value: impl AsRef<str>) -> Result<Self, ValueError> {
                bounded_text($kind, value.as_ref(), $max).map(Self)
            }

            /// Returns the exact external value.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(de::Error::custom)
            }
        }
    };
}

bounded_external_value!(
    /// Exact opaque Codex App Server thread identity.
    CasThreadId,
    "CAS thread identity",
    MAX_EXTERNAL_ID_BYTES
);
bounded_external_value!(
    /// Exact opaque Codex App Server turn identity.
    CasTurnId,
    "CAS turn identity",
    MAX_EXTERNAL_ID_BYTES
);
bounded_external_value!(
    /// Exact opaque Codex App Server item identity.
    CasItemId,
    "CAS item identity",
    MAX_EXTERNAL_ID_BYTES
);
bounded_external_value!(
    /// Exact opaque Codex App Server dynamic-tool call identity.
    DynamicToolCallId,
    "dynamic tool call identity",
    MAX_EXTERNAL_ID_BYTES
);
bounded_external_value!(
    /// Exact bounded dynamic-tool name.
    DynamicToolName,
    "dynamic tool name",
    MAX_TOOL_NAME_BYTES
);

/// Bounded source provenance shared across package boundaries.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Provenance {
    /// Input authored directly by the user.
    UserAuthored,
    /// Input generated from one admitted durable branch-handoff job.
    BerylGeneratedHandoff {
        /// Exact durable job identity.
        job_id: JobId,
        /// Logical-operation identity retained across safe retries.
        idempotency_key: IdempotencyKey,
    },
    /// One normalized live event emitted by CAS.
    CasLiveEvent {
        /// Exact source CAS thread.
        thread_id: CasThreadId,
        /// Exact source CAS turn.
        turn_id: CasTurnId,
        /// Monotonic source sequence retained by the owning capture boundary.
        sequence: u64,
    },
    /// One exact Beryl dynamic-tool invocation reported by CAS.
    DynamicToolCall {
        /// Exact source CAS thread.
        thread_id: CasThreadId,
        /// Exact source CAS turn.
        turn_id: CasTurnId,
        /// Registered dynamic-tool name.
        tool_name: DynamicToolName,
        /// Exact source tool-call identity.
        call_id: DynamicToolCallId,
    },
    /// An action performed by an admitted durable recovery command.
    DurableRecovery {
        /// Exact admitted command identity.
        command_id: CommandId,
        /// Logical-operation identity retained across safe retries.
        idempotency_key: IdempotencyKey,
    },
}

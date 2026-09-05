//! Exact identities and bounded evidence for receipt-aware compaction.
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

/// Normalized optional capability. Invalid advertisements disable only compaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompactionObservationCapability {
    Supported { observation_session_id: Uuid },
    Unavailable(CompactionCapabilityError),
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum CompactionCapabilityError {
    #[error("backend does not advertise compaction observation")]
    Missing,
    #[error("backend advertises an incomplete compaction observation capability")]
    Incomplete,
    #[error("backend compaction observation advertisement is malformed")]
    Malformed,
    #[error("backend compaction observation version {0} is unsupported (requires 1)")]
    UnsupportedVersion(u64),
}

impl Default for CompactionObservationCapability {
    fn default() -> Self {
        Self::Unavailable(CompactionCapabilityError::Missing)
    }
}

impl CompactionObservationCapability {
    pub fn session_id(&self) -> Result<Uuid, CompactionCapabilityError> {
        match self {
            Self::Supported {
                observation_session_id,
            } => Ok(*observation_session_id),
            Self::Unavailable(reason) => Err(*reason),
        }
    }
}

// These fields are flattened into initialize. Drop malformed raw data after
// normalization rather than retaining arbitrary backend values in the report.
impl<'de> Deserialize<'de> for CompactionObservationCapability {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Fields {
            compaction_observation_version: Option<Value>,
            compaction_observation_session_id: Option<Value>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let parsed = match (
            fields.compaction_observation_version,
            fields.compaction_observation_session_id,
        ) {
            (None, None) => Err(CompactionCapabilityError::Missing),
            (None, _) | (_, None) => Err(CompactionCapabilityError::Incomplete),
            (Some(version), Some(session)) => (|| {
                let version = version
                    .as_u64()
                    .ok_or(CompactionCapabilityError::Malformed)?;
                if version != 1 {
                    return Err(CompactionCapabilityError::UnsupportedVersion(version));
                }
                let session = session
                    .as_str()
                    .ok_or(CompactionCapabilityError::Malformed)?;
                Uuid::parse_str(session).map_err(|_| CompactionCapabilityError::Malformed)
            })(),
        };
        Ok(match parsed {
            Ok(observation_session_id) => Self::Supported {
                observation_session_id,
            },
            Err(reason) => Self::Unavailable(reason),
        })
    }
}

impl Serialize for CompactionObservationCapability {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        if let Self::Supported {
            observation_session_id,
        } = self
        {
            map.serialize_entry("compactionObservationVersion", &1u32)?;
            map.serialize_entry("compactionObservationSessionId", observation_session_id)?;
        }
        map.end()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompactionError {
    #[error("compaction is unavailable: {0}")]
    Capability(#[from] CompactionCapabilityError),
    #[error("compaction {field} must be a UUID")]
    InvalidIdentity { field: &'static str },
    #[error("compaction receipt has mismatched {field}")]
    IdentityMismatch { field: &'static str },
    #[error("invalid compaction receipt: {0}")]
    InvalidReceipt(&'static str),
}

pub(crate) fn identity(value: &str, field: &'static str) -> Result<Uuid, CompactionError> {
    Uuid::parse_str(value).map_err(|_| CompactionError::InvalidIdentity { field })
}

/// Retain this identity before sending start; never resubmit after a lost acknowledgement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionOperation {
    thread_id: Uuid,
    operation_id: Uuid,
    observation_session_id: Uuid,
}

impl CompactionOperation {
    pub(crate) fn new(
        thread_id: &str,
        observation_session_id: Uuid,
    ) -> Result<Self, CompactionError> {
        Ok(Self {
            thread_id: identity(thread_id, "threadId")?,
            operation_id: Uuid::new_v4(),
            observation_session_id,
        })
    }
    pub fn thread_id(&self) -> Uuid {
        self.thread_id
    }
    pub fn operation_id(&self) -> Uuid {
        self.operation_id
    }
    pub fn observation_session_id(&self) -> Uuid {
        self.observation_session_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactionReceipt {
    pub thread_id: Uuid,
    pub operation_id: Uuid,
    pub observation_session_id: Uuid,
    pub turn_id: Option<Uuid>,
    pub state: CompactionReceiptState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompactionReceiptState {
    Unknown { reason: CompactionUnknownReason },
    Accepted,
    Running,
    Completed,
    Failed { error: CompactionReceiptError },
    Interrupted { reason: CompactionInterruptedReason },
}

impl<'de> Deserialize<'de> for CompactionReceiptState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Empty struct variants enforce unknown-field rejection; Serde's unit
        // variants otherwise ignore accompanying fields even with deny_unknown_fields.
        #[derive(Deserialize)]
        #[serde(tag = "status", rename_all = "camelCase", deny_unknown_fields)]
        enum Wire {
            Unknown { reason: CompactionUnknownReason },
            Accepted {},
            Running {},
            Completed {},
            Failed { error: CompactionReceiptError },
            Interrupted { reason: CompactionInterruptedReason },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Unknown { reason } => Self::Unknown { reason },
            Wire::Accepted {} => Self::Accepted,
            Wire::Running {} => Self::Running,
            Wire::Completed {} => Self::Completed,
            Wire::Failed { error } => Self::Failed { error },
            Wire::Interrupted { reason } => Self::Interrupted { reason },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompactionUnknownReason {
    AbsentOrExpired,
    RuntimeChange,
    ObservationGap,
    IncompleteTerminalEvidence,
    SubmissionPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompactionInterruptedReason {
    Interrupted,
    Replaced,
    ReviewEnded,
    BudgetLimited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompactionErrorClassification {
    CoreTurnError,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompactionReceiptError {
    pub classification: CompactionErrorClassification,
    pub message: String,
    pub truncated: bool,
}

impl CompactionReceipt {
    pub(crate) fn validate(
        &self,
        operation: &CompactionOperation,
        expected_turn: Option<Uuid>,
        current_session: Uuid,
        start: bool,
    ) -> Result<(), CompactionError> {
        for (actual, expected, field) in [
            (self.thread_id, operation.thread_id, "threadId"),
            (self.operation_id, operation.operation_id, "operationId"),
            (
                self.observation_session_id,
                operation.observation_session_id,
                "observationSessionId",
            ),
        ] {
            if actual != expected {
                return Err(CompactionError::IdentityMismatch { field });
            }
        }
        let absent = matches!(
            self.state,
            CompactionReceiptState::Unknown {
                reason: CompactionUnknownReason::AbsentOrExpired
                    | CompactionUnknownReason::RuntimeChange
            }
        );
        if absent != self.turn_id.is_none() {
            return Err(CompactionError::InvalidReceipt(
                "state and turn identity disagree",
            ));
        }
        if let (Some(expected), Some(actual)) = (expected_turn, self.turn_id) {
            if actual != expected {
                return Err(CompactionError::IdentityMismatch { field: "turnId" });
            }
        }
        if current_session != operation.observation_session_id
            && !matches!(
                self.state,
                CompactionReceiptState::Unknown {
                    reason: CompactionUnknownReason::RuntimeChange
                }
            )
        {
            return Err(CompactionError::IdentityMismatch {
                field: "current observation session",
            });
        }
        if start
            && (!matches!(self.state, CompactionReceiptState::Accepted) || self.turn_id.is_none())
        {
            return Err(CompactionError::InvalidReceipt(
                "start must acknowledge accepted submission with exact turn identity",
            ));
        }
        if let CompactionReceiptState::Failed { error } = &self.state {
            if error.message.len() > 4096 {
                return Err(CompactionError::InvalidReceipt(
                    "error text exceeds 4096 UTF-8 bytes",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
pub(crate) struct CompactionStartResponse {
    pub receipt: CompactionReceipt,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompactionReadParams<'a> {
    #[serde(flatten)]
    pub operation: &'a CompactionOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_turn_id: Option<Uuid>,
}

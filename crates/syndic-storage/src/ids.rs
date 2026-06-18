use serde::{Deserialize, Serialize};

use crate::{Result, StorageError};

macro_rules! id_type {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self> {
                let value = value.into();
                validate_id($kind, &value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub(crate) fn validate(&self) -> Result<()> {
                validate_id($kind, &self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

id_type!(ConversationId, "conversation");
id_type!(ThreadViewId, "thread view");
id_type!(TurnId, "turn");
id_type!(ItemId, "item");
id_type!(SourceEventId, "source event");
id_type!(ProjectionRecordId, "projection record");
id_type!(TranscriptViewRecordId, "transcript view record");
id_type!(ResourceId, "resource");
id_type!(CursorId, "cursor");
id_type!(RecoveryMarkerId, "recovery marker");
id_type!(CasProjectionBindingId, "cas projection binding");

pub(crate) fn validate_id(kind: &'static str, value: &str) -> Result<()> {
    if value.is_empty() || value.as_bytes().contains(&0) {
        return Err(StorageError::InvalidId {
            kind,
            value: value.to_string(),
        });
    }

    Ok(())
}

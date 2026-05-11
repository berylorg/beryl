use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticGraphIdError {
    Empty,
    InvalidCharacter { ch: char },
}

macro_rules! define_graph_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, SemanticGraphIdError> {
                let value = value.into();
                validate_identifier(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

define_graph_id!(SemanticNodeId);
define_graph_id!(SoftLinkId);
define_graph_id!(ThreadRefId);
define_graph_id!(SoftLinkKind);

fn validate_identifier(value: &str) -> Result<(), SemanticGraphIdError> {
    if value.is_empty() {
        return Err(SemanticGraphIdError::Empty);
    }

    for ch in value.chars() {
        if !matches!(ch, 'a'..='z' | '0'..='9' | '-' | '_') {
            return Err(SemanticGraphIdError::InvalidCharacter { ch });
        }
    }

    Ok(())
}

impl fmt::Display for SemanticGraphIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "graph identifier must not be empty"),
            Self::InvalidCharacter { ch } => write!(
                f,
                "graph identifier contains invalid character {ch:?}; only lowercase ASCII letters, digits, '-' and '_' are allowed"
            ),
        }
    }
}

impl Error for SemanticGraphIdError {}

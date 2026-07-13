use super::*;

impl ThreadedDecisionRecordId {
    pub fn new(value: impl Into<String>) -> Result<Self, ThreadedDecisionIdError> {
        Ok(Self(normalize_id(value.into())?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ThreadedDecisionOperationId {
    pub fn new(value: impl Into<String>) -> Result<Self, ThreadedDecisionIdError> {
        Ok(Self(normalize_id(value.into())?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ThreadedDecisionIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "threaded-decision identifier must not be empty"),
        }
    }
}

impl Error for ThreadedDecisionIdError {}

fn normalize_id(value: String) -> Result<String, ThreadedDecisionIdError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(ThreadedDecisionIdError::Empty);
    }
    Ok(value)
}

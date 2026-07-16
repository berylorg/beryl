use std::{error::Error, fmt};

use beryl_home_store::{DomainCallbackError, DomainCallbackSource, MutationBuildError, ReadError};
use beryl_model::{RootId, RuntimeId};

use crate::{RecordRevision, ValueError};

/// Why a runtime/root mutation was rejected from current authoritative state.
#[derive(Debug)]
pub enum RuntimeRootMutationError {
    Read(ReadError),
    Build(MutationBuildError),
    Value(ValueError),
    RuntimeModeMismatch,
    RuntimeIdExists {
        runtime_id: RuntimeId,
    },
    ExecutableExists {
        runtime_id: RuntimeId,
    },
    RuntimeMissing {
        runtime_id: RuntimeId,
    },
    RootIdExists {
        root_id: RootId,
    },
    RootPathExists {
        root_id: RootId,
    },
    RootMissing {
        root_id: RootId,
    },
    RecordRevisionConflict {
        kind: &'static str,
        expected: RecordRevision,
        current: RecordRevision,
    },
    RootActivityNotLater,
}

impl fmt::Display for RuntimeRootMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::RuntimeModeMismatch => {
                formatter.write_str("runtime-native path does not match the owning runtime mode")
            }
            Self::RuntimeIdExists { runtime_id } => {
                write!(
                    formatter,
                    "runtime identity {runtime_id} is already registered"
                )
            }
            Self::ExecutableExists { runtime_id } => write!(
                formatter,
                "canonical executable is already registered as runtime {runtime_id}"
            ),
            Self::RuntimeMissing { runtime_id } => {
                write!(formatter, "runtime {runtime_id} is not registered")
            }
            Self::RootIdExists { root_id } => {
                write!(formatter, "root identity {root_id} is already registered")
            }
            Self::RootPathExists { root_id } => write!(
                formatter,
                "canonical root path is already registered as {root_id}"
            ),
            Self::RootMissing { root_id } => {
                write!(formatter, "root {root_id} is not registered")
            }
            Self::RecordRevisionConflict {
                kind,
                expected,
                current,
            } => write!(
                formatter,
                "{kind} record revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::RootActivityNotLater => {
                formatter.write_str("root activity time must strictly advance")
            }
        }
    }
}

impl Error for RuntimeRootMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Build(source) => Some(source),
            Self::Value(source) => Some(source),
            _ => None,
        }
    }
}

impl DomainCallbackError for RuntimeRootMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for RuntimeRootMutationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl From<MutationBuildError> for RuntimeRootMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl From<ValueError> for RuntimeRootMutationError {
    fn from(source: ValueError) -> Self {
        Self::Value(source)
    }
}

#[derive(Debug)]
pub(crate) enum RuntimeRootValidationError {
    Read(ReadError),
    Invariant(&'static str),
}

impl fmt::Display for RuntimeRootValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for RuntimeRootValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Invariant(_) => None,
        }
    }
}

impl DomainCallbackError for RuntimeRootValidationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for RuntimeRootValidationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

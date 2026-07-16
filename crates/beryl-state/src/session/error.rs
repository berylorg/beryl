use std::{error::Error, fmt};

use beryl_home_store::{DomainCallbackError, DomainCallbackSource, MutationBuildError, ReadError};
use beryl_model::{RevisionError, SessionRevision, SyndicThreadId, WindowId};

use crate::{RecordRevision, ValueError};

use super::WindowClaimSelection;

/// Why a minimal fixed-record session read could not produce one coherent generation.
#[derive(Debug)]
pub enum SessionReadError {
    Read(ReadError),
    MissingWindow {
        window_id: WindowId,
    },
    WindowIdentityMismatch {
        window_id: WindowId,
    },
    WindowRevisionConflict {
        window_id: WindowId,
        expected: RecordRevision,
        current: RecordRevision,
    },
    ConcurrentPublication,
}

impl fmt::Display for SessionReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::MissingWindow { window_id } => {
                write!(
                    formatter,
                    "active session references missing window {window_id}"
                )
            }
            Self::WindowIdentityMismatch { window_id } => write!(
                formatter,
                "active session window key and record identity disagree for {window_id}"
            ),
            Self::WindowRevisionConflict {
                window_id,
                expected,
                current,
            } => write!(
                formatter,
                "window {window_id} revision changed from {} to {} during bootstrap",
                expected.get(),
                current.get()
            ),
            Self::ConcurrentPublication => {
                formatter.write_str("active session changed during minimal bootstrap")
            }
        }
    }
}

impl Error for SessionReadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            _ => None,
        }
    }
}

impl From<ReadError> for SessionReadError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

/// Why a revision-checked session mutation was rejected.
#[derive(Debug)]
pub enum SessionMutationError {
    Read(ReadError),
    Build(MutationBuildError),
    Value(ValueError),
    Revision(RevisionError),
    NotInitialized,
    AlreadyInitialized,
    OrderlyExitInProgress,
    AlreadyOrderlyExit,
    WindowLimit,
    WindowExists {
        window_id: WindowId,
    },
    WindowMissing {
        window_id: WindowId,
    },
    SessionRevisionConflict {
        expected: SessionRevision,
        current: SessionRevision,
    },
    WindowRevisionConflict {
        window_id: WindowId,
        expected: RecordRevision,
        current: RecordRevision,
    },
    ClaimExpectationConflict {
        window_id: WindowId,
        expected: Option<WindowClaimSelection>,
        current: Option<WindowClaimSelection>,
    },
    ClaimMissing {
        window_id: WindowId,
    },
    ClaimCopiesDisagree {
        window_id: WindowId,
    },
    ThreadAlreadyClaimed {
        thread_id: SyndicThreadId,
        window_id: WindowId,
    },
    ClaimNotRestoring {
        window_id: WindowId,
    },
    SameThreadClaim {
        window_id: WindowId,
    },
    PlacementUnchanged {
        window_id: WindowId,
    },
    InvalidCurrentState(&'static str),
}

impl fmt::Display for SessionMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Build(source) => source.fmt(formatter),
            Self::Value(source) => source.fmt(formatter),
            Self::Revision(source) => source.fmt(formatter),
            Self::NotInitialized => formatter.write_str("session domain is not initialized"),
            Self::AlreadyInitialized => {
                formatter.write_str("session domain is already initialized")
            }
            Self::OrderlyExitInProgress => {
                formatter.write_str("ordinary session mutation is forbidden after orderly Exit")
            }
            Self::AlreadyOrderlyExit => {
                formatter.write_str("session is already marked orderly Exit")
            }
            Self::WindowLimit => formatter.write_str("session already has 256 restorable windows"),
            Self::WindowExists { window_id } => {
                write!(formatter, "window {window_id} already exists")
            }
            Self::WindowMissing { window_id } => {
                write!(formatter, "window {window_id} is not active")
            }
            Self::SessionRevisionConflict { expected, current } => write!(
                formatter,
                "session revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::WindowRevisionConflict {
                window_id,
                expected,
                current,
            } => write!(
                formatter,
                "window {window_id} revision conflict: expected {}, current {}",
                expected.get(),
                current.get()
            ),
            Self::ClaimExpectationConflict { window_id, .. } => {
                write!(formatter, "window {window_id} claim expectation is stale")
            }
            Self::ClaimMissing { window_id } => {
                write!(formatter, "window {window_id} claim is missing")
            }
            Self::ClaimCopiesDisagree { window_id } => {
                write!(
                    formatter,
                    "window {window_id} reverse claim copies disagree"
                )
            }
            Self::ThreadAlreadyClaimed {
                thread_id,
                window_id,
            } => write!(
                formatter,
                "thread {thread_id} is already claimed by {window_id}"
            ),
            Self::ClaimNotRestoring { window_id } => {
                write!(formatter, "window {window_id} claim is not restoring")
            }
            Self::SameThreadClaim { window_id } => {
                write!(
                    formatter,
                    "window {window_id} already owns the requested thread"
                )
            }
            Self::PlacementUnchanged { window_id } => {
                write!(formatter, "window {window_id} placement is unchanged")
            }
            Self::InvalidCurrentState(message) => formatter.write_str(message),
        }
    }
}

impl Error for SessionMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Build(source) => Some(source),
            Self::Value(source) => Some(source),
            Self::Revision(source) => Some(source),
            _ => None,
        }
    }
}

impl DomainCallbackError for SessionMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for SessionMutationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

impl From<MutationBuildError> for SessionMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

impl From<ValueError> for SessionMutationError {
    fn from(source: ValueError) -> Self {
        Self::Value(source)
    }
}

impl From<RevisionError> for SessionMutationError {
    fn from(source: RevisionError) -> Self {
        Self::Revision(source)
    }
}

#[derive(Debug)]
pub(crate) enum SessionValidationError {
    Read(ReadError),
    Invariant(&'static str),
}

impl fmt::Display for SessionValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Invariant(message) => formatter.write_str(message),
        }
    }
}

impl Error for SessionValidationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Invariant(_) => None,
        }
    }
}

impl DomainCallbackError for SessionValidationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl From<ReadError> for SessionValidationError {
    fn from(source: ReadError) -> Self {
        Self::Read(source)
    }
}

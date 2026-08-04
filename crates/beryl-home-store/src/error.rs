use std::{io, path::PathBuf};

use thiserror::Error;

use crate::HomeSchemaVersion;

/// Operation that failed before a pre-existing Beryl store was admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeOpenStage {
    /// The configured path was not an absolute host path.
    ValidateConfiguredPath,
    /// The configured home directory could not be created.
    CreateHomeDirectory,
    /// The home directory could not be opened with retained ownership semantics.
    OpenHomeDirectory,
    /// The opened home could not be assigned a canonical path and identity.
    IdentifyHomeDirectory,
    /// A reserved physical-layout path collided with another object.
    AdmitPhysicalLayout,
    /// The fixed ownership file could not be opened.
    OpenLockFile,
    /// The non-blocking exclusive ownership lock failed unexpectedly.
    AcquireLock,
    /// The package-owned practical Fjall storage profile was invalid.
    ConfigureStoragePolicy,
    /// A new Fjall database could not be created.
    CreateDatabase,
    /// A new home schema header could not be initialized durably.
    InitializeHeader,
    /// A generated opaque home identity could not be obtained.
    GenerateHomeIdentity,
}

/// Capability required to own a Beryl home safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeLockCapability {
    /// This platform does not implement the Windows home-ownership contract.
    WindowsPlatform,
    /// The filesystem cannot provide a stable opened-object identity.
    OpenedObjectIdentity,
    /// The opened target cannot be proved local.
    LocalStorage,
    /// The filesystem cannot provide the required exclusive byte-range lock.
    ExclusiveFileLock,
}

/// Validation step that made an existing physical store unreadable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeUnreadableStage {
    /// The database path could not be inspected safely.
    InspectDatabaseLayout,
    /// A nonempty database directory did not contain Fjall's version marker.
    MissingDatabaseVersion,
    /// Fjall could not recover the existing database.
    RecoverDatabase,
    /// The reserved home-header keyspace was absent.
    MissingHeaderKeyspace,
    /// The reserved home-header keyspace could not be opened.
    OpenHeaderKeyspace,
    /// The reserved logical-domain registry keyspace was absent.
    MissingDomainRegistryKeyspace,
    /// The reserved logical-domain registry keyspace could not be opened.
    OpenDomainRegistryKeyspace,
    /// The one required header record was absent.
    MissingHeaderRecord,
    /// The header record was malformed or had an unsupported encoding.
    DecodeHeader,
    /// The required complete-home revision record was absent.
    MissingHomeRevision,
    /// The complete-home revision record was malformed.
    DecodeHomeRevision,
}

/// Why a Beryl home could not be opened.
#[derive(Debug, Error)]
pub enum HomeOpenError {
    /// Another live owner holds the home lock.
    #[error("Beryl home `{path}` is already owned by another process")]
    Busy {
        /// Configured path whose fixed lock could not be acquired.
        path: PathBuf,
    },

    /// The target cannot supply a required ownership capability.
    #[error(
        "Beryl home `{path}` does not support required ownership capability {capability:?}: {source}"
    )]
    LockUnsupported {
        /// Configured home path.
        path: PathBuf,
        /// Capability that could not be established.
        capability: HomeLockCapability,
        /// Bounded diagnostic source.
        #[source]
        source: io::Error,
    },

    /// The durable home schema is not the exact schema supported by this caller.
    #[error(
        "Beryl home `{path}` uses schema {found}, but this process supports schema {supported}"
    )]
    UnsupportedSchema {
        /// Configured home path.
        path: PathBuf,
        /// Exact schema supported by the caller.
        supported: HomeSchemaVersion,
        /// Exact schema found in the durable header.
        found: HomeSchemaVersion,
    },

    /// A pre-existing store could not be validated without replacing it.
    #[error("Beryl home `{path}` is unreadable during {stage:?}: {source}")]
    Unreadable {
        /// Configured home path.
        path: PathBuf,
        /// Validation step that failed.
        stage: HomeUnreadableStage,
        /// Underlying engine, I/O, or format error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Opening or initializing the requested home failed.
    #[error("failed to open Beryl home `{path}` during {stage:?}: {source}")]
    Open {
        /// Configured home path.
        path: PathBuf,
        /// Admission step that failed.
        stage: HomeOpenStage,
        /// Underlying I/O, engine, or randomness error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl HomeOpenError {
    pub(crate) fn open(
        path: &std::path::Path,
        stage: HomeOpenStage,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Open {
            path: path.to_path_buf(),
            stage,
            source: Box::new(source),
        }
    }

    pub(crate) fn unreadable(
        path: &std::path::Path,
        stage: HomeUnreadableStage,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Unreadable {
            path: path.to_path_buf(),
            stage,
            source: Box::new(source),
        }
    }
}

/// Failure while explicitly releasing an orderly home ownership handle.
#[derive(Debug, Error)]
#[error("failed to release the Beryl-home ownership lock: {source}")]
pub struct HomeCloseError {
    pub(crate) source: io::Error,
}

//! Typed physical storage and exclusive process-ownership boundary for one Beryl home.
//!
//! Opening retains a real directory handle and one fixed, exclusively locked
//! `home.lock` file before Fjall is touched. A fresh home receives one opaque
//! durable identity; an existing home is force-recovered and validated without
//! any create-on-failure fallback.
//! Logical owners register exact versioned keyspace families and use typed,
//! explicitly bounded point/cursor reads. Cross-domain mutation plans are
//! validated on one serialized writer snapshot, committed in one physical
//! batch, and acknowledged only after `SyncAll`.
//! Surfaced persistence failures close the process-wide health gate. Callers
//! may perform one bounded verification or force-recover only the same still
//! locked home; successful recovery publishes a new generation, so domains
//! must reacquire typed handles with [`HomeStore::domain_handle`].
//! Content-addressed sidecars are flushed and atomically published before a
//! typed metadata command may retain their admission token. This package has
//! no sidecar deletion API.
//!
//! ```no_run
//! use std::convert::Infallible;
//! use beryl_home_store::{
//!     DomainReader, DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion,
//!     HomeStore, KeyspaceFamily, KeyspaceSchemaVersion, StorageDomain,
//! };
//!
//! struct ExampleDomain;
//! impl StorageDomain for ExampleDomain {
//!     const NAME: &'static str = "example";
//!     const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
//!     const KEYSPACES: &'static [KeyspaceFamily] = &[
//!         KeyspaceFamily::new("records", KeyspaceSchemaVersion::new(1)),
//!     ];
//!     type ValidationError = Infallible;
//!
//!     fn validate(
//!         _reader: &DomainReader<'_, Self>,
//!     ) -> Result<(), Self::ValidationError> {
//!         Ok(())
//!     }
//! }
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let directory = tempfile::tempdir()?;
//! let mut store = HomeStore::open(HomeOpenOptions::new(
//!     directory.path(),
//!     HomeSchemaVersion::CURRENT,
//! ))?;
//! let domain = store.register_domain::<ExampleDomain>()?;
//! assert_eq!(store.domain_revision(domain)?.get(), 1);
//! store.close()?;
//! # Ok(())
//! # }
//! ```
#![deny(unsafe_op_in_unsafe_fn)]

mod codec;
mod command;
mod domain;
mod error;
mod fault;
mod header;
mod health;
mod layout;
mod metadata;
mod ownership;
mod read;
mod recovery;
mod sidecar;
mod store;
mod writer;

pub use codec::{
    CursorDirection, CursorPage, CursorRange, CursorReadLimits, CursorRecord, DomainSchemaVersion,
    KeyspaceSchemaVersion, PointReadLimit, ReadLimitError, RecordCodec, RecordVersion,
};
pub use command::{
    CommandBuildError, CommandCancellation, CommandError, CommitReceipt, DomainMutation,
    HomeCommand, MutationBuildError, MutationBuilder, MutationContribution, RevisionConflict,
};
pub use domain::{
    DomainDefinitionError, DomainHandle, DomainHandleError, DomainRegistrationError,
    DomainRegistrationStage, DomainValidationError, KeyspaceFamily, StorageDomain,
};
pub use error::{
    HomeCloseError, HomeLockCapability, HomeOpenError, HomeOpenStage, HomeUnreadableStage,
};
pub use header::HomeSchemaVersion;
pub use health::{
    HealthGateError, HomeGeneration, HomeHealthSnapshot, HomeHealthState, RecoveryRetrySchedule,
};
pub use ownership::CanonicalHomeIdentity;
pub use read::{CodecOperation, DomainReader, ReadError, ReadStage};
pub use recovery::{HealthVerificationError, HomeRecoveryError, RecoveryReceipt};
pub use sidecar::{
    AdmittedSidecar, SidecarAddress, SidecarByteLimit, SidecarDigest, SidecarError,
    SidecarNamespace, SidecarNamespaceError, SidecarStage, SidecarVerifier, VerifiedSidecar,
};
pub use store::{HomeOpenOptions, HomeStore};

/// Deterministic concrete-boundary fault controls compiled only for package tests.
#[cfg(feature = "test-faults")]
pub mod test_faults {
    pub use crate::fault::{FaultBlock, FaultController, FaultPoint};
}

pub(crate) use header::HomeHeader;

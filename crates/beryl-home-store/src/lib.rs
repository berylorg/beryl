//! Typed physical storage and exclusive process-ownership boundary for one Beryl home.
//!
//! Opening retains a real directory handle and one fixed, exclusively locked
//! `home.lock` file before Fjall is touched. A fresh home receives one opaque
//! durable identity; an existing home is force-recovered and validated without
//! any create-on-failure fallback. The exact ordinary `state` directory is
//! retained without delete sharing across every Fjall generation, and recovery
//! requires that same opened-object identity rather than accepting a copied
//! database with a matching durable header.
//! Logical owners register exact owner- and codec-bound record families and use
//! typed, explicitly bounded point/cursor reads. Cross-domain mutation plans
//! perform only bounded participant checks on one serialized writer snapshot,
//! commit in one physical batch, and acknowledge only after `SyncAll`.
//! Registration, explicit verification, and recovery separately stream every
//! physical record envelope through its exact codec with bounded memory.
//! Each successful receipt carries its exact healthy home generation. Typed
//! owners use [`HomeStore::receipt_domain_revision`] to distinguish an affected
//! domain from an unaffected one and to reject foreign or obsolete completions.
//! Surfaced persistence failures close the process-wide health gate. Callers
//! may perform one bounded verification or force-recover only the same still
//! locked home; successful recovery publishes a new generation, so domains
//! must reacquire typed handles with [`HomeStore::domain_handle`].
//! Content-addressed sidecars retain every ancestor and final file object,
//! reject reparse or non-ordinary objects, and complete every parent and final
//! directory durability barrier before a typed metadata command may retain an
//! admission token. This package has no sidecar deletion API.
//! The `test-faults` feature adds only deterministic boundary controls and one
//! bounded exact-codec-rejected physical-envelope fixture; production builds
//! expose no corruption writer or raw storage handle.
//!
//! ```no_run
//! use std::convert::Infallible;
//! use beryl_home_store::{
//!     DomainReader, DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion,
//!     HomeStore, KeyspaceSchemaVersion, RecordCodec, RecordFamily,
//!     RecordVersion, StorageDomain,
//! };
//!
//! struct ExampleDomain;
//! struct ExampleCodec;
//! impl RecordCodec<ExampleDomain> for ExampleCodec {
//!     type Key = u8;
//!     type Value = u8;
//!     type Error = Infallible;
//!     const FAMILY: &'static str = "records";
//!     const VERSION: RecordVersion = RecordVersion::new(1);
//!     const MAX_KEY_BYTES: usize = 1;
//!     const MAX_VALUE_BYTES: usize = 1;
//!
//!     fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
//!         Ok(vec![*key])
//!     }
//!
//!     fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
//!         Ok(encoded[0])
//!     }
//!
//!     fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
//!         Ok(vec![*value])
//!     }
//!
//!     fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
//!         Ok(encoded[0])
//!     }
//! }
//!
//! impl StorageDomain for ExampleDomain {
//!     const NAME: &'static str = "example";
//!     const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
//!     const FAMILIES: &'static [RecordFamily<Self>] = &[
//!         RecordFamily::new::<ExampleCodec>(KeyspaceSchemaVersion::new(1)),
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
    KeyspaceSchemaVersion, PointReadLimit, ReadLimitError, RecordCodec, RecordFamily,
    RecordVersion,
};
pub use command::{
    CommandBuildError, CommandCancellation, CommandError, CommitReceipt, CommitReceiptError,
    ContributorCallbackStage, CurrentDomainCommand, DomainMutation, HomeCommand,
    MutationBuildError, MutationBuilder, MutationContribution, RevisionConflict,
};
pub use domain::{
    DomainCallbackError, DomainCallbackSource, DomainDefinitionError, DomainHandle,
    DomainHandleError, DomainRegistrationError, DomainRegistrationStage, DomainValidationError,
    StorageDomain,
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
    pub use crate::fault::{
        FaultBlock, FaultController, FaultPoint, PersistedCorruptionError, PersistedCorruptionStage,
    };
}

pub(crate) use header::HomeHeader;

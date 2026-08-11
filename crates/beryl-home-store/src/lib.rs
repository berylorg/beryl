//! Typed physical storage and exclusive process-ownership boundary for one Beryl home.
//!
//! Opening acquires one fixed, exclusive `home.lock` before Fjall is touched. A fresh home receives
//! one opaque durable identity; an existing nonempty `state` directory is force-recovered without
//! create-on-failure fallback. The configured home is trusted Operator-selected storage: this
//! package rejects an existing `state` reparse-point collision but does not promise detection of
//! external replacement, rollback, or tampering inside the selected home.
//! Logical owners register exact owner- and codec-bound record families and use
//! typed, explicitly bounded point/cursor reads. One point limit bounds its
//! stored value and decoded result while the request key remains independently
//! schema-bounded; one cursor limit
//! independently bounds the page's stored and practical decoded totals.
//! Codecs may refine the default encoded-length decoded-size estimates.
//! Cross-domain commands perform bounded participant checks on one serialized writer snapshot.
//! Before writer admission, every mutation declares schema- and count-bounded reconciliation
//! capacity and reserves one of exactly 1,024 operation slots. Under admission, exact encoded old
//! and intended-new record facts plus the intended receipt are materialized before Fjall batch
//! construction or mutation. Per-descriptor or aggregate reservation exhaustion returns exact
//! typed `NotCommitted` evidence without eager descriptor allocation. Explicit validation-only
//! participants may guard another domain without changing its revision; at least one mutation
//! remains required.
//!
//! [`HomeStore::execute`] and [`HomeStore::execute_current`] return exactly [`CommandOutcome`]:
//! definitive rejection carries only `NotCommitted` evidence, durable completion always carries a
//! generation-bound receipt and any later typed failure, and an uncertain durability cut carries
//! only the failure plus one opaque concrete reconciliation descriptor. Success is never reported
//! before `SyncAll`. Typed owners use [`HomeStore::receipt_domain_revision`] to reject foreign or
//! obsolete completions and distinguish affected from unaffected domains.
//!
//! Registration at a schema-validation boundary and explicit scrub paths stream physical record
//! envelopes through their exact codecs with bounded memory; routine command work remains
//! operation-bounded. Content-addressed sidecars complete the strongest supported write, rename,
//! and directory-persistence sequence before a typed metadata command may retain an admission
//! token. This package has no sidecar deletion API.
//! The `test-faults` feature adds only deterministic boundary controls and one
//! bounded exact-codec-rejected physical-envelope fixture; production builds
//! expose no corruption writer or raw storage handle.
//!
//! ```no_run
//! use std::convert::Infallible;
//! use beryl_home_store::{
//!     CommandOutcome, DomainMutation, DomainReader, DomainSchemaVersion, DomainValidator,
//!     HomeCommand, HomeOpenOptions,
//!     HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, PointReadLimit, RecordCodec,
//!     MutationBuilder, RecordFamily, RecordVersion, ReconciliationReservation, StorageDomain,
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
//! struct ExampleGuard;
//! impl DomainValidator<ExampleDomain> for ExampleGuard {
//!     type Error = Infallible;
//!
//!     fn validate(
//!         &self,
//!         _reader: &DomainReader<'_, ExampleDomain>,
//!     ) -> Result<(), Self::Error> {
//!         Ok(())
//!     }
//! }
//!
//! struct ExampleMutation;
//! impl DomainMutation<ExampleDomain> for ExampleMutation {
//!     type Error = Infallible;
//!
//!     fn validate(
//!         &self,
//!         _reader: &DomainReader<'_, ExampleDomain>,
//!     ) -> Result<(), Self::Error> {
//!         Ok(())
//!     }
//!
//!     fn reserve_reconciliation(
//!         &self,
//!         reservation: &mut ReconciliationReservation<'_, ExampleDomain>,
//!     ) -> Result<(), Self::Error> {
//!         reservation.reserve_records::<ExampleCodec>(1).unwrap();
//!         Ok(())
//!     }
//!
//!     fn contribute(
//!         &self,
//!         _reader: &DomainReader<'_, ExampleDomain>,
//!         mutations: &mut MutationBuilder<'_, ExampleDomain>,
//!     ) -> Result<(), Self::Error> {
//!         mutations.put::<ExampleCodec>(&1, &1).unwrap();
//!         Ok(())
//!     }
//! }
//!
//! # fn example() -> Result<CommandOutcome, Box<dyn std::error::Error>> {
//! let directory = tempfile::tempdir()?;
//! let mut store = HomeStore::open(HomeOpenOptions::new(
//!     directory.path(),
//!     HomeSchemaVersion::CURRENT,
//! ))?;
//! let domain = store.register_domain::<ExampleDomain>()?;
//! assert_eq!(store.domain_revision(domain)?.get(), 1);
//! assert!(store
//!     .read_point::<ExampleDomain, ExampleCodec>(
//!         domain,
//!         &1,
//!         PointReadLimit::new(6)?,
//!     )?
//!     .is_none());
//! let mut command = HomeCommand::new(store.home_revision()?);
//! command.add(domain.contribution(store.domain_revision(domain)?, ExampleMutation))?;
//! let outcome = store.execute(command);
//! match &outcome {
//!     CommandOutcome::NotCommitted { evidence } => eprintln!("not committed: {evidence}"),
//!     CommandOutcome::Committed { receipt, later_failure } => {
//!         assert!(later_failure.is_none());
//!         assert_eq!(receipt.home_revision().get(), 2);
//!     }
//!     CommandOutcome::Indeterminate { failure, reconciliation } => {
//!         eprintln!("indeterminate and retained for reconciliation: {failure}; {reconciliation:?}");
//!     }
//! }
//! store.close()?;
//! # Ok(outcome)
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
mod reconciliation;
mod recovery;
mod sidecar;
mod store;
mod writer;

pub use codec::{
    CursorDirection, CursorPage, CursorRange, CursorReadLimits, CursorRecord, DomainSchemaVersion,
    KeyspaceSchemaVersion, PointReadLimit, ReadLimitError, RecordCodec, RecordFamily,
    RecordVersion, RECORD_VERSION_BYTES,
};
pub use command::{
    CommandBuildError, CommandCancellation, CommandError, CommandOutcome, CommitReceipt,
    CommitReceiptError, ContributorCallbackStage, CurrentDomainCommand, DomainMutation,
    DomainValidator, HomeCommand, MutationBuildError, MutationBuilder, MutationContribution,
    ReconciliationDescriptor, ReconciliationReservation, RevisionConflict, StorageCommitState,
    StorageErrorClass, StorageResource, ValidationContribution,
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
        FaultBlock, FaultController, FaultPoint, FaultScope, PersistedCorruptionError,
        PersistedCorruptionStage,
    };
}

pub(crate) use header::HomeHeader;

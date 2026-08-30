//! Typed physical storage and exclusive process-ownership boundary for one Beryl home.
//!
//! Opening acquires one fixed, exclusive `home.lock` before Fjall is touched. A fresh home receives
//! one opaque durable identity; an existing nonempty `state` directory is force-recovered without
//! create-on-failure fallback. The configured home is trusted Operator-selected storage: this
//! package rejects an existing `state` reparse-point collision but does not promise detection of
//! external replacement, rollback, or tampering inside the selected home. [`HomeDurabilityTier`]
//! reports full durability only for native local NTFS; every other successfully locked filesystem
//! is admitted as best effort without a remote-durability probe.
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
//! only the failure plus move-only [`ReconciliationCustody`]. The immediate recipient may call
//! [`ReconciliationCustody::install`] to synchronously and infallibly transfer the sole descriptor,
//! exact reserved slot, and conservative byte charge into its originating per-home registry;
//! ordinary custody destruction performs that same fallback installation. Installation executes no
//! reconciliation work. [`ReconciliationCustody::install_and_handle`] additionally returns the
//! opaque exact-scope capability accepted by [`HomeStore::reconcile`]; duplicate triggers join one
//! result, and [`HomeStore::pending_reconciliations`] recovers handles after fallback installation.
//! Domain hooks receive only [`ReconciliationReader`], whose typed records are limited to the
//! descriptor's exact natural identities. At most four caller-thread workers execute per home.
//! The registry survives same-home store-generation recovery; orderly close
//! stops reservations and returns a [`HomeCloseError`] retaining the open store while reserved or
//! installed custody remains. Success is never reported before `SyncAll`.
//! Persisted-domain [`HomeStore::register_domain`] is routine declaration/family/type
//! reacquisition and never scans application records. Call
//! [`HomeStore::register_domain_with_schema_validation`] only at an explicit schema-validation
//! boundary. [`HomeStore::scrub_whole_home`] is the separate per-home exhaustive path; concurrent
//! requests join one worker and corruption evidence coalesces at most one rerun.
//! Failed-store [`HomeStore::recover_same_home`] consumes the failed service, drops its Fjall
//! generation and writer, and returns an unpublished [`HomeRecoveryCandidate`] built from a fresh
//! Fjall configuration and writer. Typed handles may be reacquired from the candidate, but ordinary
//! reads and writes remain closed until the owning full-stack recovery boundary consumes
//! [`HomeRecoveryCandidate::publish`]. Stack construction may consume
//! [`HomeRecoveryCandidate::abort`] to retain failed authority for retry; plain candidate drop
//! retains the lifetime custodian fail-closed. A failed attempt returns [`HomeRecoveryFailure`], which
//! retains the failed store, lifetime lock, and reconciliation registry for a later retry.
//! Typed owners use [`HomeStore::receipt_domain_revision`] to reject foreign or obsolete
//! completions and distinguish affected from unaffected domains.
//!
//! Registration at a schema-validation boundary and explicit scrub paths stream physical record
//! envelopes through their exact codecs with bounded memory; routine command work remains
//! operation-bounded. Content-addressed sidecars complete the strongest supported write, rename,
//! and directory-persistence sequence before a typed metadata command may retain an admission
//! token. This package has no sidecar deletion API.
//! The installed-theme repository is a separate physical boundary at `themes/manifest.toml` and
//! `themes/installed/<stable-theme-id>.toml`. Callers acquire a store-instance snapshot, observe
//! and read exact files with explicit bounds, and stream staged replacements through document-only,
//! manifest-only, manifest-last install, or manifest-first delete operations. Mutation results are
//! exactly [`ThemeMutationOutcome::NotCommitted`], [`ThemeMutationOutcome::Committed`], or
//! [`ThemeMutationOutcome::Indeterminate`]; retained indeterminate evidence can be reconciled by a
//! fresh store for the same durable home. [`HomeStore::subscribe_theme_changes`] exposes one bounded
//! coalescing wakeup lane without paths, bytes, parsing, or commit authority.
//! [`HomeStore::query_free_space`] performs one synchronous, uncached observation against the
//! opened home's canonical path. [`FreeSpaceOutcome::Sufficient`] is not a filesystem reservation:
//! later writes retain their ordinary error and commit-outcome classification.
//! [`DurableStartFootprint`] composes only typed Syndic durable-start and optional Asset owner-
//! transfer participants. It derives journal bytes from Fjall's public format-owned calculator;
//! it accepts no caller-provided aggregate or admission-policy budget.
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
//!     ReconciliationResolution,
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
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let directory = tempfile::tempdir()?;
//! let mut store = HomeStore::open(HomeOpenOptions::new(
//!     directory.path(),
//!     HomeSchemaVersion::CURRENT,
//! ))?;
//! let _durability_tier = store.durability_tier();
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
//! match outcome {
//!     CommandOutcome::NotCommitted { evidence } => eprintln!("not committed: {evidence}"),
//!     CommandOutcome::Committed { receipt, later_failure } => {
//!         assert!(later_failure.is_none());
//!         assert_eq!(receipt.home_revision().get(), 2);
//!     }
//!     CommandOutcome::Indeterminate { failure, reconciliation } => {
//!         eprintln!("indeterminate and retained for reconciliation: {failure}; {reconciliation:?}");
//!         let handle = reconciliation.install_and_handle();
//!         match store.reconcile(&handle)? {
//!             ReconciliationResolution::ExactOld => eprintln!("the command did not commit"),
//!             ReconciliationResolution::ExactNew { receipt } => {
//!                 assert_eq!(receipt.home_revision().get(), 2);
//!             }
//!             ReconciliationResolution::Collision => {
//!                 eprintln!("the exact operation scope remains closed");
//!             }
//!         }
//!     }
//! }
//! store.close()?;
//! # Ok(())
//! # }
//! ```
//!
//! ```
//! use beryl_home_store::CheckedBatchFootprint;
//!
//! let record = CheckedBatchFootprint::new(1, 16, 68);
//! assert_eq!(84, record.encoded_key_value_bytes()?);
//! # Ok::<(), beryl_home_store::DurableStartFootprintError>(())
//! ```
//!
//! ```no_run
//! use std::num::NonZeroUsize;
//! use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore, ThemeOperationLimits};
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let directory = tempfile::tempdir()?;
//! let store = HomeStore::open(HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT))?;
//! let limits = ThemeOperationLimits::new(
//!     1024 * 1024,
//!     NonZeroUsize::new(8192).unwrap(),
//!     NonZeroUsize::new(2).unwrap(),
//!     NonZeroUsize::new(4).unwrap(),
//!     NonZeroUsize::new(512).unwrap(),
//! )?;
//! let snapshot = store.theme_repository_snapshot(limits)?;
//! assert!(snapshot.manifest_identity().is_none());
//! # Ok(())
//! # }
//! ```
#![deny(unsafe_op_in_unsafe_fn)]

mod codec;
mod command;
mod domain;
mod error;
mod fault;
mod footprint;
mod free_space;
mod header;
mod health;
mod layout;
mod metadata;
mod ownership;
mod proof;
mod read;
mod reconciliation;
mod recovery;
mod scrub;
mod sidecar;
mod store;
mod successor;
mod theme;
mod turn_start_admission;
mod writer;

pub use codec::{
    CursorDirection, CursorPage, CursorRange, CursorReadLimits, CursorRecord, DomainSchemaVersion,
    KeyspaceSchemaVersion, PointReadLimit, RECORD_VERSION_BYTES, ReadLimitError, RecordCodec,
    RecordFamily, RecordVersion,
};
pub use command::{
    CommandBuildError, CommandCancellation, CommandError, CommandOutcome, CommitReceipt,
    CommitReceiptError, CommittedLocalFinalization, CommittedLocalFinalizationError,
    ContributorCallbackStage, CurrentDomainCommand, DomainMutation, DomainValidator, HomeCommand,
    MutationBuildError, MutationBuilder, MutationContribution, ReconciliationCustody,
    ReconciliationReservation, RevisionConflict, StorageCommitState, StorageErrorClass,
    StorageResource, ValidationContribution,
};
pub use domain::{
    DomainAttachmentAccessError, DomainAttachmentCapability, DomainCallbackError,
    DomainCallbackSource, DomainDefinitionError, DomainHandle, DomainHandleError,
    DomainRegistrationError, DomainRegistrationStage, DomainRuntimeAttachment,
    DomainValidationError, StorageDomain, WholeHomeScrubError,
};
pub use error::{
    HomeCloseError, HomeLockCapability, HomeOpenError, HomeOpenStage, HomeUnreadableStage,
};
pub use footprint::{
    AssetOwnerTransferFootprint, CheckedBatchFootprint, DurableStartFootprint,
    DurableStartFootprintError, ParticipatingDomainFootprint, SyndicDurableStartFootprint,
    participating_domain_footprint,
};
pub use free_space::FreeSpaceOutcome;
pub use header::HomeSchemaVersion;
pub use health::{
    HealthGateError, HomeGeneration, HomeHealthSnapshot, HomeHealthState, RecoveryRetrySchedule,
};
pub use proof::{
    ExecutableHomeProofCommand, FixedDigestHomeProofProtocol, HomeProofCommand, HomeProofProtocol,
    HomeProofReceipt, InlineProofCorrelation, MAX_PROOF_CORRELATION_BYTES, MAX_PROOF_ROLES,
    ProofCommandBuildError, ProofCommandSealError, ProofCompositionError, ProofCorrelation,
    ProofCorrelationBytes, ProofDomain, ProofProtocolIdentity, ProofReceiptConsumer,
    ProofReceiptError, ProofSourceContribution, ProofWitnessContribution,
};
pub use read::{CodecOperation, DomainReader, DomainRegistrationReader, ReadError, ReadStage};
pub use reconciliation::{
    DomainReconciliation, ReconciliationFailure, ReconciliationHandle, ReconciliationReader,
    ReconciliationRecord, ReconciliationResolution,
};
pub use recovery::{
    HomeRecoveryCandidate, HomeRecoveryError, HomeRecoveryFailure, RecoveryReceipt,
};
pub use scrub::WholeHomeScrubTrigger;
pub use sidecar::{
    AdmittedSidecar, SidecarAddress, SidecarByteLimit, SidecarDigest, SidecarError,
    SidecarNamespace, SidecarNamespaceError, SidecarStage, SidecarVerifier, VerifiedSidecar,
};
#[cfg(feature = "test-faults")]
pub use store::HomeOwnershipTestSeam;
pub use store::{HomeDurabilityTier, HomeOpenOptions, HomeStore};
pub use successor::{
    FirstAcceptancePromotionProtocolV1, SuccessorCorrelation, SuccessorObservation,
    SuccessorPointRead, SuccessorPointReader, SuccessorPointRecord, SuccessorProtocol,
    SuccessorReadRejection, SuccessorReadReservation, SuccessorSource, SuccessorWitness,
};
pub use theme::{
    StableThemeFileId, StableThemeFileIdError, ThemeCommitEvidence, ThemeFileIdentity,
    ThemeFileRange, ThemeFileSelector, ThemeMutationOutcome, ThemeOperationLimits,
    ThemeOperationLimitsError, ThemeReconciliationEvidence, ThemeReconciliationOutcome,
    ThemeRepositoryError, ThemeRepositorySnapshot, ThemeRepositoryStage, ThemeWatchError,
    ThemeWatchHint, ThemeWatchLimits, ThemeWatchLimitsError, ThemeWatchSubscription,
};
pub use turn_start_admission::{
    DURABLE_START_ADMISSION_BUDGET_BYTES, MinimumTurnCaptureReserve, TurnStartAdmissionRequirement,
    TurnStartAdmissionRequirementError,
};

/// Deterministic concrete-boundary fault controls compiled only for package tests.
#[cfg(feature = "test-faults")]
pub mod test_faults {
    pub use crate::domain::capability_with_test_attachment_type;
    pub use crate::fault::{
        FaultBlock, FaultController, FaultPoint, FaultScope, FreeSpaceTestObservation,
        PersistedCorruptionError, PersistedCorruptionStage,
    };
    pub use crate::metadata::{decode_test_domain_metadata, encode_test_domain_metadata};
    pub use crate::proof::ProofCommandIdentityTestHarness;
    pub use crate::scrub::{ScrubTerminalDecisionBlock, ScrubTestSnapshot};
}

pub(crate) use header::HomeHeader;

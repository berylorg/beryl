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
#![deny(unsafe_op_in_unsafe_fn)]

mod codec;
mod coherence;
mod command;
mod domain;
mod error;
mod fault;
mod footprint;
mod free_space;
mod header;
mod health;
mod initial_open;
mod layout;
mod metadata;
mod mutation_observation;
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
pub use coherence::HomeCoherenceError;
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
pub use initial_open::{
    HomeCandidateError, HomeCandidateFailure, HomeDomainRequirements, HomeDomainRequirementsError,
    HomeOpenCandidate, HomeOpenPublication,
};
pub use mutation_observation::{
    HomeMutationObservation, HomeMutationObservationError, HomeMutationObserver,
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
    FirstAcceptancePromotionAdmission, FirstAcceptancePromotionAssetAdapter,
    FirstAcceptancePromotionAssetPlan, FirstAcceptancePromotionAssetSeed,
    FirstAcceptancePromotionObservation, FirstAcceptancePromotionSource,
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
        JournalWriteFault, PersistedCorruptionError, PersistedCorruptionStage,
        fail_next_journal_write,
    };
    pub use crate::initial_open::{with_initial_candidate_store, with_initial_publication_store};
    pub use crate::metadata::{decode_test_domain_metadata, encode_test_domain_metadata};
    pub use crate::proof::ProofCommandIdentityTestHarness;
    pub use crate::read::{reset_test_point_acquisition_count, test_point_acquisition_count};
    pub use crate::scrub::{ScrubTerminalDecisionBlock, ScrubTestSnapshot};
}

pub(crate) use header::HomeHeader;

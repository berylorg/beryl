//! Typed durable schemas and commands for Beryl-owned application state.
//!
//! `beryl-state` registers exact Beryl-owned runtime/root, thread-presentation,
//! session/window/claim, settings, durable-job, catalog, and asset-reference domains through
//! [`beryl_home_store`] without receiving a database, keyspace, batch, or
//! encoded record. Callers admit external facts before building short
//! revision-checked contributions. Successful command receipts are projected
//! through each opaque domain state, which rejects obsolete home generations
//! without exposing the underlying storage-domain handle.
//!
//! # Asset Reference Batches
//!
//! Marker-bearing owners retain the stable marker identity across bounded atomic
//! additions and moves. A move description can be cloned for exact post-admission
//! reconciliation.
//!
//! ```
//! use std::num::NonZeroU64;
//! use beryl_model::{
//!     AssetId, SyndicAcceptedInputId, SyndicDraftMarkerId, SyndicItemId,
//! };
//! use beryl_state::{AssetReferenceMove, AssetReferenceOwner, MoveAssetReferences};
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let marker_id = SyndicDraftMarkerId::from_bytes([3; 16]);
//! let source = AssetReferenceOwner::AcceptedInputMarker {
//!     input_id: SyndicAcceptedInputId::from_bytes([1; 16]),
//!     marker_id,
//! };
//! let destination = AssetReferenceOwner::SubmittedTurnItemMarker {
//!     item_id: SyndicItemId::from_bytes([2; 16]),
//!     marker_id,
//! };
//! let asset_id = AssetId::sha256_v1([4; 32], NonZeroU64::new(12).unwrap());
//! let moves = MoveAssetReferences::new(vec![AssetReferenceMove::new(
//!     source,
//!     destination,
//!     asset_id,
//! )?])?;
//! assert_eq!(moves.moves().len(), 1);
//! # Ok(())
//! # }
//! ```
//!
//! # Example
//!
//! ```no_run
//! use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
//! use beryl_model::{
//!     AdmittedHostPath, Availability, PathFlavor, RootId, RuntimeId, RuntimeMode,
//!     RuntimeNativePath,
//! };
//! use beryl_state::{
//!     AvailabilitySnapshot, BerylStateBootstrap, CreateRuntimeWithHomeRoot, RootRegistration,
//!     RuntimeRegistration, UnixMillis,
//! };
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let directory = tempfile::tempdir()?;
//! let mut home = HomeStore::open(HomeOpenOptions::new(
//!     directory.path(),
//!     HomeSchemaVersion::CURRENT,
//! ))?;
//! let bootstrap = BerylStateBootstrap::register(&mut home)?;
//! let _restorable_session = bootstrap.session().minimal_bootstrap(&home)?;
//! let state = bootstrap.complete(&mut home)?;
//! let mode = RuntimeMode::host();
//! let runtime = RuntimeRegistration::new(
//!     RuntimeId::from_bytes([1; 16]),
//!     AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\\Codex\\codex.exe")?,
//!     mode.clone(),
//!     RuntimeNativePath::from_admitted(
//!         mode.clone(),
//!         PathFlavor::Windows,
//!         r"C:\\Codex\\codex.exe",
//!     )?,
//!     UnixMillis::new(1),
//!     AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(1))?,
//! )?;
//! let root = RootRegistration::new(
//!     RootId::from_bytes([2; 16]),
//!     RuntimeNativePath::from_admitted(mode, PathFlavor::Windows, r"C:\\Users\\operator")?,
//!     AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\\Users\\operator")?,
//!     UnixMillis::new(1),
//!     AvailabilitySnapshot::unknown(),
//! );
//! let expected_domain = state.runtime_roots().revision(&home)?;
//! let mut command = HomeCommand::new(home.home_revision()?);
//! command.add(state.runtime_roots().create_runtime_with_home_root(
//!     expected_domain,
//!     CreateRuntimeWithHomeRoot::new(runtime, root)?,
//! ))?;
//! let receipt = home.execute(command)?;
//! assert_eq!(
//!     state.runtime_roots().committed_revision(&home, &receipt)?,
//!     Some(expected_domain.checked_next()?),
//! );
//! # Ok(())
//! # }
//! ```
#![forbid(unsafe_code)]

mod asset;
mod catalog;
mod durable_job;
mod encoding;
mod runtime_root;
mod session;
mod settings;
mod state;
mod thread_metadata;
mod value;

pub use asset::{
    AddAssetReference, AddAssetReferences, AssetAdmissionError, AssetDimensions, AssetMediaType,
    AssetMetadataRecord, AssetMutationError, AssetReferenceAddition, AssetReferenceAdditionStatus,
    AssetReferenceBatchError, AssetReferenceMove, AssetReferenceMoveStatus, AssetReferenceOwner,
    AssetReferenceRecord, AssetReferenceStatusError, AssetSidecarState, AssetState,
    AssetValueError, CreateAssetWithReference, FirstAssetContribution, MAX_ASSET_REFERENCE_BATCH,
    MoveAssetReferences, RemoveAssetReference,
};
pub use catalog::{
    CATALOG_MAX_STORED_RECENCY_BYTES, CATALOG_MAX_STORED_ROW_BYTES, CatalogArchiveSummary,
    CatalogAvailabilitySummary, CatalogClaimKind, CatalogClaimSummary, CatalogExecutionSummary,
    CatalogFacts, CatalogFreshness, CatalogLineageSummary, CatalogMutationError, CatalogPage,
    CatalogPointReadLimit, CatalogReadError, CatalogRecencyCursor, CatalogRevision, CatalogRow,
    CatalogRowExpectation, CatalogSearchFields, CatalogSourceRevisions, CatalogState,
    CatalogStoredRow, CatalogTitleCandidate, CatalogTitleFacts, CatalogTitleSource,
    CatalogValueError, MarkCatalogRowStale, PublishCatalogRow,
};
pub use durable_job::{
    AdmitBranchHandoffJob, BranchHandoffCheckpoint, BranchHandoffJobAdmission,
    BranchHandoffJobLifecycle, BranchHandoffJobRecord, BranchHandoffJobState,
    CompleteResolvingTurn, DiscussionContextDigest, DiscussionContextOwnerId,
    DurableJobMutationError, DurableJobState, DurableJobValueError,
    HANDOFF_FAILURE_DETAIL_MAX_BYTES, HandoffFailureEvidence, HandoffFailureKind,
    LatestBranchHandoffAttempt, ParentCasIdentity, ParentHandoffIdentity, ParentQueueOrdinal,
    RESOLUTION_TEXT_MAX_BYTES, RecordParentCasAcceptance, RecordRetryableHandoffFailure,
    RecordTerminalHandoffFailure, ResolutionAttemptOrdinal, ResolutionRequestAdmission,
    ResolutionRequestIdentity, ResolutionText, RetryBranchHandoff, StartParentHandoff,
    SucceedBranchHandoff, branch_handoff_job_id,
};
pub use runtime_root::{
    AddConfiguredRoot, CreateRuntimeWithHomeRoot, RootActivityUpdate, RootRecord, RootRegistration,
    RuntimeRecord, RuntimeRegistration, RuntimeRootMutationError, RuntimeRootState,
    SetRootAvailability, SetRuntimeAvailability,
};
pub use session::{
    ActivateRestoringClaim, BeginSessionRestore, CreateClaimedWindow, InitializeThreadlessWindow,
    MAX_RESTORABLE_WINDOWS, MarkOrderlyExit, MinimalSessionBootstrap, RememberedTarget,
    RemoveSessionWindow, ReplaceWindowClaim, SESSION_HEADER_V1_BYTES, SESSION_WINDOW_V1_BYTES,
    SessionExitIntent, SessionHeader, SessionMutationError, SessionReadError, SessionState,
    SessionWindowRecord, SessionWindowReference, ThreadClaimRecord, ThreadClaimState,
    UpdateWindowPlacement, WindowClaimSelection,
};
pub use settings::{
    ApplySettings, ApplySettingsError, ExpectedSettingRevision, SettingKey, SettingRecord,
    SettingSchemaVersion, SettingUpdate, SettingValue, SettingValueError, SettingsMutationError,
    SettingsState,
};
pub use state::{
    BerylState, BerylStateBootstrap, BerylStateReacquireError, BerylStateRegistrationError,
    StatePage,
};
pub use thread_metadata::{
    ArchiveBranchDiscussion, CreateThreadMetadata, SetGeneratedTitle, ThreadMetadataKind,
    ThreadMetadataMutationError, ThreadMetadataRecord, ThreadMetadataState, UpdateThreadActivity,
    UpdateTokenUsage,
};
pub use value::{
    AvailabilitySnapshot, GeneratedTitle, RecordRevision, ThreadActivitySummary,
    ThreadArchiveState, TokenUsageBreakdown, TokenUsageSnapshot, UnixMillis, ValueError,
};

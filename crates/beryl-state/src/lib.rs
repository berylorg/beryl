//! Typed durable schemas and commands for Beryl-owned application state.
//!
//! `beryl-state` registers exact Beryl-owned runtime/root, session/window/claim, settings,
//! durable-job, catalog, and asset-reference domains through
//! [`beryl_home_store`] without receiving a database, keyspace, batch, or
//! encoded record. Callers admit external facts before building short
//! revision-checked contributions. Successful command receipts are projected
//! through each opaque domain state, which rejects obsolete home generations
//! without exposing the underlying storage-domain handle.
//!
//! # Catalog Projection
//!
//! Catalog rows are rebuildable Beryl projections. Callers obtain exact runtime/root facts through
//! [`RuntimeRootState::catalog_source`], current present-or-absent claim facts through
//! [`SessionState::thread_claim_catalog_source`], and one resolved public Syndic summary. The
//! matching source validators and [`PublishCatalogRow`] contribution belong in the same
//! [`beryl_home_store::HomeCommand`]. Beryl preserves visible strings byte-for-byte while deriving
//! fixed-profile search fields; [`CatalogNormalizedQuery`] applies that same profile at query
//! admission.
//!
//! ```
//! use beryl_model::{
//!     AdmittedHostPath, Availability, PathFlavor, ProjectionRevision, RootId, RuntimeId,
//!     SyndicThreadId,
//! };
//! use beryl_state::{
//!     CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimSummary,
//!     CatalogExecutionSummary, CatalogFacts, CatalogLineageSummary, CatalogNormalizedQuery,
//!     CatalogResolvedTitle, CatalogRowExpectation, CatalogSourceRevisions, PublishCatalogRow,
//!     RecordRevision, UnixMillis,
//! };
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let visible_title = "Stra\u{00df}e";
//! let title = CatalogResolvedTitle::history_derived(visible_title)?;
//! let execution = CatalogExecutionSummary::new(
//!     RuntimeId::from_bytes([1; 16]),
//!     RootId::from_bytes([2; 16]),
//!     "Host",
//!     AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Codex\codex.exe")?,
//!     AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Work\beryl")?,
//!     CatalogAvailabilitySummary::new(Availability::Available, Availability::Available),
//! )?;
//! let facts = CatalogFacts::new(
//!     title,
//!     execution,
//!     CatalogArchiveSummary::Ordinary,
//!     UnixMillis::new(10),
//!     true,
//!     CatalogClaimSummary::Unclaimed,
//!     CatalogLineageSummary::TopLevel,
//! )?;
//! assert_eq!(facts.title().text(), Some(visible_title));
//! assert_eq!(facts.search().title(), "strasse");
//! assert_eq!(
//!     CatalogNormalizedQuery::new("STRASSE")?.as_str(),
//!     facts.search().title(),
//! );
//!
//! let admitted_sources = CatalogSourceRevisions::new(
//!     ProjectionRevision::new(7)?,
//!     RecordRevision::new(4)?,
//!     RecordRevision::new(9)?,
//!     None,
//! );
//! let publication = PublishCatalogRow::new(
//!     SyndicThreadId::from_bytes([3; 16]),
//!     CatalogRowExpectation::Missing,
//!     admitted_sources,
//!     facts,
//! )?;
//! let _ = publication;
//! # Ok(())
//! # }
//! ```
//!
//! # Asset Reference Sets
//!
//! Marker-bearing content is staged through fixed-capacity append pages and
//! sealed before a compact durable owner head may select it. The begin command
//! emits opaque staging authority; a sealed manifest is readable only with its
//! complete sealed proof.
//!
//! ```
//! use beryl_model::{
//!     AssetReferenceSetId, SealedContentMarkerSummary, SyndicContentDigest, SyndicContentId,
//!     content_marker_digest_seed,
//! };
//! use beryl_state::BeginAssetReferenceSet;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let set_id = AssetReferenceSetId::from_bytes([1; 16]);
//! let source = SealedContentMarkerSummary::new(
//!     SyndicContentId::from_bytes([2; 16]),
//!     SyndicContentDigest::from_bytes([3; 32]),
//!     content_marker_digest_seed(),
//!     0,
//!     None,
//! )?;
//! let build = BeginAssetReferenceSet::new(set_id, source);
//! let staging = build.staging_authority();
//! assert_eq!(set_id.as_bytes(), &[1; 16]);
//! let _ = (build, staging);
//! # Ok(())
//! # }
//! ```
//!
//! # Example
//!
//! ```no_run
//! use beryl_home_store::{
//!     CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
//! };
//! use beryl_model::{
//!     AdmittedHostPath, Availability, PathFlavor, RootId, RuntimeId, RuntimeMode,
//!     RuntimeNativePath,
//! };
//! use beryl_state::{
//!     AvailabilitySnapshot, BerylStateBootstrap, CreateRuntimeWithHomeRoot, RootRegistration,
//!     RuntimeRegistration, UnixMillis,
//! };
//!
//! # fn example() -> Result<CommandOutcome, Box<dyn std::error::Error>> {
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
//! let outcome = home.execute(command);
//! if let CommandOutcome::Committed {
//!     receipt,
//!     later_failure,
//! } = &outcome {
//!     assert!(later_failure.is_none());
//!     assert_eq!(
//!         state.runtime_roots().committed_revision(&home, receipt)?,
//!         Some(expected_domain.checked_next()?),
//!     );
//! }
//! Ok(outcome)
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
mod value;

pub use asset::{
    AppendAssetReferencePage, AssetAdmissionError, AssetDimensions, AssetLabelDisposition,
    AssetMediaType, AssetMetadataContribution, AssetMetadataRecord, AssetMutationError, AssetOwner,
    AssetOwnerHeadAssertion, AssetOwnerHeadExpectation, AssetOwnerHeadRecord, AssetOwnerHeadUpdate,
    AssetOwnerHeadUpdateError, AssetOwnerHeadValidationError, AssetReadError,
    AssetReferenceEntryRecord, AssetReferenceOrdinal, AssetReferencePageEntry,
    AssetReferencePageError, AssetReferenceSetBuildProof, AssetReferenceSetLifecycle,
    AssetReferenceSetManifest, AssetReferenceSetStagingAuthority, AssetSidecarState, AssetState,
    AssetValueError, BeginAssetReferenceSet, PublishAssetMetadata, SealAssetReferenceSet,
    UpdateAssetOwnerHeads, ValidateAssetOwnerHeads, ASSET_OWNER_HEAD_UPDATE_MAX_ENTRIES,
    ASSET_REFERENCE_PAGE_MAX_ENTRIES, ASSET_REFERENCE_PAGE_MAX_STORED_BYTES,
};
pub use catalog::{
    CatalogArchiveSummary, CatalogAvailabilitySummary, CatalogClaimKind, CatalogClaimSummary,
    CatalogExecutionSummary, CatalogFacts, CatalogFreshness, CatalogLineageSummary,
    CatalogMutationError, CatalogNormalizationProfile, CatalogNormalizedQuery, CatalogPage,
    CatalogPointReadLimit, CatalogReadError, CatalogRecencyCursor, CatalogResolvedTitle,
    CatalogRevision, CatalogRow, CatalogRowExpectation, CatalogSearchFields,
    CatalogSourceRevisions, CatalogState, CatalogTitleSource, CatalogValueError,
    MarkCatalogRowStale, PublishCatalogRow, CATALOG_MAX_STORED_RECENCY_BYTES,
    CATALOG_NORMALIZATION_PROFILE, CATALOG_QUERY_MAX_BYTES,
};
pub use durable_job::{
    branch_handoff_job_id, AdmitBranchHandoffJob, BranchHandoffCheckpoint,
    BranchHandoffJobAdmission, BranchHandoffJobLifecycle, BranchHandoffJobRecord,
    BranchHandoffJobState, CompleteResolvingTurn, DiscussionContextDigest,
    DiscussionContextOwnerId, DurableJobMutationError, DurableJobState, DurableJobValueError,
    HandoffFailureEvidence, HandoffFailureKind, LatestBranchHandoffAttempt, ParentCasIdentity,
    ParentHandoffIdentity, ParentQueueOrdinal, RecordParentCasAcceptance,
    RecordRetryableHandoffFailure, RecordTerminalHandoffFailure, ResolutionAttemptOrdinal,
    ResolutionRequestAdmission, ResolutionRequestIdentity, ResolutionText, RetryBranchHandoff,
    StartParentHandoff, SucceedBranchHandoff, HANDOFF_FAILURE_DETAIL_MAX_BYTES,
    RESOLUTION_TEXT_MAX_BYTES,
};
pub use runtime_root::{
    AddConfiguredRoot, CreateRuntimeWithHomeRoot, RootActivityUpdate, RootRecord, RootRegistration,
    RuntimeRecord, RuntimeRegistration, RuntimeRootCatalogSource, RuntimeRootCatalogSourceError,
    RuntimeRootMutationError, RuntimeRootState, SetRootAvailability, SetRuntimeAvailability,
};
pub use session::{
    ActivateRestoringClaim, BeginSessionRestore, CreateClaimedWindow, InitializeThreadlessWindow,
    MarkOrderlyExit, MinimalSessionBootstrap, RememberedTarget, RemoveSessionWindow,
    ReplaceWindowClaim, SessionExitIntent, SessionHeader, SessionMutationError, SessionReadError,
    SessionState, SessionWindowRecord, SessionWindowReference, ThreadClaimCatalogSource,
    ThreadClaimCatalogSourceError, ThreadClaimRecord, ThreadClaimState, UpdateWindowPlacement,
    WindowClaimSelection, MAX_RESTORABLE_WINDOWS, SESSION_HEADER_V1_BYTES, SESSION_WINDOW_V1_BYTES,
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
pub use value::{AvailabilitySnapshot, RecordRevision, UnixMillis, ValueError};

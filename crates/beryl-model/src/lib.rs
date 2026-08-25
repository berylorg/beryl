//! Shared pure-data identities and values used across Beryl packages.
//!
//! The crate validates bounded value shapes but performs no identity
//! generation, filesystem observation, persistence, process work, or protocol
//! I/O. Stored record codecs remain owned by their storage packages.
//!
//! # Example
//!
//! ```
//! use beryl_model::{
//!     CasConversationToolProfile, CasNativeTurnCount, ExecutionBinding, ImageLabelOrdinal,
//!     PathFlavor, ProviderObservationId, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
//! };
//!
//! let runtime_id = RuntimeId::from_bytes([1; 16]);
//! let root_id = RootId::from_bytes([2; 16]);
//! let mode = RuntimeMode::wsl("Ubuntu-24.04")?;
//! let root_path = RuntimeNativePath::from_admitted(
//!     mode,
//!     PathFlavor::Posix,
//!     "/home/operator/project",
//! )?;
//! let binding = ExecutionBinding::new(runtime_id, root_id, root_path);
//!
//! assert_eq!(binding.runtime_id(), runtime_id);
//! assert_eq!(CasNativeTurnCount::ZERO.checked_next()?.get(), 1);
//! assert_eq!(CasConversationToolProfile::v1([3; 32]).digest(), [3; 32]);
//! assert_eq!(ImageLabelOrdinal::new(27)?.to_string(), "AA");
//! let observation = ProviderObservationId::from_bytes([4; 16]);
//! assert_eq!(observation.as_bytes(), &[4; 16]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Recovery preflight and replay share the same constant-resident digest logic:
//!
//! ```
//! use beryl_model::{RecoveryItemSequenceAccumulator, RecoveryItemSequenceRole};
//!
//! let mut sequence = RecoveryItemSequenceAccumulator::new(1, 5);
//! sequence.begin_item(1, RecoveryItemSequenceRole::UserInputText, 5)?;
//! sequence.update_text(b"hello")?;
//! sequence.finish_item()?;
//! let digest = sequence.finish()?;
//! assert_ne!(digest.as_bytes(), &[0; 32]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

mod asset;
mod availability;
mod ids;
mod placement;
mod provenance;
mod recovery;
mod revision;
mod runtime;
mod syndic;

pub use asset::{
    AssetId, AssetIdentityVersion, AssetProofError, AssetReferenceSetDigest, AssetReferenceSetId,
    DraftMarkerCommitmentV1, FirstAcceptancePromotionSuccessorV1, ImageLabelOrdinal,
    ImageLabelOrdinalError, OrderedMarkerAssetSummaryV1, SealedAssetReferenceSetProof,
    SealedContentMarkerSummary, SequentialMarkerSummaryV1, advance_ordered_marker_asset_digest,
    advance_sequential_marker_digest, ordered_marker_asset_digest_seed,
    sequential_marker_digest_seed,
};
pub use availability::{Availability, UnavailableReason};
pub use ids::{
    BerylHomeId, CommandId, IdempotencyKey, IdentityParseError, JobId, ProviderObservationId,
    ResolutionIntentId, RootId, RuntimeId, SyndicAcceptedInputId, SyndicContentId, SyndicDraftId,
    SyndicDraftMarkerId, SyndicExecutionSnapshotId, SyndicItemId, SyndicProjectionId,
    SyndicResourceId, SyndicRetryRecordId, SyndicThreadId, SyndicTurnId, VirtualDesktopId,
    WindowId,
};
pub use placement::{
    MonitorHint, MonitorId, PlacementError, WindowBounds, WindowDisplayState, WindowPlacement,
};
pub use provenance::{
    CasItemId, CasThreadId, CasTurnId, DynamicToolCallId, DynamicToolName, Provenance,
};
pub use recovery::{
    RecoveryItemSequenceAccumulator, RecoveryItemSequenceError, RecoveryItemSequenceRole,
};
pub use revision::{
    AcceptedInputRevision, BindingRevision, ClaimRevision, ContentRevision, DomainRevision,
    DraftRevision, HomeRevision, InputGateRevision, JobRevision, ProjectionRevision, RevisionError,
    SessionRevision, ThreadRevision,
};
pub use runtime::{
    AdmittedHostPath, ExecutionBinding, PathFlavor, RuntimeMode, RuntimeNativePath, ValueError,
    WslDistributionName,
};
pub use syndic::{
    CasConversationToolProfile, CasConversationToolProfileVersion, CasGenerationError,
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasNativeTurnCount,
    CasNativeTurnCountError, CasProcessGeneration, DiscussionContextDigest,
    DiscussionContextOwnerId, RecoveryItemSequenceDigest, SyndicContentDigest, SyndicPathDigest,
};

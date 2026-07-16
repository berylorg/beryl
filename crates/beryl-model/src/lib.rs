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
//!     CasConversationToolProfile, CasNativeTurnCount, ExecutionBinding, PathFlavor, RootId,
//!     RuntimeId, RuntimeMode, RuntimeNativePath,
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
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

mod asset;
mod availability;
mod ids;
mod placement;
mod provenance;
mod revision;
mod runtime;
mod syndic;

pub use asset::{AssetId, AssetIdentityVersion};
pub use availability::{Availability, UnavailableReason};
pub use ids::{
    BerylHomeId, CommandId, IdempotencyKey, IdentityParseError, JobId, ResolutionIntentId, RootId,
    RuntimeId, SyndicAcceptedInputId, SyndicContentId, SyndicDraftId, SyndicDraftMarkerId,
    SyndicExecutionSnapshotId, SyndicItemId, SyndicProjectionId, SyndicResourceId,
    SyndicRetryRecordId, SyndicThreadId, SyndicTurnId, VirtualDesktopId, WindowId,
};
pub use placement::{
    MonitorHint, MonitorId, PlacementError, WindowBounds, WindowDisplayState, WindowPlacement,
};
pub use provenance::{
    CasItemId, CasThreadId, CasTurnId, DynamicToolCallId, DynamicToolName, Provenance,
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

//! Fjall-backed durable storage for Syndic conversation history.
//!
//! `syndic-storage` owns the typed persistence boundary for captured
//! conversation views, turns, source events, canonical items, transcript
//! projections, resources, revisions, cursors, recovery markers, and CAS
//! projection bindings.
//!
//! Callers provide data that has already crossed the owning redaction and
//! execution-policy boundary. This crate stores and reads that data; it does not
//! call Codex App Server, OpenAI APIs, model providers, auth services, or GPUI
//! rendering code.
//!
//! ```no_run
//! use syndic_storage::{
//!     ConversationRecord, ConversationId, HistoryState, ProviderRevision,
//!     StoreOpenOptions, SyndicStore, SyndicWriteBatch, ThreadViewId,
//! };
//!
//! # fn main() -> syndic_storage::Result<()> {
//! let dir = std::env::temp_dir().join(format!(
//!     "syndic-storage-doc-example-{}",
//!     std::process::id()
//! ));
//! let _ = std::fs::remove_dir_all(&dir);
//! let store = SyndicStore::open(&dir, StoreOpenOptions::default())?;
//!
//! let conversation = ConversationRecord {
//!     id: ConversationId::from("conversation-1"),
//!     view_id: ThreadViewId::from("view-1"),
//!     parent_view_id: None,
//!     branch_source_turn_id: None,
//!     title: Some("Captured work".to_string()),
//!     created_at_ms: 1,
//!     updated_at_ms: 1,
//!     current_revision: ProviderRevision(1),
//!     source: None,
//!     history_state: HistoryState::Complete,
//! };
//!
//! store.commit(SyndicWriteBatch::new().put_conversation(conversation))?;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]

mod batch;
mod error;
mod ids;
mod keys;
mod records;
mod store;

pub use batch::{CommitSummary, SyndicWriteBatch, SyndicWriteOperation};
pub use error::{Result, StorageError};
pub use ids::{
    CasProjectionBindingId, ConversationId, CursorId, ItemId, ProjectionRecordId, RecoveryMarkerId,
    ResourceId, SourceEventId, ThreadViewId, TranscriptViewRecordId, TurnId,
};
pub use records::{
    ByteRange, CanonicalItemKind, CanonicalItemRecord, CanonicalItemVisibility,
    CasProjectionBindingRecord, CasProjectionBindingStatus, CasProjectionBindingSummary,
    ConversationRecord, ConversationTitleCandidate, ConversationTitleCandidateSource,
    ConversationViewBranchSummary, ConversationViewSummary, CursorRecord, ExternalSourceMetadata,
    HistoryIncompleteReason, HistoryState, ProjectionPayload, ProjectionRecord,
    ProjectionRecordKind, ProjectionStatus, ProviderOperationKind, ProviderRevision,
    RecoveryMarkerKind, RecoveryMarkerRecord, ResourceKind, ResourceMetadataRecord,
    ResourceRangeResponse, ResourceRecord, ResourceState, SourceEventPayload, SourceEventRecord,
    SourceEventVisibility, SyndicSourceProvenance, TerminalError, TranscriptNarrativeKind,
    TranscriptPage, TranscriptPageAnchor, TranscriptPageDirection, TranscriptViewPosition,
    TranscriptViewRecord, TranscriptViewRecordSummary, TurnKind, TurnRecord, TurnStatus,
};
pub use store::{
    MAX_CONVERSATION_SUMMARY_READ_LIMIT, MAX_RESOURCE_RANGE_BYTES, MAX_SOURCE_EVENT_PAYLOAD_BYTES,
    MAX_SOURCE_EVENT_READ_LIMIT, MAX_TRANSCRIPT_PAGE_LIMIT, StoreOpenOptions, SyndicStore,
};

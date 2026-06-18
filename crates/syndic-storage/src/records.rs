use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    CasProjectionBindingId, ConversationId, CursorId, ItemId, ProjectionRecordId, RecoveryMarkerId,
    ResourceId, SourceEventId, ThreadViewId, TranscriptViewRecordId, TurnId,
};

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct ProviderRevision(pub u64);

impl ProviderRevision {
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct TranscriptViewPosition(pub u64);

impl TranscriptViewPosition {
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn to_range(self) -> std::ops::Range<u64> {
        self.start..self.end
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExternalSourceMetadata {
    pub provider: String,
    pub runtime_target: Option<String>,
    pub external_thread_id: Option<String>,
    pub external_turn_id: Option<String>,
    pub external_item_id: Option<String>,
    pub external_event_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConversationRecord {
    pub id: ConversationId,
    pub view_id: ThreadViewId,
    #[serde(default)]
    pub parent_view_id: Option<ThreadViewId>,
    #[serde(default)]
    pub branch_source_turn_id: Option<TurnId>,
    pub title: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub current_revision: ProviderRevision,
    pub source: Option<ExternalSourceMetadata>,
    pub history_state: HistoryState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HistoryState {
    Complete,
    Incomplete {
        reason: HistoryIncompleteReason,
        detail: Option<String>,
    },
    Unavailable {
        reason: HistoryIncompleteReason,
        detail: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum HistoryIncompleteReason {
    NotCaptured,
    MissedEvents,
    StreamLost,
    StorageFailure,
    UnknownTerminalState,
    ProjectionStale,
    ResourceMissing,
    Other(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConversationViewSummary {
    pub conversation_id: ConversationId,
    pub view_id: ThreadViewId,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub current_revision: ProviderRevision,
    pub source: Option<ExternalSourceMetadata>,
    pub history_state: HistoryState,
    pub title_candidates: Vec<ConversationTitleCandidate>,
    pub branch: Option<ConversationViewBranchSummary>,
    pub latest_transcript_record: Option<TranscriptViewRecordSummary>,
    pub cas_projection_binding: Option<CasProjectionBindingSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConversationTitleCandidate {
    pub title: String,
    pub source: ConversationTitleCandidateSource,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ConversationTitleCandidateSource {
    ConversationRecord,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConversationViewBranchSummary {
    pub parent_view_id: ThreadViewId,
    pub source_turn_id: Option<TurnId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TranscriptViewRecordSummary {
    pub id: TranscriptViewRecordId,
    pub position: TranscriptViewPosition,
    pub narrative_kind: TranscriptNarrativeKind,
    pub turn_id: Option<TurnId>,
    pub item_id: Option<ItemId>,
    pub projection_id: ProjectionRecordId,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CasProjectionBindingSummary {
    pub id: CasProjectionBindingId,
    pub binding_revision: u64,
    pub selected_path_revision: ProviderRevision,
    pub selected_path_digest: Option<String>,
    pub established_at_ms: u64,
    pub status: CasProjectionBindingStatus,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TurnRecord {
    pub id: TurnId,
    pub conversation_id: ConversationId,
    pub view_id: ThreadViewId,
    pub parent_turn_id: Option<TurnId>,
    pub kind: TurnKind,
    pub status: TurnStatus,
    pub source: Option<ExternalSourceMetadata>,
    pub created_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub completed_at_ms: Option<u64>,
    pub terminal_error: Option<TerminalError>,
    pub projection_revision: ProviderRevision,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TurnKind {
    User,
    ProviderOperation(ProviderOperationKind),
    Other(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProviderOperationKind {
    ContextCompaction,
    FreshProjectionMaterialization,
    Maintenance,
    Other(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TurnStatus {
    Pending,
    Running,
    Completed,
    Failed {
        reason: String,
    },
    Incomplete {
        reason: HistoryIncompleteReason,
        detail: Option<String>,
    },
    Interrupted,
    Aborted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TerminalError {
    pub code: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourceEventRecord {
    pub id: SourceEventId,
    pub turn_id: TurnId,
    pub sequence: u64,
    pub captured_at_ms: u64,
    pub source: ExternalSourceMetadata,
    pub visibility: SourceEventVisibility,
    pub payload: SourceEventPayload,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SourceEventVisibility {
    TranscriptVisible,
    CanonicalOnly,
    Operational,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourceEventPayload {
    pub kind: String,
    pub body: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CanonicalItemRecord {
    pub id: ItemId,
    pub turn_id: TurnId,
    pub source_event_id: SourceEventId,
    pub kind: CanonicalItemKind,
    pub visibility: CanonicalItemVisibility,
    pub source: Option<ExternalSourceMetadata>,
    pub payload: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CanonicalItemKind {
    UserInput,
    AssistantMessage,
    Operational,
    GeneratedMedia,
    ResourceReference,
    ProviderMetadata,
    Other(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CanonicalItemVisibility {
    Transcript,
    CanonicalOnly,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SyndicSourceProvenance {
    pub view_id: ThreadViewId,
    pub position: Option<TranscriptViewPosition>,
    pub turn_id: Option<TurnId>,
    pub item_id: Option<ItemId>,
    pub source_event_id: Option<SourceEventId>,
    pub projection_id: Option<ProjectionRecordId>,
    pub resource_id: Option<ResourceId>,
    pub source_range: Option<ByteRange>,
    pub resource_range: Option<ByteRange>,
    pub copy_source_range: Option<ByteRange>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectionRecord {
    pub id: ProjectionRecordId,
    pub view_id: ThreadViewId,
    pub turn_id: TurnId,
    pub item_id: ItemId,
    pub revision: ProviderRevision,
    pub kind: ProjectionRecordKind,
    pub status: ProjectionStatus,
    pub payload: ProjectionPayload,
    pub provenance: SyndicSourceProvenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProjectionRecordKind {
    TextChunk,
    ResourceReference,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProjectionStatus {
    Current,
    Stale {
        reason: HistoryIncompleteReason,
        detail: Option<String>,
    },
    Incomplete {
        reason: HistoryIncompleteReason,
        detail: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum ProjectionPayload {
    Text {
        text: String,
    },
    ResourceReference {
        resource_id: ResourceId,
        resource_kind: ResourceKind,
        label: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TranscriptViewRecord {
    pub id: TranscriptViewRecordId,
    pub view_id: ThreadViewId,
    pub position: TranscriptViewPosition,
    pub projection_id: ProjectionRecordId,
    pub narrative_kind: TranscriptNarrativeKind,
    pub provenance: SyndicSourceProvenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TranscriptNarrativeKind {
    UserInput,
    UserMedia,
    AssistantCommentary,
    AssistantFinalAnswer,
    AssistantGeneratedMedia,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResourceRecord {
    pub metadata: ResourceMetadataRecord,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResourceMetadataRecord {
    pub id: ResourceId,
    pub revision: ProviderRevision,
    pub kind: ResourceKind,
    pub state: ResourceState,
    pub media_type: Option<String>,
    pub byte_len: u64,
    pub digest: Option<String>,
    pub line_count: Option<u64>,
    pub row_count: Option<u64>,
    pub column_count: Option<u64>,
    pub preview_range: Option<ByteRange>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ResourceKind {
    Code,
    Table,
    Image,
    Attachment,
    GeneratedImage,
    Other(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ResourceState {
    Ready,
    Missing {
        reason: HistoryIncompleteReason,
        detail: Option<String>,
    },
    Rejected {
        reason: String,
        message: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResourceRangeResponse {
    pub resource_id: ResourceId,
    pub revision: ProviderRevision,
    pub kind: ResourceKind,
    pub range: ByteRange,
    pub bytes: Vec<u8>,
    pub complete: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TranscriptPage {
    pub view_id: ThreadViewId,
    pub revision: ProviderRevision,
    pub records: Vec<TranscriptViewRecord>,
    pub previous_cursor: Option<CursorId>,
    pub next_cursor: Option<CursorId>,
    pub at_start: bool,
    pub at_end: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TranscriptPageAnchor {
    Start,
    End,
    Cursor(CursorId),
    Position(TranscriptViewPosition),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TranscriptPageDirection {
    Forward,
    Backward,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CursorRecord {
    pub id: CursorId,
    pub view_id: ThreadViewId,
    pub revision: ProviderRevision,
    pub position: Option<TranscriptViewPosition>,
    pub offset_hint: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RecoveryMarkerRecord {
    pub id: RecoveryMarkerId,
    pub kind: RecoveryMarkerKind,
    pub view_id: Option<ThreadViewId>,
    pub turn_id: Option<TurnId>,
    pub created_at_ms: u64,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum RecoveryMarkerKind {
    WriteBatchStarted,
    SourceIngestionInterrupted,
    ProjectionRebuildPending,
    ResourcePayloadIncomplete,
    CrashRecoveryRequired,
    Other(String),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CasProjectionBindingRecord {
    pub id: CasProjectionBindingId,
    pub view_id: ThreadViewId,
    pub binding_revision: u64,
    pub selected_path_revision: ProviderRevision,
    pub selected_path_digest: Option<String>,
    pub established_at_ms: u64,
    pub status: CasProjectionBindingStatus,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum CasProjectionBindingStatus {
    Valid {
        runtime_target: String,
        cas_thread_id: String,
        lineage_proof: String,
    },
    Active {
        runtime_target: String,
        cas_thread_id: String,
        cas_turn_id: Option<String>,
        execution_snapshot_id: String,
        accepted_input_id: ItemId,
        started_at_ms: u64,
        lineage_proof: String,
    },
    Stale {
        old_cas_thread_id: Option<String>,
        reason: String,
    },
    Unbound {
        reason: Option<String>,
    },
}

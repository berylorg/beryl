//! Beryl-facing provider contract for admitting Syndic transcript data.
//!
//! This module is intentionally a contract only. Implementations live behind
//! this boundary, while transcript residency consumes these request and response
//! shapes before turning resident Syndic data into presentation data.

use std::ops::Range;

pub(crate) type TranscriptProviderResult =
    Result<TranscriptProviderResponse, TranscriptProviderError>;

pub(crate) trait SyndicTranscriptProvider {
    fn handle_request(&mut self, request: TranscriptProviderRequest) -> TranscriptProviderResult;
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ProviderRequestId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ProviderRevision(pub(crate) u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptProviderRequest {
    pub(crate) id: ProviderRequestId,
    pub(crate) kind: TranscriptProviderRequestKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptProviderRequestKind {
    ReadViewPage(TranscriptViewPageRequest),
    ReadProjectionRecords(ProjectionRecordsRequest),
    ReadResourceMetadata(ResourceMetadataRequest),
    ReadResourceRange(ResourceRangeRequest),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptProviderResponse {
    pub(crate) request_id: ProviderRequestId,
    pub(crate) kind: TranscriptProviderResponseKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptProviderResponseKind {
    ViewPage(TranscriptViewPage),
    ProjectionRecords(ProjectionRecordSet),
    ResourceMetadata(ResourceMetadata),
    ResourceRange(ResourceRangeResponse),
    Rejected(TranscriptProviderRejection),
    Stale(TranscriptProviderStale),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptViewPageRequest {
    pub(crate) view_id: TranscriptViewId,
    pub(crate) anchor: TranscriptPageAnchor,
    pub(crate) direction: TranscriptPageDirection,
    pub(crate) limit: usize,
    pub(crate) observed_revision: Option<ProviderRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptPageAnchor {
    Start,
    End,
    Cursor(TranscriptCursor),
    Position(TranscriptViewPosition),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptPageDirection {
    Forward,
    Backward,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptViewPage {
    pub(crate) view_id: TranscriptViewId,
    pub(crate) revision: ProviderRevision,
    pub(crate) records: Vec<TranscriptViewRecord>,
    pub(crate) previous_cursor: Option<TranscriptCursor>,
    pub(crate) next_cursor: Option<TranscriptCursor>,
    pub(crate) at_start: bool,
    pub(crate) at_end: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptViewRecord {
    pub(crate) id: TranscriptViewRecordId,
    pub(crate) position: TranscriptViewPosition,
    pub(crate) projection_id: ProjectionRecordId,
    pub(crate) narrative_kind: TranscriptNarrativeKind,
    pub(crate) provenance: SyndicSourceProvenance,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TranscriptViewId(pub(crate) String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TranscriptCursor(pub(crate) String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TranscriptViewRecordId(pub(crate) String);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TranscriptViewPosition(pub(crate) u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptNarrativeKind {
    UserInput,
    UserMedia,
    AssistantCommentary,
    AssistantFinalAnswer,
    AssistantGeneratedMedia,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectionRecordsRequest {
    pub(crate) view_id: TranscriptViewId,
    pub(crate) projection_ids: Vec<ProjectionRecordId>,
    pub(crate) observed_revision: Option<ProviderRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectionRecordSet {
    pub(crate) view_id: TranscriptViewId,
    pub(crate) revision: ProviderRevision,
    pub(crate) records: Vec<ProjectionRecord>,
    pub(crate) rejections: Vec<TranscriptProviderRejection>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectionRecord {
    pub(crate) id: ProjectionRecordId,
    pub(crate) revision: ProviderRevision,
    pub(crate) kind: ProjectionRecordKind,
    pub(crate) payload: ProjectionPayload,
    pub(crate) provenance: SyndicSourceProvenance,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ProjectionRecordId(pub(crate) String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProjectionRecordKind {
    TextChunk,
    ResourceReference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProjectionPayload {
    Text {
        text: String,
    },
    ResourceReference {
        resource_id: ResourceId,
        resource_kind: ResourceKind,
        label: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceMetadataRequest {
    pub(crate) resource_id: ResourceId,
    pub(crate) observed_revision: Option<ProviderRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceRangeRequest {
    pub(crate) resource_id: ResourceId,
    pub(crate) range: Range<u64>,
    pub(crate) observed_revision: Option<ProviderRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceMetadata {
    pub(crate) resource_id: ResourceId,
    pub(crate) revision: ProviderRevision,
    pub(crate) kind: ResourceKind,
    pub(crate) media_type: Option<String>,
    pub(crate) byte_len: u64,
    pub(crate) digest: Option<String>,
    pub(crate) line_count: Option<u64>,
    pub(crate) row_count: Option<u64>,
    pub(crate) column_count: Option<u64>,
    pub(crate) preview_range: Option<Range<u64>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceRangeResponse {
    pub(crate) resource_id: ResourceId,
    pub(crate) revision: ProviderRevision,
    pub(crate) kind: ResourceKind,
    pub(crate) range: Range<u64>,
    pub(crate) bytes: Vec<u8>,
    pub(crate) complete: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ResourceId(pub(crate) String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResourceKind {
    Code,
    Table,
    Image,
    Attachment,
    GeneratedImage,
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SyndicSourceProvenance {
    pub(crate) view_id: TranscriptViewId,
    pub(crate) position: Option<TranscriptViewPosition>,
    pub(crate) turn_id: Option<SyndicTurnId>,
    pub(crate) item_id: Option<SyndicItemId>,
    pub(crate) projection_id: Option<ProjectionRecordId>,
    pub(crate) resource_id: Option<ResourceId>,
    pub(crate) source_range: Option<Range<u64>>,
    pub(crate) resource_range: Option<Range<u64>>,
    pub(crate) copy_source_range: Option<Range<u64>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SyndicTurnId(pub(crate) String);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SyndicItemId(pub(crate) String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptProviderStale {
    pub(crate) target: TranscriptProviderTarget,
    pub(crate) observed_revision: Option<ProviderRevision>,
    pub(crate) current_revision: ProviderRevision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TranscriptProviderRejection {
    pub(crate) target: TranscriptProviderTarget,
    pub(crate) reason: TranscriptProviderRejectionReason,
    pub(crate) revision: Option<ProviderRevision>,
    pub(crate) message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptProviderTarget {
    Request(ProviderRequestId),
    View(TranscriptViewId),
    Cursor(TranscriptCursor),
    ProjectionRecord(ProjectionRecordId),
    Resource(ResourceId),
    ResourceRange {
        resource_id: ResourceId,
        range: Range<u64>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptProviderRejectionReason {
    MissingView,
    MissingCursor,
    MissingProjectionRecord,
    MissingResource,
    UnsupportedResourceKind,
    RangeOutOfBounds,
    BudgetExceeded,
    PolicyDenied,
    InvalidRequest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TranscriptProviderError {
    Unavailable { reason: String },
    Interrupted { reason: String },
    Internal { reason: String },
}

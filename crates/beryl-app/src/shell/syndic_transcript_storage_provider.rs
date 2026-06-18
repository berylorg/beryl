use std::{ops::Range, path::Path};

use syndic_storage::{self as storage, StoreOpenOptions, SyndicStore};

use super::syndic_transcript::{
    ProjectionPayload, ProjectionRecord, ProjectionRecordId, ProjectionRecordKind,
    ProjectionRecordSet, ProjectionRecordsRequest, ProviderRequestId, ProviderRevision, ResourceId,
    ResourceKind, ResourceMetadata, ResourceMetadataRequest, ResourceRangeRequest,
    ResourceRangeResponse, SyndicItemId, SyndicSourceProvenance, SyndicTranscriptProvider,
    SyndicTurnId, TranscriptCursor, TranscriptNarrativeKind, TranscriptPageAnchor,
    TranscriptPageDirection, TranscriptProviderError, TranscriptProviderHistoryReason,
    TranscriptProviderHistoryState, TranscriptProviderRejection, TranscriptProviderRejectionReason,
    TranscriptProviderRequest, TranscriptProviderRequestKind, TranscriptProviderResponse,
    TranscriptProviderResponseKind, TranscriptProviderResult, TranscriptProviderStale,
    TranscriptProviderTarget, TranscriptViewId, TranscriptViewPage, TranscriptViewPageRequest,
    TranscriptViewPosition, TranscriptViewRecord, TranscriptViewRecordId,
};

pub(crate) struct StorageSyndicTranscriptProvider {
    store: SyndicStore,
}

impl StorageSyndicTranscriptProvider {
    pub(crate) fn open(path: impl AsRef<Path>) -> Result<Self, TranscriptProviderError> {
        let store = SyndicStore::open(path, StoreOpenOptions::default()).map_err(|error| {
            TranscriptProviderError::Unavailable {
                reason: error.to_string(),
            }
        })?;
        Ok(Self { store })
    }

    fn read_view_page(
        &self,
        request: TranscriptViewPageRequest,
    ) -> Result<TranscriptProviderResponseKind, storage::StorageError> {
        let view_id = to_storage_view_id(&request.view_id);
        let conversation = self.store.conversation_by_view(&view_id)?;
        let revision = match conversation.as_ref() {
            Some(conversation) => conversation.current_revision,
            None => self.store.current_revision(&view_id)?,
        };
        let history_state = conversation
            .as_ref()
            .map(|conversation| history_state_from_storage(&conversation.history_state))
            .unwrap_or_else(not_captured_history_state);

        if let Some(stale) = stale_for_observed_revision(
            request.observed_revision,
            from_storage_revision(revision),
            TranscriptProviderTarget::View(request.view_id.clone()),
        ) {
            return Ok(TranscriptProviderResponseKind::Stale(stale));
        }

        if conversation.is_none() {
            return Ok(TranscriptProviderResponseKind::ViewPage(empty_page(
                request.view_id,
                from_storage_revision(revision),
                history_state,
            )));
        }

        let page = self.store.read_transcript_page(
            &view_id,
            to_storage_page_anchor(request.anchor),
            to_storage_page_direction(request.direction),
            request.limit,
            request.observed_revision.map(to_storage_revision),
        )?;

        Ok(TranscriptProviderResponseKind::ViewPage(
            view_page_from_storage(page, history_state),
        ))
    }

    fn read_projection_records(
        &self,
        request: ProjectionRecordsRequest,
    ) -> Result<TranscriptProviderResponseKind, storage::StorageError> {
        let view_id = to_storage_view_id(&request.view_id);
        let current_revision = self.store.current_revision(&view_id)?;
        if let Some(stale) = stale_for_observed_revision(
            request.observed_revision,
            from_storage_revision(current_revision),
            TranscriptProviderTarget::View(request.view_id.clone()),
        ) {
            return Ok(TranscriptProviderResponseKind::Stale(stale));
        }

        let mut records = Vec::new();
        let mut rejections = Vec::new();
        for projection_id in request.projection_ids {
            let storage_projection_id = storage::ProjectionRecordId::from(projection_id.0.clone());
            match self.store.projection(&storage_projection_id)? {
                Some(record) => match &record.status {
                    storage::ProjectionStatus::Current => {
                        records.push(projection_record_from_storage(record));
                    }
                    storage::ProjectionStatus::Stale { reason, detail } => {
                        rejections.push(rejection(
                            TranscriptProviderTarget::ProjectionRecord(projection_id),
                            TranscriptProviderRejectionReason::ProjectionStale,
                            Some(from_storage_revision(record.revision)),
                            detail
                                .clone()
                                .or_else(|| Some(history_reason_label(reason))),
                        ));
                    }
                    storage::ProjectionStatus::Incomplete { reason, detail } => {
                        rejections.push(rejection(
                            TranscriptProviderTarget::ProjectionRecord(projection_id),
                            TranscriptProviderRejectionReason::ProjectionIncomplete,
                            Some(from_storage_revision(record.revision)),
                            detail
                                .clone()
                                .or_else(|| Some(history_reason_label(reason))),
                        ));
                    }
                },
                None => {
                    rejections.push(rejection(
                        TranscriptProviderTarget::ProjectionRecord(projection_id),
                        TranscriptProviderRejectionReason::MissingProjectionRecord,
                        Some(from_storage_revision(current_revision)),
                        None,
                    ));
                }
            }
        }

        Ok(TranscriptProviderResponseKind::ProjectionRecords(
            ProjectionRecordSet {
                view_id: request.view_id,
                revision: from_storage_revision(current_revision),
                records,
                rejections,
            },
        ))
    }

    fn read_resource_metadata(
        &self,
        request: ResourceMetadataRequest,
    ) -> Result<TranscriptProviderResponseKind, storage::StorageError> {
        let resource_id = to_storage_resource_id(&request.resource_id);
        let Some(metadata) = self.store.resource_metadata(&resource_id)? else {
            return Ok(TranscriptProviderResponseKind::Rejected(rejection(
                TranscriptProviderTarget::Resource(request.resource_id),
                TranscriptProviderRejectionReason::MissingResource,
                None,
                None,
            )));
        };
        let revision = from_storage_revision(metadata.revision);
        if let Some(stale) = stale_for_observed_revision(
            request.observed_revision,
            revision,
            TranscriptProviderTarget::Resource(request.resource_id.clone()),
        ) {
            return Ok(TranscriptProviderResponseKind::Stale(stale));
        }

        if let Some(rejection) =
            resource_state_rejection(&request.resource_id, &metadata.state, revision, None)
        {
            return Ok(TranscriptProviderResponseKind::Rejected(rejection));
        }

        Ok(TranscriptProviderResponseKind::ResourceMetadata(
            resource_metadata_from_storage(metadata),
        ))
    }

    fn read_resource_range(
        &self,
        request: ResourceRangeRequest,
    ) -> Result<TranscriptProviderResponseKind, storage::StorageError> {
        let resource_id = to_storage_resource_id(&request.resource_id);
        let range = request.range.clone();
        let Some(metadata) = self.store.resource_metadata(&resource_id)? else {
            return Ok(TranscriptProviderResponseKind::Rejected(rejection(
                TranscriptProviderTarget::Resource(request.resource_id),
                TranscriptProviderRejectionReason::MissingResource,
                None,
                None,
            )));
        };
        let revision = from_storage_revision(metadata.revision);
        if let Some(stale) = stale_for_observed_revision(
            request.observed_revision,
            revision,
            TranscriptProviderTarget::ResourceRange {
                resource_id: request.resource_id.clone(),
                range: range.clone(),
            },
        ) {
            return Ok(TranscriptProviderResponseKind::Stale(stale));
        }

        if let Some(rejection) = resource_state_rejection(
            &request.resource_id,
            &metadata.state,
            revision,
            Some(range.clone()),
        ) {
            return Ok(TranscriptProviderResponseKind::Rejected(rejection));
        }

        let response = self.store.read_resource_range(
            &resource_id,
            storage::ByteRange::new(range.start, range.end),
            request.observed_revision.map(to_storage_revision),
        )?;
        Ok(TranscriptProviderResponseKind::ResourceRange(
            resource_range_from_storage(response),
        ))
    }
}

impl SyndicTranscriptProvider for StorageSyndicTranscriptProvider {
    fn handle_request(&mut self, request: TranscriptProviderRequest) -> TranscriptProviderResult {
        let request_id = request.id;
        let result = match request.kind {
            TranscriptProviderRequestKind::ReadViewPage(request) => self.read_view_page(request),
            TranscriptProviderRequestKind::ReadProjectionRecords(request) => {
                self.read_projection_records(request)
            }
            TranscriptProviderRequestKind::ReadResourceMetadata(request) => {
                self.read_resource_metadata(request)
            }
            TranscriptProviderRequestKind::ReadResourceRange(request) => {
                self.read_resource_range(request)
            }
        };

        let kind = match result {
            Ok(kind) => kind,
            Err(error) => response_kind_for_storage_error(request_id, error),
        };
        Ok(TranscriptProviderResponse { request_id, kind })
    }
}

fn response_kind_for_storage_error(
    request_id: ProviderRequestId,
    error: storage::StorageError,
) -> TranscriptProviderResponseKind {
    match error {
        storage::StorageError::StaleRevision {
            view_id,
            observed,
            current,
        } => TranscriptProviderResponseKind::Stale(TranscriptProviderStale {
            target: TranscriptProviderTarget::View(from_storage_view_id(view_id)),
            observed_revision: observed.map(from_storage_revision),
            current_revision: from_storage_revision(current),
        }),
        storage::StorageError::StaleRecordRevision {
            observed, current, ..
        } => TranscriptProviderResponseKind::Stale(TranscriptProviderStale {
            target: TranscriptProviderTarget::Request(request_id),
            observed_revision: observed.map(from_storage_revision),
            current_revision: from_storage_revision(current),
        }),
        storage::StorageError::MissingCursor { cursor } => {
            TranscriptProviderResponseKind::Rejected(rejection(
                TranscriptProviderTarget::Cursor(TranscriptCursor(cursor)),
                TranscriptProviderRejectionReason::MissingCursor,
                None,
                None,
            ))
        }
        storage::StorageError::Missing { kind, id } => {
            let (target, reason) = if kind.contains("projection") {
                (
                    TranscriptProviderTarget::ProjectionRecord(ProjectionRecordId(id)),
                    TranscriptProviderRejectionReason::MissingProjectionRecord,
                )
            } else {
                (
                    TranscriptProviderTarget::Resource(ResourceId(id)),
                    TranscriptProviderRejectionReason::MissingResource,
                )
            };
            TranscriptProviderResponseKind::Rejected(rejection(target, reason, None, None))
        }
        storage::StorageError::ResourceRangeOutOfBounds {
            resource_id, range, ..
        } => TranscriptProviderResponseKind::Rejected(rejection(
            TranscriptProviderTarget::ResourceRange {
                resource_id: from_storage_resource_id(resource_id),
                range,
            },
            TranscriptProviderRejectionReason::RangeOutOfBounds,
            None,
            None,
        )),
        storage::StorageError::ResourceRangeTooLarge { requested, max } => {
            TranscriptProviderResponseKind::Rejected(rejection(
                TranscriptProviderTarget::Request(request_id),
                TranscriptProviderRejectionReason::BudgetExceeded,
                None,
                Some(format!(
                    "resource range length {requested} exceeds maximum {max}"
                )),
            ))
        }
        storage::StorageError::LimitExceeded { requested, max } => {
            TranscriptProviderResponseKind::Rejected(rejection(
                TranscriptProviderTarget::Request(request_id),
                TranscriptProviderRejectionReason::BudgetExceeded,
                None,
                Some(format!("requested limit {requested} exceeds maximum {max}")),
            ))
        }
        other => TranscriptProviderResponseKind::Rejected(rejection(
            TranscriptProviderTarget::Request(request_id),
            TranscriptProviderRejectionReason::InvalidRequest,
            None,
            Some(other.to_string()),
        )),
    }
}

fn empty_page(
    view_id: TranscriptViewId,
    revision: ProviderRevision,
    history_state: TranscriptProviderHistoryState,
) -> TranscriptViewPage {
    TranscriptViewPage {
        view_id,
        revision,
        history_state,
        records: Vec::new(),
        previous_cursor: None,
        next_cursor: None,
        at_start: true,
        at_end: true,
    }
}

fn view_page_from_storage(
    page: storage::TranscriptPage,
    history_state: TranscriptProviderHistoryState,
) -> TranscriptViewPage {
    TranscriptViewPage {
        view_id: from_storage_view_id(page.view_id),
        revision: from_storage_revision(page.revision),
        history_state,
        records: page
            .records
            .into_iter()
            .map(view_record_from_storage)
            .collect(),
        previous_cursor: page.previous_cursor.map(from_storage_cursor),
        next_cursor: page.next_cursor.map(from_storage_cursor),
        at_start: page.at_start,
        at_end: page.at_end,
    }
}

fn view_record_from_storage(record: storage::TranscriptViewRecord) -> TranscriptViewRecord {
    TranscriptViewRecord {
        id: TranscriptViewRecordId(record.id.to_string()),
        position: from_storage_position(record.position),
        projection_id: ProjectionRecordId(record.projection_id.to_string()),
        narrative_kind: narrative_kind_from_storage(record.narrative_kind),
        provenance: provenance_from_storage(record.provenance),
    }
}

fn projection_record_from_storage(record: storage::ProjectionRecord) -> ProjectionRecord {
    ProjectionRecord {
        id: ProjectionRecordId(record.id.to_string()),
        revision: from_storage_revision(record.revision),
        kind: projection_kind_from_storage(record.kind),
        payload: projection_payload_from_storage(record.payload),
        provenance: provenance_from_storage(record.provenance),
    }
}

fn resource_metadata_from_storage(record: storage::ResourceMetadataRecord) -> ResourceMetadata {
    ResourceMetadata {
        resource_id: from_storage_resource_id(record.id),
        revision: from_storage_revision(record.revision),
        kind: resource_kind_from_storage(record.kind),
        media_type: record.media_type,
        byte_len: record.byte_len,
        digest: record.digest,
        line_count: record.line_count,
        row_count: record.row_count,
        column_count: record.column_count,
        preview_range: record.preview_range.map(byte_range_to_range),
    }
}

fn resource_range_from_storage(response: storage::ResourceRangeResponse) -> ResourceRangeResponse {
    ResourceRangeResponse {
        resource_id: from_storage_resource_id(response.resource_id),
        revision: from_storage_revision(response.revision),
        kind: resource_kind_from_storage(response.kind),
        range: byte_range_to_range(response.range),
        bytes: response.bytes,
        complete: response.complete,
    }
}

fn resource_state_rejection(
    resource_id: &ResourceId,
    state: &storage::ResourceState,
    revision: ProviderRevision,
    range: Option<Range<u64>>,
) -> Option<TranscriptProviderRejection> {
    match state {
        storage::ResourceState::Ready => None,
        storage::ResourceState::Missing { reason, detail } => Some(rejection(
            resource_target(resource_id, range),
            TranscriptProviderRejectionReason::MissingResource,
            Some(revision),
            detail
                .clone()
                .or_else(|| Some(history_reason_label(reason))),
        )),
        storage::ResourceState::Rejected { reason, message } => Some(rejection(
            resource_target(resource_id, range),
            rejected_resource_reason(reason),
            Some(revision),
            message.clone(),
        )),
    }
}

fn resource_target(
    resource_id: &ResourceId,
    range: Option<Range<u64>>,
) -> TranscriptProviderTarget {
    match range {
        Some(range) => TranscriptProviderTarget::ResourceRange {
            resource_id: resource_id.clone(),
            range,
        },
        None => TranscriptProviderTarget::Resource(resource_id.clone()),
    }
}

fn rejected_resource_reason(reason: &str) -> TranscriptProviderRejectionReason {
    match reason {
        "budget" | "budget_exceeded" => TranscriptProviderRejectionReason::BudgetExceeded,
        "policy" | "policy_denied" => TranscriptProviderRejectionReason::PolicyDenied,
        "unsupported" | "unsupported_resource_kind" => {
            TranscriptProviderRejectionReason::UnsupportedResourceKind
        }
        _ => TranscriptProviderRejectionReason::InvalidRequest,
    }
}

fn rejection(
    target: TranscriptProviderTarget,
    reason: TranscriptProviderRejectionReason,
    revision: Option<ProviderRevision>,
    message: Option<String>,
) -> TranscriptProviderRejection {
    TranscriptProviderRejection {
        target,
        reason,
        revision,
        message,
    }
}

fn stale_for_observed_revision(
    observed_revision: Option<ProviderRevision>,
    current_revision: ProviderRevision,
    target: TranscriptProviderTarget,
) -> Option<TranscriptProviderStale> {
    observed_revision
        .filter(|observed| *observed != current_revision)
        .map(|observed_revision| TranscriptProviderStale {
            target,
            observed_revision: Some(observed_revision),
            current_revision,
        })
}

fn history_state_from_storage(state: &storage::HistoryState) -> TranscriptProviderHistoryState {
    match state {
        storage::HistoryState::Complete => TranscriptProviderHistoryState::Complete,
        storage::HistoryState::Incomplete { reason, detail } => {
            TranscriptProviderHistoryState::Incomplete {
                reason: history_reason_from_storage(reason),
                detail: detail.clone(),
            }
        }
        storage::HistoryState::Unavailable { reason, detail } => {
            TranscriptProviderHistoryState::Unavailable {
                reason: history_reason_from_storage(reason),
                detail: detail.clone(),
            }
        }
    }
}

fn not_captured_history_state() -> TranscriptProviderHistoryState {
    TranscriptProviderHistoryState::Incomplete {
        reason: TranscriptProviderHistoryReason::NotCaptured,
        detail: Some("Syndic has no captured transcript history for this view".to_string()),
    }
}

fn history_reason_from_storage(
    reason: &storage::HistoryIncompleteReason,
) -> TranscriptProviderHistoryReason {
    match reason {
        storage::HistoryIncompleteReason::NotCaptured => {
            TranscriptProviderHistoryReason::NotCaptured
        }
        storage::HistoryIncompleteReason::MissedEvents => {
            TranscriptProviderHistoryReason::MissedEvents
        }
        storage::HistoryIncompleteReason::StreamLost => TranscriptProviderHistoryReason::StreamLost,
        storage::HistoryIncompleteReason::StorageFailure => {
            TranscriptProviderHistoryReason::StorageFailure
        }
        storage::HistoryIncompleteReason::UnknownTerminalState => {
            TranscriptProviderHistoryReason::UnknownTerminalState
        }
        storage::HistoryIncompleteReason::ProjectionStale => {
            TranscriptProviderHistoryReason::ProjectionStale
        }
        storage::HistoryIncompleteReason::ResourceMissing => {
            TranscriptProviderHistoryReason::ResourceMissing
        }
        storage::HistoryIncompleteReason::Other(reason) => {
            TranscriptProviderHistoryReason::Other(reason.clone())
        }
    }
}

fn history_reason_label(reason: &storage::HistoryIncompleteReason) -> String {
    match reason {
        storage::HistoryIncompleteReason::NotCaptured => "not captured".to_string(),
        storage::HistoryIncompleteReason::MissedEvents => "missed events".to_string(),
        storage::HistoryIncompleteReason::StreamLost => "stream lost".to_string(),
        storage::HistoryIncompleteReason::StorageFailure => "storage failure".to_string(),
        storage::HistoryIncompleteReason::UnknownTerminalState => {
            "unknown terminal state".to_string()
        }
        storage::HistoryIncompleteReason::ProjectionStale => "projection stale".to_string(),
        storage::HistoryIncompleteReason::ResourceMissing => "resource missing".to_string(),
        storage::HistoryIncompleteReason::Other(reason) => reason.clone(),
    }
}

fn provenance_from_storage(provenance: storage::SyndicSourceProvenance) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: from_storage_view_id(provenance.view_id),
        position: provenance.position.map(from_storage_position),
        turn_id: provenance.turn_id.map(|id| SyndicTurnId(id.to_string())),
        item_id: provenance.item_id.map(|id| SyndicItemId(id.to_string())),
        projection_id: provenance
            .projection_id
            .map(|id| ProjectionRecordId(id.to_string())),
        resource_id: provenance.resource_id.map(from_storage_resource_id),
        source_range: provenance.source_range.map(byte_range_to_range),
        resource_range: provenance.resource_range.map(byte_range_to_range),
        copy_source_range: provenance.copy_source_range.map(byte_range_to_range),
    }
}

fn projection_payload_from_storage(payload: storage::ProjectionPayload) -> ProjectionPayload {
    match payload {
        storage::ProjectionPayload::Text { text } => ProjectionPayload::Text { text },
        storage::ProjectionPayload::ResourceReference {
            resource_id,
            resource_kind,
            label,
        } => ProjectionPayload::ResourceReference {
            resource_id: from_storage_resource_id(resource_id),
            resource_kind: resource_kind_from_storage(resource_kind),
            label,
        },
    }
}

fn projection_kind_from_storage(kind: storage::ProjectionRecordKind) -> ProjectionRecordKind {
    match kind {
        storage::ProjectionRecordKind::TextChunk => ProjectionRecordKind::TextChunk,
        storage::ProjectionRecordKind::ResourceReference => ProjectionRecordKind::ResourceReference,
    }
}

fn narrative_kind_from_storage(kind: storage::TranscriptNarrativeKind) -> TranscriptNarrativeKind {
    match kind {
        storage::TranscriptNarrativeKind::UserInput => TranscriptNarrativeKind::UserInput,
        storage::TranscriptNarrativeKind::UserMedia => TranscriptNarrativeKind::UserMedia,
        storage::TranscriptNarrativeKind::AssistantCommentary => {
            TranscriptNarrativeKind::AssistantCommentary
        }
        storage::TranscriptNarrativeKind::AssistantFinalAnswer => {
            TranscriptNarrativeKind::AssistantFinalAnswer
        }
        storage::TranscriptNarrativeKind::AssistantGeneratedMedia => {
            TranscriptNarrativeKind::AssistantGeneratedMedia
        }
    }
}

fn resource_kind_from_storage(kind: storage::ResourceKind) -> ResourceKind {
    match kind {
        storage::ResourceKind::Code => ResourceKind::Code,
        storage::ResourceKind::Table => ResourceKind::Table,
        storage::ResourceKind::Image => ResourceKind::Image,
        storage::ResourceKind::Attachment => ResourceKind::Attachment,
        storage::ResourceKind::GeneratedImage => ResourceKind::GeneratedImage,
        storage::ResourceKind::Other(label) => ResourceKind::Other(label),
    }
}

fn to_storage_view_id(view_id: &TranscriptViewId) -> storage::ThreadViewId {
    storage::ThreadViewId::from(view_id.0.clone())
}

fn from_storage_view_id(view_id: storage::ThreadViewId) -> TranscriptViewId {
    TranscriptViewId(view_id.to_string())
}

fn to_storage_resource_id(resource_id: &ResourceId) -> storage::ResourceId {
    storage::ResourceId::from(resource_id.0.clone())
}

fn from_storage_resource_id(resource_id: storage::ResourceId) -> ResourceId {
    ResourceId(resource_id.to_string())
}

fn from_storage_cursor(cursor: storage::CursorId) -> TranscriptCursor {
    TranscriptCursor(cursor.to_string())
}

fn to_storage_revision(revision: ProviderRevision) -> storage::ProviderRevision {
    storage::ProviderRevision(revision.0)
}

fn from_storage_revision(revision: storage::ProviderRevision) -> ProviderRevision {
    ProviderRevision(revision.0)
}

fn from_storage_position(position: storage::TranscriptViewPosition) -> TranscriptViewPosition {
    TranscriptViewPosition(position.0)
}

fn to_storage_page_anchor(anchor: TranscriptPageAnchor) -> storage::TranscriptPageAnchor {
    match anchor {
        TranscriptPageAnchor::Start => storage::TranscriptPageAnchor::Start,
        TranscriptPageAnchor::End => storage::TranscriptPageAnchor::End,
        TranscriptPageAnchor::Cursor(cursor) => {
            storage::TranscriptPageAnchor::Cursor(storage::CursorId::from(cursor.0))
        }
        TranscriptPageAnchor::Position(position) => {
            storage::TranscriptPageAnchor::Position(storage::TranscriptViewPosition(position.0))
        }
    }
}

fn to_storage_page_direction(
    direction: TranscriptPageDirection,
) -> storage::TranscriptPageDirection {
    match direction {
        TranscriptPageDirection::Forward => storage::TranscriptPageDirection::Forward,
        TranscriptPageDirection::Backward => storage::TranscriptPageDirection::Backward,
    }
}

fn byte_range_to_range(range: storage::ByteRange) -> Range<u64> {
    range.start..range.end
}

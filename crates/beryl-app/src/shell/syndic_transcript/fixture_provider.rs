//! In-memory Syndic transcript provider for contract tests.
//!
//! The fixture speaks the same provider contract as a production Syndic-backed
//! implementation. It intentionally stores only provider-shaped data so tests
//! cannot depend on backend history or renderer presentation records.

use std::collections::HashMap;

use super::*;

#[derive(Clone, Debug, Default)]
pub(crate) struct InMemorySyndicTranscriptProvider {
    revision: ProviderRevision,
    views: HashMap<TranscriptViewId, Vec<TranscriptViewRecord>>,
    projections: HashMap<ProjectionRecordId, ProjectionRecord>,
    projection_rejections: HashMap<ProjectionRecordId, FixtureRejection>,
    resources: HashMap<ResourceId, FixtureResource>,
    resource_rejections: HashMap<ResourceId, FixtureRejection>,
}

#[derive(Clone, Debug)]
struct FixtureResource {
    metadata: ResourceMetadata,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct FixtureRejection {
    reason: TranscriptProviderRejectionReason,
    message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedPageAnchor {
    forward_start: usize,
    backward_end: usize,
}

impl InMemorySyndicTranscriptProvider {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn revision(&self) -> ProviderRevision {
        self.revision
    }

    pub(crate) fn set_revision(&mut self, revision: ProviderRevision) -> &mut Self {
        self.revision = revision;
        self
    }

    pub(crate) fn advance_revision(&mut self) -> ProviderRevision {
        self.revision.0 += 1;
        self.revision
    }

    pub(crate) fn insert_view_records(
        &mut self,
        view_id: TranscriptViewId,
        mut records: Vec<TranscriptViewRecord>,
    ) -> &mut Self {
        sort_view_records(&mut records);
        self.views.insert(view_id, records);
        self
    }

    pub(crate) fn push_view_record(
        &mut self,
        view_id: TranscriptViewId,
        record: TranscriptViewRecord,
    ) -> &mut Self {
        let records = self.views.entry(view_id).or_default();
        records.push(record);
        sort_view_records(records);
        self
    }

    pub(crate) fn insert_projection_record(&mut self, record: ProjectionRecord) -> &mut Self {
        self.projections.insert(record.id.clone(), record);
        self
    }

    pub(crate) fn reject_projection_record(
        &mut self,
        projection_id: ProjectionRecordId,
        reason: TranscriptProviderRejectionReason,
    ) -> &mut Self {
        self.insert_projection_rejection(projection_id, reason, None)
    }

    pub(crate) fn reject_projection_record_with_message(
        &mut self,
        projection_id: ProjectionRecordId,
        reason: TranscriptProviderRejectionReason,
        message: impl Into<String>,
    ) -> &mut Self {
        self.insert_projection_rejection(projection_id, reason, Some(message.into()))
    }

    pub(crate) fn insert_resource(
        &mut self,
        mut metadata: ResourceMetadata,
        bytes: Vec<u8>,
    ) -> &mut Self {
        metadata.byte_len = bytes.len() as u64;
        self.resources.insert(
            metadata.resource_id.clone(),
            FixtureResource { metadata, bytes },
        );
        self
    }

    pub(crate) fn reject_resource(
        &mut self,
        resource_id: ResourceId,
        reason: TranscriptProviderRejectionReason,
    ) -> &mut Self {
        self.insert_resource_rejection(resource_id, reason, None)
    }

    pub(crate) fn reject_resource_with_message(
        &mut self,
        resource_id: ResourceId,
        reason: TranscriptProviderRejectionReason,
        message: impl Into<String>,
    ) -> &mut Self {
        self.insert_resource_rejection(resource_id, reason, Some(message.into()))
    }

    fn insert_resource_rejection(
        &mut self,
        resource_id: ResourceId,
        reason: TranscriptProviderRejectionReason,
        message: Option<String>,
    ) -> &mut Self {
        self.resource_rejections
            .insert(resource_id, FixtureRejection { reason, message });
        self
    }

    fn insert_projection_rejection(
        &mut self,
        projection_id: ProjectionRecordId,
        reason: TranscriptProviderRejectionReason,
        message: Option<String>,
    ) -> &mut Self {
        self.projection_rejections
            .insert(projection_id, FixtureRejection { reason, message });
        self
    }

    pub(crate) fn cursor_for_offset(offset: usize) -> TranscriptCursor {
        TranscriptCursor(format!("offset:{offset}"))
    }

    fn read_view_page(&self, request: TranscriptViewPageRequest) -> TranscriptProviderResponseKind {
        if let Some(stale) = self.stale_response(
            request.observed_revision,
            TranscriptProviderTarget::View(request.view_id.clone()),
        ) {
            return TranscriptProviderResponseKind::Stale(stale);
        }

        let Some(records) = self.views.get(&request.view_id) else {
            return TranscriptProviderResponseKind::Rejected(self.rejection(
                TranscriptProviderTarget::View(request.view_id),
                TranscriptProviderRejectionReason::MissingView,
                None,
            ));
        };

        let anchor = match self.resolve_page_anchor(&request.anchor, records) {
            Ok(anchor) => anchor,
            Err(rejection) => return TranscriptProviderResponseKind::Rejected(rejection),
        };

        let (start, end) = match request.direction {
            TranscriptPageDirection::Forward => {
                let start = anchor.forward_start;
                let end = start.saturating_add(request.limit).min(records.len());
                (start, end)
            }
            TranscriptPageDirection::Backward => {
                let end = anchor.backward_end;
                let start = end.saturating_sub(request.limit);
                (start, end)
            }
        };

        TranscriptProviderResponseKind::ViewPage(TranscriptViewPage {
            view_id: request.view_id,
            revision: self.revision,
            records: records[start..end].to_vec(),
            previous_cursor: (start > 0).then(|| Self::cursor_for_offset(start)),
            next_cursor: (end < records.len()).then(|| Self::cursor_for_offset(end)),
            at_start: start == 0,
            at_end: end == records.len(),
        })
    }

    fn read_projection_records(
        &self,
        request: ProjectionRecordsRequest,
    ) -> TranscriptProviderResponseKind {
        if let Some(stale) = self.stale_response(
            request.observed_revision,
            TranscriptProviderTarget::View(request.view_id.clone()),
        ) {
            return TranscriptProviderResponseKind::Stale(stale);
        }

        let mut records = Vec::new();
        let mut rejections = Vec::new();

        for projection_id in request.projection_ids {
            if let Some(rejection) = self.projection_rejection(&projection_id) {
                rejections.push(rejection);
                continue;
            }

            if let Some(record) = self.projections.get(&projection_id) {
                let mut record = record.clone();
                record.revision = self.revision;
                records.push(record);
            } else {
                rejections.push(self.rejection(
                    TranscriptProviderTarget::ProjectionRecord(projection_id),
                    TranscriptProviderRejectionReason::MissingProjectionRecord,
                    None,
                ));
            }
        }

        TranscriptProviderResponseKind::ProjectionRecords(ProjectionRecordSet {
            view_id: request.view_id,
            revision: self.revision,
            records,
            rejections,
        })
    }

    fn read_resource_metadata(
        &self,
        request: ResourceMetadataRequest,
    ) -> TranscriptProviderResponseKind {
        if let Some(stale) = self.stale_response(
            request.observed_revision,
            TranscriptProviderTarget::Resource(request.resource_id.clone()),
        ) {
            return TranscriptProviderResponseKind::Stale(stale);
        }

        if let Some(rejection) = self.resource_rejection(
            &request.resource_id,
            TranscriptProviderTarget::Resource(request.resource_id.clone()),
        ) {
            return TranscriptProviderResponseKind::Rejected(rejection);
        }

        let Some(resource) = self.resources.get(&request.resource_id) else {
            return TranscriptProviderResponseKind::Rejected(self.rejection(
                TranscriptProviderTarget::Resource(request.resource_id),
                TranscriptProviderRejectionReason::MissingResource,
                None,
            ));
        };

        let mut metadata = resource.metadata.clone();
        metadata.revision = self.revision;
        TranscriptProviderResponseKind::ResourceMetadata(metadata)
    }

    fn read_resource_range(&self, request: ResourceRangeRequest) -> TranscriptProviderResponseKind {
        if let Some(stale) = self.stale_response(
            request.observed_revision,
            TranscriptProviderTarget::ResourceRange {
                resource_id: request.resource_id.clone(),
                range: request.range.clone(),
            },
        ) {
            return TranscriptProviderResponseKind::Stale(stale);
        }

        if let Some(rejection) = self.resource_rejection(
            &request.resource_id,
            TranscriptProviderTarget::ResourceRange {
                resource_id: request.resource_id.clone(),
                range: request.range.clone(),
            },
        ) {
            return TranscriptProviderResponseKind::Rejected(rejection);
        }

        let Some(resource) = self.resources.get(&request.resource_id) else {
            return TranscriptProviderResponseKind::Rejected(self.rejection(
                TranscriptProviderTarget::Resource(request.resource_id),
                TranscriptProviderRejectionReason::MissingResource,
                None,
            ));
        };

        let byte_len = resource.bytes.len() as u64;
        if request.range.start > request.range.end || request.range.end > byte_len {
            return TranscriptProviderResponseKind::Rejected(self.rejection(
                TranscriptProviderTarget::ResourceRange {
                    resource_id: request.resource_id,
                    range: request.range,
                },
                TranscriptProviderRejectionReason::RangeOutOfBounds,
                None,
            ));
        }

        let range = request.range;
        let start = range.start as usize;
        let end = range.end as usize;
        TranscriptProviderResponseKind::ResourceRange(ResourceRangeResponse {
            resource_id: request.resource_id,
            revision: self.revision,
            kind: resource.metadata.kind.clone(),
            range,
            bytes: resource.bytes[start..end].to_vec(),
            complete: end == resource.bytes.len(),
        })
    }

    fn resolve_page_anchor(
        &self,
        anchor: &TranscriptPageAnchor,
        records: &[TranscriptViewRecord],
    ) -> Result<ResolvedPageAnchor, TranscriptProviderRejection> {
        let len = records.len();
        match anchor {
            TranscriptPageAnchor::Start => Ok(ResolvedPageAnchor {
                forward_start: 0,
                backward_end: 0,
            }),
            TranscriptPageAnchor::End => Ok(ResolvedPageAnchor {
                forward_start: len,
                backward_end: len,
            }),
            TranscriptPageAnchor::Cursor(cursor) => {
                let Some(offset) = parse_offset_cursor(cursor).filter(|offset| *offset <= len)
                else {
                    return Err(self.rejection(
                        TranscriptProviderTarget::Cursor(cursor.clone()),
                        TranscriptProviderRejectionReason::MissingCursor,
                        None,
                    ));
                };
                Ok(ResolvedPageAnchor {
                    forward_start: offset,
                    backward_end: offset,
                })
            }
            TranscriptPageAnchor::Position(position) => {
                let forward_start = records
                    .iter()
                    .position(|record| record.position >= *position)
                    .unwrap_or(len);
                let backward_end = records
                    .iter()
                    .position(|record| record.position > *position)
                    .unwrap_or(len);
                Ok(ResolvedPageAnchor {
                    forward_start,
                    backward_end,
                })
            }
        }
    }

    fn stale_response(
        &self,
        observed_revision: Option<ProviderRevision>,
        target: TranscriptProviderTarget,
    ) -> Option<TranscriptProviderStale> {
        observed_revision
            .filter(|observed_revision| *observed_revision != self.revision)
            .map(|observed_revision| TranscriptProviderStale {
                target,
                observed_revision: Some(observed_revision),
                current_revision: self.revision,
            })
    }

    fn projection_rejection(
        &self,
        projection_id: &ProjectionRecordId,
    ) -> Option<TranscriptProviderRejection> {
        self.projection_rejections
            .get(projection_id)
            .map(|rejection| {
                self.rejection(
                    TranscriptProviderTarget::ProjectionRecord(projection_id.clone()),
                    rejection.reason.clone(),
                    rejection.message.clone(),
                )
            })
    }

    fn resource_rejection(
        &self,
        resource_id: &ResourceId,
        target: TranscriptProviderTarget,
    ) -> Option<TranscriptProviderRejection> {
        self.resource_rejections.get(resource_id).map(|rejection| {
            self.rejection(target, rejection.reason.clone(), rejection.message.clone())
        })
    }

    fn rejection(
        &self,
        target: TranscriptProviderTarget,
        reason: TranscriptProviderRejectionReason,
        message: Option<String>,
    ) -> TranscriptProviderRejection {
        TranscriptProviderRejection {
            target,
            reason,
            revision: Some(self.revision),
            message,
        }
    }
}

impl SyndicTranscriptProvider for InMemorySyndicTranscriptProvider {
    fn handle_request(&mut self, request: TranscriptProviderRequest) -> TranscriptProviderResult {
        let request_id = request.id;
        let kind = match request.kind {
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

        Ok(TranscriptProviderResponse { request_id, kind })
    }
}

fn sort_view_records(records: &mut [TranscriptViewRecord]) {
    records.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
}

fn parse_offset_cursor(cursor: &TranscriptCursor) -> Option<usize> {
    cursor.0.strip_prefix("offset:")?.parse().ok()
}

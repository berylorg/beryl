use std::{ops::Range, sync::atomic::AtomicBool};

use beryl_home_store::CursorReadLimits;
use syndic_storage::{
    AssistantMessagePhase, CanonicalItemKind, ProjectionLifecycle, SyndicPointReadLimit,
    TranscriptPosition, TranscriptViewHeadRecord,
};

use crate::syndic_transcript::{
    PreparedTranscriptActivation, ProjectionPayload, ProjectionRecord, ProjectionRecordId,
    ProjectionRecordKind, ProjectionRecordSet, ProviderRevision, ResourceId, SyndicItemId,
    SyndicSourceProvenance, SyndicTurnId, TranscriptActivationPlacement, TranscriptNarrativeKind,
    TranscriptProviderHistoryReason, TranscriptProviderHistoryState,
    TranscriptProviderResponseKind, TranscriptViewId, TranscriptViewPage, TranscriptViewPosition,
    TranscriptViewRecord, TranscriptViewRecordId,
};

use super::{
    ATTACHMENT_MAX_BYTES, ATTACHMENT_MAX_RECORDS, TranscriptAttachmentAuthority,
    TranscriptAttachmentError, TranscriptAttachmentPurpose, TranscriptAttachmentRequest,
    TranscriptProviderReader,
};

impl TranscriptProviderReader {
    pub(super) fn read_activation(
        &self,
        request: &TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
    ) -> Result<
        (
            TranscriptAttachmentAuthority,
            Range<u64>,
            PreparedTranscriptActivation,
        ),
        TranscriptAttachmentError,
    > {
        match self.read_bounded_activation(request, cancelled) {
            Err(TranscriptAttachmentError::Capacity)
                if request.purpose == TranscriptAttachmentPurpose::Attach =>
            {
                self.read_capacity_activation(request, cancelled)
            }
            result => result,
        }
    }

    fn read_bounded_activation(
        &self,
        request: &TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
    ) -> Result<
        (
            TranscriptAttachmentAuthority,
            Range<u64>,
            PreparedTranscriptActivation,
        ),
        TranscriptAttachmentError,
    > {
        let point = SyndicPointReadLimit::new(ATTACHMENT_MAX_BYTES)
            .map_err(|_| TranscriptAttachmentError::Capacity)?;
        let thread = self
            .syndic
            .thread(&self.home, request.thread_id, point)
            .map_err(|_| TranscriptAttachmentError::Unavailable)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        let head = self
            .syndic
            .transcript_view_head(&self.home, request.thread_id, point)
            .map_err(|_| TranscriptAttachmentError::Unavailable)?;
        if head
            .as_ref()
            .is_none_or(|head| head.lifecycle() != ProjectionLifecycle::Current)
        {
            if request.purpose == TranscriptAttachmentPurpose::Refresh {
                return Err(TranscriptAttachmentError::Stale);
            }
            let summary = self
                .syndic
                .history_summary(&self.home, request.thread_id, point)
                .map_err(|_| TranscriptAttachmentError::Unavailable)?;
            if head
                .as_ref()
                .is_some_and(|head| head.thread_id() != request.thread_id)
            {
                return Err(TranscriptAttachmentError::Identity);
            }
            self.check_request(request, cancelled)?;
            let view_id = TranscriptViewId(request.thread_id.to_string());
            let revision = ProviderRevision(thread.revision().get());
            let history_state = TranscriptProviderHistoryState::Unavailable {
                reason: TranscriptProviderHistoryReason::ProjectionStale,
                detail: Some("The transcript is still being prepared.".to_owned()),
            };
            let page = TranscriptViewPage {
                view_id: view_id.clone(),
                revision,
                history_state,
                records: Vec::new(),
                previous_cursor: None,
                next_cursor: None,
                at_start: true,
                at_end: true,
            };
            let seed = PreparedTranscriptActivation {
                view_id: Some(view_id),
                placement: request.placement,
                view_page_response: Some(TranscriptProviderResponseKind::ViewPage(page)),
                projection_records_response: None,
            };
            return Ok((
                TranscriptAttachmentAuthority::Unpublished {
                    thread,
                    head,
                    summary,
                },
                0..0,
                seed,
            ));
        }
        let head = head.ok_or(TranscriptAttachmentError::Stale)?;
        if head.thread_id() != request.thread_id {
            return Err(TranscriptAttachmentError::Identity);
        }
        if head.committed_tail() != thread.committed_tail()
            || head.selected_path_digest() != thread.selected_path_digest()
        {
            return Err(TranscriptAttachmentError::Stale);
        }
        if head.lifecycle() != ProjectionLifecycle::Current {
            return Err(TranscriptAttachmentError::Stale);
        }
        let count = head.entry_count();
        let first = match request.placement {
            TranscriptActivationPlacement::Tail => count
                .saturating_sub(ATTACHMENT_MAX_RECORDS as u64)
                .saturating_add(1),
            TranscriptActivationPlacement::Start => 1,
            TranscriptActivationPlacement::Position(position) => position.0,
        };
        if first == 0 || (count != 0 && first > count) {
            return Err(TranscriptAttachmentError::Identity);
        }
        let after = if first > 1 {
            Some(
                TranscriptPosition::new(first - 1)
                    .map_err(|_| TranscriptAttachmentError::Identity)?,
            )
        } else {
            None
        };
        let limits = CursorReadLimits::new(ATTACHMENT_MAX_RECORDS, ATTACHMENT_MAX_BYTES)
            .map_err(|_| TranscriptAttachmentError::Capacity)?;
        let page = self
            .syndic
            .transcript_entries(
                &self.home,
                request.thread_id,
                head.generation(),
                after,
                limits,
            )
            .map_err(|_| TranscriptAttachmentError::Unavailable)?;
        let mut entries = page.into_records();
        let tail = request.placement == TranscriptActivationPlacement::Tail;
        if tail && count != 0 && entries.last().map(|entry| entry.position().get()) != Some(count) {
            return Err(TranscriptAttachmentError::Stale);
        }
        if tail {
            entries.reverse();
        }
        let view_id = TranscriptViewId(request.thread_id.to_string());
        let revision = ProviderRevision(head.revision().get());
        let mut views = Vec::with_capacity(entries.len());
        let mut projections = Vec::with_capacity(entries.len());
        let mut rejections = Vec::new();
        let mut bytes = 0_usize;
        let mut expected_position = if tail { count } else { first };
        for entry in &entries {
            self.check_request(request, cancelled)?;
            if entry.thread_id() != request.thread_id
                || entry.generation() != head.generation()
                || entry.position().get() != expected_position
            {
                return Err(TranscriptAttachmentError::Identity);
            }
            let projection = self
                .syndic
                .projection(&self.home, entry.projection_id(), point)
                .map_err(attachment_read_error)?
                .ok_or(TranscriptAttachmentError::Unavailable)?;
            let item = self
                .syndic
                .canonical_item(&self.home, entry.item_id(), point)
                .map_err(attachment_read_error)?
                .ok_or(TranscriptAttachmentError::Unavailable)?;
            if projection.id() != entry.projection_id()
                || projection.revision() != entry.projection_revision()
                || projection.item_id() != entry.item_id()
                || item.revision() != entry.item_revision()
            {
                return Err(TranscriptAttachmentError::Stale);
            }
            let narrative_kind = match item.kind() {
                CanonicalItemKind::UserInput => TranscriptNarrativeKind::UserInput,
                CanonicalItemKind::AssistantMessage(AssistantMessagePhase::Commentary) => {
                    TranscriptNarrativeKind::AssistantCommentary
                }
                CanonicalItemKind::AssistantMessage(AssistantMessagePhase::FinalAnswer) => {
                    TranscriptNarrativeKind::AssistantFinalAnswer
                }
                CanonicalItemKind::AssistantMessage(AssistantMessagePhase::Unknown)
                | CanonicalItemKind::ProviderText(_) => TranscriptNarrativeKind::AssistantText,
                CanonicalItemKind::GeneratedMedia => {
                    TranscriptNarrativeKind::AssistantGeneratedMedia
                }
                _ => return Err(TranscriptAttachmentError::Identity),
            };
            let projection_id = ProjectionRecordId(projection.id().to_string());
            let (payload, source_range, payload_bytes) = match projection.payload() {
                syndic_storage::ProjectionPayload::InlineMarkdown {
                    source,
                    source_range,
                    ..
                } => (
                    ProjectionPayload::Text {
                        text: std::sync::Arc::from(source.as_ref()),
                    },
                    Some(source_range.start()..source_range.end()),
                    source.len(),
                ),
                syndic_storage::ProjectionPayload::ResourceReference {
                    resource_id,
                    preview,
                    source_range,
                    ..
                } => {
                    let resource = self
                        .syndic
                        .resource(&self.home, *resource_id, point)
                        .map_err(attachment_read_error)?
                        .ok_or(TranscriptAttachmentError::Unavailable)?;
                    if resource.item_id() != entry.item_id()
                        || resource.projection_id() != Some(entry.projection_id())
                    {
                        return Err(TranscriptAttachmentError::Identity);
                    }
                    let kind = match resource.kind() {
                        syndic_storage::ResourceKind::Code => {
                            crate::syndic_transcript::ResourceKind::Code
                        }
                        syndic_storage::ResourceKind::Table => {
                            crate::syndic_transcript::ResourceKind::Table
                        }
                        syndic_storage::ResourceKind::Image => {
                            crate::syndic_transcript::ResourceKind::Image
                        }
                        syndic_storage::ResourceKind::Attachment => {
                            crate::syndic_transcript::ResourceKind::Attachment
                        }
                        _ => crate::syndic_transcript::ResourceKind::Other("other".into()),
                    };
                    (
                        ProjectionPayload::ResourceReference {
                            resource_id: ResourceId(resource_id.to_string()),
                            resource_kind: kind,
                            label: Some(std::sync::Arc::from(preview.as_ref())),
                        },
                        Some(source_range.start()..source_range.end()),
                        preview.len(),
                    )
                }
                syndic_storage::ProjectionPayload::Empty => {
                    (ProjectionPayload::Text { text: "".into() }, None, 0)
                }
                syndic_storage::ProjectionPayload::ImageMarker { source_offset, .. } => (
                    ProjectionPayload::Text { text: "".into() },
                    Some(*source_offset..*source_offset),
                    0,
                ),
            };
            let record_bytes = payload_bytes
                .checked_add(1024)
                .ok_or(TranscriptAttachmentError::Capacity)?;
            let next_bytes = bytes
                .checked_add(record_bytes)
                .ok_or(TranscriptAttachmentError::Capacity)?;
            if next_bytes > ATTACHMENT_MAX_BYTES {
                if views.is_empty() {
                    return Err(TranscriptAttachmentError::Capacity);
                }
                break;
            }
            bytes = next_bytes;
            let provenance = SyndicSourceProvenance {
                view_id: view_id.clone(),
                position: Some(TranscriptViewPosition(entry.position().get())),
                turn_id: Some(SyndicTurnId(projection.turn_id().to_string())),
                item_id: Some(SyndicItemId(entry.item_id().to_string())),
                projection_id: Some(projection_id.clone()),
                resource_id: None,
                source_range: source_range.clone(),
                resource_range: None,
                copy_source_range: source_range,
            };
            let kind = match &payload {
                ProjectionPayload::Text { .. } => ProjectionRecordKind::TextChunk,
                ProjectionPayload::ResourceReference { .. } => {
                    ProjectionRecordKind::ResourceReference
                }
            };
            if matches!(
                projection.payload(),
                syndic_storage::ProjectionPayload::ImageMarker { .. }
            ) {
                rejections.push(crate::syndic_transcript::TranscriptProviderRejection {
                    target: crate::syndic_transcript::TranscriptProviderTarget::ProjectionRecord(projection_id.clone()),
                    reason: crate::syndic_transcript::TranscriptProviderRejectionReason::UnsupportedResourceKind,
                    revision: Some(ProviderRevision(projection.revision().get())), message: None,
                });
            } else {
                projections.push(ProjectionRecord {
                    id: projection_id.clone(),
                    revision: ProviderRevision(projection.revision().get()),
                    kind,
                    payload,
                    provenance: provenance.clone(),
                });
            }
            views.push(TranscriptViewRecord {
                id: TranscriptViewRecordId(format!(
                    "{}:{}:{}",
                    request.thread_id,
                    head.generation().get(),
                    entry.position().get()
                )),
                position: TranscriptViewPosition(entry.position().get()),
                projection_id,
                narrative_kind,
                provenance,
            });
            expected_position = if tail {
                expected_position.saturating_sub(1)
            } else {
                expected_position
                    .checked_add(1)
                    .ok_or(TranscriptAttachmentError::Capacity)?
            };
        }
        if count != 0 && views.is_empty() {
            return Err(TranscriptAttachmentError::Unavailable);
        }
        let summary = self
            .syndic
            .history_summary(&self.home, request.thread_id, point)
            .map_err(|_| TranscriptAttachmentError::Unavailable)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        if summary.committed_tail() != head.committed_tail()
            || summary.selected_path_digest() != head.selected_path_digest()
        {
            return Err(TranscriptAttachmentError::Stale);
        }
        let history_state = if summary.complete() {
            TranscriptProviderHistoryState::Complete
        } else {
            TranscriptProviderHistoryState::Incomplete {
                reason: TranscriptProviderHistoryReason::Other(
                    "Syndic selected history is incomplete".into(),
                ),
                detail: None,
            }
        };
        if tail {
            views.reverse();
            projections.reverse();
        }
        let range_start = views.first().map(|entry| entry.position.0).unwrap_or(first);
        let range_end = views
            .last()
            .map(|entry| entry.position.0.saturating_add(1))
            .unwrap_or(first);
        let range = range_start..range_end;
        let page = TranscriptViewPage {
            view_id: view_id.clone(),
            revision,
            history_state,
            records: views,
            previous_cursor: None,
            next_cursor: None,
            at_start: range_start == 1,
            at_end: count == 0 || range_end > count,
        };
        let projection_set = ProjectionRecordSet {
            view_id: view_id.clone(),
            revision,
            records: projections,
            rejections,
        };
        self.check_request(request, cancelled)?;
        let confirmed = self
            .syndic
            .transcript_view_head(&self.home, request.thread_id, point)
            .map_err(|_| TranscriptAttachmentError::Unavailable)?;
        if confirmed.as_ref() != Some(&head) {
            return Err(TranscriptAttachmentError::Stale);
        }
        Ok((
            TranscriptAttachmentAuthority::Published { head, summary },
            range,
            PreparedTranscriptActivation {
                view_id: Some(view_id),
                placement: request.placement,
                view_page_response: Some(TranscriptProviderResponseKind::ViewPage(page)),
                projection_records_response: Some(
                    TranscriptProviderResponseKind::ProjectionRecords(projection_set),
                ),
            },
        ))
    }

    fn read_capacity_activation(
        &self,
        request: &TranscriptAttachmentRequest,
        cancelled: &AtomicBool,
    ) -> Result<
        (
            TranscriptAttachmentAuthority,
            Range<u64>,
            PreparedTranscriptActivation,
        ),
        TranscriptAttachmentError,
    > {
        self.check_request(request, cancelled)?;
        let point = SyndicPointReadLimit::new(ATTACHMENT_MAX_BYTES)
            .map_err(|_| TranscriptAttachmentError::Capacity)?;
        let thread = self
            .syndic
            .thread(&self.home, request.thread_id, point)
            .map_err(attachment_read_error)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        let head = self
            .syndic
            .transcript_view_head(&self.home, request.thread_id, point)
            .map_err(attachment_read_error)?
            .ok_or(TranscriptAttachmentError::Stale)?;
        let summary = self
            .syndic
            .history_summary(&self.home, request.thread_id, point)
            .map_err(attachment_read_error)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        if head.lifecycle() != ProjectionLifecycle::Current
            || head.thread_id() != request.thread_id
            || head.committed_tail() != thread.committed_tail()
            || head.selected_path_digest() != thread.selected_path_digest()
            || summary.committed_tail() != head.committed_tail()
            || summary.selected_path_digest() != head.selected_path_digest()
        {
            return Err(TranscriptAttachmentError::Stale);
        }
        self.check_request(request, cancelled)?;
        let view_id = TranscriptViewId(request.thread_id.to_string());
        let revision = ProviderRevision(head.revision().get());
        let page = TranscriptViewPage {
            view_id: view_id.clone(),
            revision,
            history_state: TranscriptProviderHistoryState::Unavailable {
                reason: TranscriptProviderHistoryReason::PresentationCapacity,
                detail: Some("Part of this transcript exceeds the display limit.".to_owned()),
            },
            records: Vec::new(),
            previous_cursor: None,
            next_cursor: None,
            at_start: true,
            at_end: true,
        };
        Ok((
            TranscriptAttachmentAuthority::CapacityLimited { head, summary },
            0..0,
            PreparedTranscriptActivation {
                view_id: Some(view_id),
                placement: request.placement,
                view_page_response: Some(TranscriptProviderResponseKind::ViewPage(page)),
                projection_records_response: None,
            },
        ))
    }
}

fn attachment_read_error(error: syndic_storage::SyndicReadError) -> TranscriptAttachmentError {
    match error {
        syndic_storage::SyndicReadError::Read(beryl_home_store::ReadError::BoundExceeded {
            ..
        }) => TranscriptAttachmentError::Capacity,
        _ => TranscriptAttachmentError::Unavailable,
    }
}

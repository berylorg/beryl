use super::*;
use crate::syndic_transcript::{
    CONTEXT_CHUNK_MAX_BYTES, SyntheticContextChunk, SyntheticContextGroup,
    SyntheticContextProvenance,
};
use beryl_model::DiscussionContextOwnerId;
use std::sync::Arc;
use syndic_storage::{ConversationParent, DraftSubmissionIntent, ThreadRecord};

const CONTEXT_ENCODED_MAX_BYTES: usize = 66_048;

impl access::ActivationReadSource<'_> {
    pub(super) fn read_discussion_context(
        &self,
        request: &TranscriptAttachmentRequest,
        thread: &ThreadRecord,
        head: &TranscriptViewHeadRecord,
        cancelled: &AtomicBool,
    ) -> Result<Option<SyntheticContextGroup>, TranscriptAttachmentError> {
        let Some(owner) = thread.context_owner_id() else {
            return Ok(None);
        };
        let point = SyndicPointReadLimit::new(CONTEXT_ENCODED_MAX_BYTES)
            .map_err(|_| TranscriptAttachmentError::Capacity)?;
        self.check_request(request, cancelled)?;
        let (parent, raw_owner) = match owner {
            DiscussionContextOwnerId::Draft(id) => {
                let draft = self
                    .draft(id, point)
                    .map_err(attachment_read_error)?
                    .ok_or(TranscriptAttachmentError::Unavailable)?;
                if thread.current_draft_id() != id
                    || draft.thread_id() != thread.id()
                    || draft.submission_intent() != DraftSubmissionIntent::DiscussionContext(owner)
                {
                    return Err(TranscriptAttachmentError::Identity);
                }
                (
                    thread
                        .committed_tail()
                        .ok_or(TranscriptAttachmentError::Unavailable)?,
                    *id.as_bytes(),
                )
            }
            DiscussionContextOwnerId::SubmittedTurn(id) => {
                let turn = self
                    .turn(id, point)
                    .map_err(attachment_read_error)?
                    .ok_or(TranscriptAttachmentError::Unavailable)?;
                if turn.origin_thread_id() != thread.id() {
                    return Err(TranscriptAttachmentError::Identity);
                }
                let ConversationParent::Turn(parent) = turn.parent() else {
                    return Err(TranscriptAttachmentError::Identity);
                };
                (parent, *id.as_bytes())
            }
        };
        let source_turn = self
            .turn(parent, point)
            .map_err(attachment_read_error)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        let mut low = 1u64;
        let mut high = head
            .entry_count()
            .checked_add(1)
            .ok_or(TranscriptAttachmentError::Capacity)?;
        // Selected transcript order follows monotonically increasing immutable turn depth.
        for _ in 0..64 {
            if low >= high {
                break;
            }
            self.check_request(request, cancelled)?;
            let mid = low + (high - low) / 2;
            let turn_id = self.context_entry_turn(request, head, mid, point)?;
            let turn = self
                .turn(turn_id, point)
                .map_err(attachment_read_error)?
                .ok_or(TranscriptAttachmentError::Unavailable)?;
            if turn.depth().get() <= source_turn.depth().get() {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let boundary = low
            .checked_sub(1)
            .filter(|position| *position > 0)
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        if self.context_entry_turn(request, head, boundary, point)? != parent {
            return Err(TranscriptAttachmentError::Identity);
        }
        self.check_request(request, cancelled)?;
        let envelope = self
            .context_envelope(owner, point)
            .ok()
            .flatten()
            .filter(|envelope| {
                let source = envelope.envelope().descriptor().source();
                envelope.owner() == owner
                    && source.turn_id() == parent
                    && Some(source.thread_id()) == thread.parent_thread_id()
            });
        let descriptor = envelope
            .as_ref()
            .map(|record| record.envelope().descriptor());
        let mut chunks = Vec::new();
        if let Some(envelope) = &envelope {
            let source = envelope.envelope().text().as_str();
            let source_start = envelope.envelope().descriptor().source_range().start();
            let mut start = 0;
            while start < source.len() {
                self.check_request(request, cancelled)?;
                let mut end = (start + CONTEXT_CHUNK_MAX_BYTES).min(source.len());
                while !source.is_char_boundary(end) {
                    end -= 1;
                }
                let text = &source[start..end];
                let plain = super::super::copy_text::stripped_span(
                    text,
                    if source_start == 0 && start == 0 {
                        1
                    } else {
                        2
                    },
                )
                .map_err(|_| TranscriptAttachmentError::Capacity)?;
                chunks.push(SyntheticContextChunk {
                    range: source_start + start as u64..source_start + end as u64,
                    text: Arc::from(text),
                    plain_text: Arc::from(plain),
                });
                start = end;
            }
        }
        let raw_owner: String = raw_owner.iter().map(|byte| format!("{byte:02x}")).collect();
        Ok(Some(SyntheticContextGroup {
            provenance: Arc::new(SyntheticContextProvenance {
                view_id: TranscriptViewId(thread.id().to_string()),
                group_id: format!("{}:{raw_owner}:{parent}", thread.id()),
                insertion_after: boundary,
                insertion_parent: parent,
                chunk_count: chunks.len().max(1),
                envelope_revision: envelope
                    .as_ref()
                    .map_or(0, |record| record.revision().get()),
                descriptor,
            }),
            chunks,
        }))
    }

    fn context_entry_turn(
        &self,
        request: &TranscriptAttachmentRequest,
        head: &TranscriptViewHeadRecord,
        position: u64,
        point: SyndicPointReadLimit,
    ) -> Result<beryl_model::SyndicTurnId, TranscriptAttachmentError> {
        let after = if position > 1 {
            Some(
                TranscriptPosition::new(position - 1)
                    .map_err(|_| TranscriptAttachmentError::Identity)?,
            )
        } else {
            None
        };
        let page = self
            .transcript_entries(
                request.thread_id,
                head.generation(),
                after,
                CursorReadLimits::new(1, ATTACHMENT_MAX_BYTES)
                    .map_err(|_| TranscriptAttachmentError::Capacity)?,
            )
            .map_err(attachment_read_error)?;
        let entry = page
            .records()
            .first()
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        if entry.position().get() != position
            || entry.generation() != head.generation()
            || entry.thread_id() != request.thread_id
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        let item = self
            .canonical_item(entry.item_id(), point)
            .map_err(attachment_read_error)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        if item.revision() != entry.item_revision() || item.id() != entry.item_id() {
            return Err(TranscriptAttachmentError::Stale);
        }
        Ok(item.turn_id())
    }

    pub(super) fn context_capacity_records(
        &self,
        request: &TranscriptAttachmentRequest,
        head: &TranscriptViewHeadRecord,
        context: &SyntheticContextGroup,
        point: SyndicPointReadLimit,
    ) -> Result<Option<(TranscriptViewRecord, ProjectionRecordSet)>, TranscriptAttachmentError>
    {
        let boundary = context.provenance.insertion_after;
        let first = match request.placement {
            TranscriptActivationPlacement::Tail => head
                .entry_count()
                .saturating_sub(ATTACHMENT_MAX_RECORDS as u64)
                .saturating_add(1),
            TranscriptActivationPlacement::Start => 1,
            TranscriptActivationPlacement::Position(position) => position.0,
        };
        if boundary < first || boundary >= first.saturating_add(ATTACHMENT_MAX_RECORDS as u64) {
            return Ok(None);
        }
        let after = if boundary > 1 {
            Some(
                TranscriptPosition::new(boundary - 1)
                    .map_err(|_| TranscriptAttachmentError::Identity)?,
            )
        } else {
            None
        };
        let page = self
            .transcript_entries(
                request.thread_id,
                head.generation(),
                after,
                CursorReadLimits::new(1, ATTACHMENT_MAX_BYTES)
                    .map_err(|_| TranscriptAttachmentError::Capacity)?,
            )
            .map_err(attachment_read_error)?;
        let entry = page
            .records()
            .first()
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        if entry.thread_id() != request.thread_id
            || entry.generation() != head.generation()
            || entry.position().get() != boundary
        {
            return Err(TranscriptAttachmentError::Identity);
        }
        let item = self
            .canonical_item(entry.item_id(), point)
            .map_err(attachment_read_error)?
            .ok_or(TranscriptAttachmentError::Unavailable)?;
        if item.turn_id() != context.provenance.insertion_parent
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
            CanonicalItemKind::GeneratedMedia => TranscriptNarrativeKind::AssistantGeneratedMedia,
            CanonicalItemKind::AssistantMessage(AssistantMessagePhase::Unknown)
            | CanonicalItemKind::ProviderText(_) => TranscriptNarrativeKind::AssistantText,
            _ => return Err(TranscriptAttachmentError::Identity),
        };
        let view_id = context.provenance.view_id.clone();
        let projection_id = ProjectionRecordId(entry.projection_id().to_string());
        let provenance = SyndicSourceProvenance {
            view_id: view_id.clone(),
            position: Some(TranscriptViewPosition(boundary)),
            turn_id: Some(SyndicTurnId(item.turn_id().to_string())),
            item_id: Some(SyndicItemId(item.id().to_string())),
            block_id: None,
            projection_id: Some(projection_id.clone()),
            resource_id: None,
            source_range: None,
            resource_range: None,
            copy_source_range: None,
        };
        let record = TranscriptViewRecord {
            id: TranscriptViewRecordId(format!(
                "{}:{}:{boundary}",
                request.thread_id,
                head.generation().get()
            )),
            position: TranscriptViewPosition(boundary),
            projection_id: projection_id.clone(),
            narrative_kind,
            provenance,
        };
        let set = ProjectionRecordSet {
            view_id,
            revision: ProviderRevision(head.revision().get()),
            records: Vec::new(),
            rejections: vec![crate::syndic_transcript::TranscriptProviderRejection {
                target: crate::syndic_transcript::TranscriptProviderTarget::ProjectionRecord(
                    projection_id,
                ),
                reason: crate::syndic_transcript::TranscriptProviderRejectionReason::BudgetExceeded,
                revision: Some(ProviderRevision(entry.projection_revision().get())),
                message: None,
            }],
        };
        Ok(Some((record, set)))
    }
}

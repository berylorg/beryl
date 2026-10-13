use super::super::*;
use crate::{DraftRecord, DraftSubmissionIntent, ReplacementEditIntent, SyndicTimestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DraftReplacementCancellationRequestV1 {
    pub(crate) selector: DraftEditorCurrentSelectorV1,
    pub(crate) session: DraftEditorCandidateSessionIdV1,
    pub(crate) generation: u64,
    pub(crate) operation: DraftPieceOperationIdV1,
    pub(crate) intent: ReplacementEditIntent,
    pub(crate) at: SyndicTimestamp,
}
impl DraftReplacementCancellationRequestV1 {
    pub const fn new(
        selector: DraftEditorCurrentSelectorV1,
        session: DraftEditorCandidateSessionIdV1,
        generation: u64,
        operation: DraftPieceOperationIdV1,
        intent: ReplacementEditIntent,
        at: SyndicTimestamp,
    ) -> Self {
        Self {
            selector,
            session,
            generation,
            operation,
            intent,
            at,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftReplacementCancellationReceiptV1 {
    pub(crate) request: DraftReplacementCancellationRequestV1,
    pub(crate) request_bytes: Vec<u8>,
    pub(crate) prior_selector: DraftEditorCurrentSelectorV1,
    pub(crate) successor_selector: DraftEditorCurrentSelectorV1,
    pub(crate) before: Box<DraftEditorCandidateSessionV1>,
    pub(crate) after: Box<DraftEditorCandidateSessionV1>,
    pub(crate) prior_draft: Box<DraftRecord>,
    pub(crate) successor_draft: Box<DraftRecord>,
    pub(crate) newest_history: Box<DraftEditHistoryFrontierV1>,
    pub(crate) mutable_history: Box<DraftEditHistoryFrontierV1>,
}
impl DraftReplacementCancellationReceiptV1 {
    pub fn request_bytes(&self) -> &[u8] {
        &self.request_bytes
    }
    pub fn selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.successor_selector
    }
    pub fn before_head(&self) -> &DraftEditorCandidateSessionV1 {
        &self.before
    }
    pub fn after_head(&self) -> &DraftEditorCandidateSessionV1 {
        &self.after
    }
    pub(crate) fn key(&self) -> DraftEditorCandidateSessionRecordKeyV1 {
        DraftEditorCandidateSessionRecordKeyV1::replacement_intent_cancellation_receipt(
            self.prior_selector.draft_id(),
            self.request.session,
            self.request.operation,
        )
    }
    pub(crate) fn is_locally_exact(&self) -> bool {
        let r = self.request;
        let Some(revision) = self.prior_draft.revision().checked_next().ok() else {
            return false;
        };
        let expected = DraftRecord::new(
            self.prior_draft.id(),
            self.prior_draft.thread_id(),
            revision,
            DraftSubmissionIntent::Ordinary,
            self.prior_draft.root_history(),
            self.prior_draft.created_at(),
            r.at,
        );
        let next_selector = DraftEditorCurrentSelectorV1::new(
            r.selector.thread_id(),
            r.selector.thread_revision(),
            r.selector.draft_id(),
            revision,
            r.selector.root(),
            r.selector.history(),
        );
        r.generation != 0
            && r.operation.as_bytes() != &[0; 16]
            && r.session.as_bytes() != &[0; 16]
            && self.request_bytes == super::cancellation_codec::canonical_cancellation_request(r)
            && r.selector == self.prior_selector
            && self.successor_selector == next_selector
            && self.prior_draft.id() == r.selector.draft_id()
            && self.prior_draft.thread_id() == r.selector.thread_id()
            && self.prior_draft.revision() == r.selector.selector_revision()
            && self.prior_draft.piece_root() == r.selector.root()
            && self.prior_draft.history() == r.selector.history()
            && self.prior_draft.submission_intent() == DraftSubmissionIntent::Replacement(r.intent)
            && r.at >= self.prior_draft.updated_at()
            && self.successor_draft.as_ref() == &expected
            && self.before.session_id() == r.session
            && self.before.session_generation() == r.generation
            && self.before.open_operation_id() != r.operation
            && self.before.draft_id() == r.selector.draft_id()
            && self.before.thread_id() == r.selector.thread_id()
            && self.before.published_selector_revision() == r.selector.selector_revision()
            && self.before.published_root() == r.selector.root()
            && self.before.published_history() == r.selector.history()
            && cancelled_session(&self.before, next_selector).as_ref() == Some(&self.after)
            && self.newest_history.reference() == self.before.newest_history()
            && self.newest_history.is_locally_valid()
            && self.mutable_history.reference().key()
                == DraftEditHistoryFrontierKeyV1::session(self.before.draft_id(), r.session)
            && self.mutable_history.is_locally_valid()
            && (self.mutable_history.reference() == self.newest_history.reference()
                || self.newest_history.fork_session(r.session).as_ref()
                    == Some(self.mutable_history.as_ref()))
    }
}
pub(crate) fn cancelled_session(
    before: &DraftEditorCandidateSessionV1,
    selector: DraftEditorCurrentSelectorV1,
) -> Option<Box<DraftEditorCandidateSessionV1>> {
    if before.active_operation().is_some()
        || before.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
        || !before.is_coherent()
    {
        return None;
    }
    let next = DraftEditorCandidateSessionV1::from_parts_with_disposal(
        before.thread_id(),
        before.draft_id(),
        before.session_id(),
        before.open_operation_id(),
        before.session_generation().checked_add(1)?,
        before.durable_base_selector_revision(),
        before.durable_base_root(),
        before.durable_base_history(),
        before.published_candidate_generation(),
        selector.selector_revision(),
        before.published_root(),
        before.published_history(),
        before.newest_candidate_generation(),
        before.newest_root(),
        before.newest_history(),
        before.dirty_generation(),
        before.logical_extent(),
        before.lifecycle(),
        before.disposal_operation_id(),
        None,
    );
    next.is_coherent().then(|| Box::new(next))
}

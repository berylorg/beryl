use super::super::*;
use crate::{DraftRecord, DraftSubmissionIntent, HistorySummaryRecord};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftSealedImportSelectionV1 {
    pub(crate) prior_selector: DraftEditorCurrentSelectorV1,
    pub(crate) successor_selector: DraftEditorCurrentSelectorV1,
    pub(crate) before_session: Box<DraftEditorCandidateSessionV1>,
    pub(crate) after_session: Box<DraftEditorCandidateSessionV1>,
    pub(crate) imported_root: DraftPieceRootReferenceV1,
    pub(crate) prior_history: Box<DraftEditHistoryFrontierV1>,
    pub(crate) baseline: Box<DraftEditHistoryFrontierV1>,
    pub(crate) before_mutable_history: Box<DraftEditHistoryFrontierV1>,
    pub(crate) after_mutable_history: Box<DraftEditHistoryFrontierV1>,
    pub(crate) basis: DraftSealedImportBasisV1,
    pub(crate) prior_draft: Box<DraftRecord>,
    pub(crate) successor_draft: Box<DraftRecord>,
    pub(crate) prior_summary: Box<HistorySummaryRecord>,
    pub(crate) successor_summary: Box<HistorySummaryRecord>,
}
impl DraftSealedImportSelectionV1 {
    pub fn prior_selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.prior_selector
    }
    pub fn successor_selector(&self) -> DraftEditorCurrentSelectorV1 {
        self.successor_selector
    }
    pub fn after_session(&self) -> &DraftEditorCandidateSessionV1 {
        &self.after_session
    }
    pub fn baseline(&self) -> &DraftEditHistoryFrontierV1 {
        &self.baseline
    }
    pub fn session_fork(&self) -> &DraftEditHistoryFrontierV1 {
        &self.after_mutable_history
    }
    pub fn basis(&self) -> DraftSealedImportBasisV1 {
        self.basis
    }

    pub(crate) fn is_locally_exact(&self, source: &DraftPieceBuildRecordV1) -> bool {
        let s = &self.before_session;
        let DraftPieceBuildFrontierV1::SealedImport(f) = source.frontier() else {
            return false;
        };
        if !import_build_is_exact(source)
            || source.lifecycle() != DraftPieceBuildLifecycleV1::Complete
            || source.successor() != Some(self.imported_root)
            || f.basis() != self.basis
            || self.prior_selector != self.basis.selector()
            || s.active_operation() != Some(&super::command::custody(source))
            || s.newest_root() != source.predecessor_root()
            || s.newest_history() != source.predecessor_history()
            || s.newest_candidate_generation() != source.predecessor_candidate_generation()
            || s.session_id() != source.session_id()
            || s.draft_id() != source.draft_id()
            || s.thread_id() != self.basis.selector().thread_id()
            || import_header_generation(source)
                .and_then(|g| g.checked_add(source.progress_receipt().key().transition_ordinal()))
                != Some(s.session_generation())
            || !s.is_coherent()
            || !candidate_session_has_saved_identity(s)
            || self.prior_history.reference() != source.predecessor_history()
            || !self.prior_history.is_locally_valid()
            || !self.before_mutable_history.is_locally_valid()
            || self.before_mutable_history.reference().key()
                != DraftEditHistoryFrontierKeyV1::session(s.draft_id(), s.session_id())
            || self.before_mutable_history.reference() != self.prior_history.reference()
                && self.prior_history.fork_session(s.session_id()).as_ref()
                    != Some(&self.before_mutable_history)
            || self
                .before_mutable_history
                .sealed_import_baseline(s.session_id(), source.operation_id(), self.imported_root)
                .as_ref()
                .is_none_or(|(baseline, fork)| {
                    baseline != self.baseline.as_ref()
                        || fork != self.after_mutable_history.as_ref()
                })
        {
            return false;
        }
        let Some(revision) = self.prior_draft.revision().checked_next().ok() else {
            return false;
        };
        let selector = DraftEditorCurrentSelectorV1::new(
            self.prior_selector.thread_id(),
            self.prior_selector.thread_revision(),
            s.draft_id(),
            revision,
            self.imported_root,
            self.baseline.reference(),
        );
        let expected = DraftRecord::new(
            s.draft_id(),
            s.thread_id(),
            revision,
            DraftSubmissionIntent::Replacement(self.basis.intent()),
            DraftRootHistoryPairV1::new(self.imported_root, self.baseline.reference()),
            self.prior_draft.created_at(),
            self.successor_draft.updated_at(),
        );
        let old_summary = &self.prior_summary;
        let Some(summary_revision) = old_summary.revision().checked_next().ok() else {
            return false;
        };
        let expected_summary = HistorySummaryRecord::new(
            old_summary.thread_id(),
            summary_revision,
            old_summary.thread_revision(),
            old_summary.committed_tail(),
            old_summary.selected_path_digest(),
            old_summary.complete(),
            old_summary
                .last_activity_at()
                .max(self.successor_draft.updated_at()),
        );
        self.prior_draft.id() == s.draft_id()
            && self.prior_draft.thread_id() == s.thread_id()
            && self.prior_draft.revision() == self.prior_selector.selector_revision()
            && self.prior_draft.piece_root() == self.prior_selector.root()
            && self.prior_draft.history() == self.prior_selector.history()
            && self.prior_draft.submission_intent() == DraftSubmissionIntent::Ordinary
            && self.successor_draft.updated_at() >= self.prior_draft.updated_at()
            && self.successor_draft.as_ref() == &expected
            && self.successor_selector == selector
            && old_summary.thread_id() == s.thread_id()
            && old_summary.thread_revision() == self.prior_selector.thread_revision()
            && old_summary.committed_tail() == self.basis.intent.selected_path().tail()
            && old_summary.selected_path_digest() == self.basis.intent.selected_path().digest()
            && self.successor_summary.as_ref() == &expected_summary
            && imported_session(s, selector).as_ref() == Some(&self.after_session)
    }
}

pub(crate) fn imported_session(
    before: &DraftEditorCandidateSessionV1,
    selector: DraftEditorCurrentSelectorV1,
) -> Option<Box<DraftEditorCandidateSessionV1>> {
    let generation = before.newest_candidate_generation().checked_add(1)?;
    let next = DraftEditorCandidateSessionV1::from_parts_with_disposal(
        before.thread_id(),
        before.draft_id(),
        before.session_id(),
        before.open_operation_id(),
        before.session_generation().checked_add(1)?,
        before.durable_base_selector_revision(),
        before.durable_base_root(),
        before.durable_base_history(),
        generation,
        selector.selector_revision(),
        selector.root(),
        selector.history(),
        generation,
        selector.root(),
        selector.history(),
        before.dirty_generation(),
        selector.root().summary().logical_extent(),
        before.lifecycle(),
        before.disposal_operation_id(),
        None,
    );
    (before.lifecycle() == DraftEditorCandidateSessionLifecycleV1::Active && next.is_coherent())
        .then(|| Box::new(next))
}

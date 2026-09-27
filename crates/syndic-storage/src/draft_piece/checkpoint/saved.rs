use super::*;
use crate::read::access::ReadAccess;
use beryl_home_store::HomeCandidateRecoveryAccess;

impl SyndicStorage {
    pub fn draft_editor_candidate_is_saved(
        &self,
        store: &HomeStore,
        candidate: DraftEditorCandidateActivationBindingV1,
        selector: DraftEditorCurrentSelectorV1,
    ) -> Result<bool, DraftEditorCandidatePublicationCommandErrorV1> {
        self.draft_editor_candidate_is_saved_with_access(
            ReadAccess::Ordinary(store),
            candidate,
            selector,
        )
    }

    pub fn draft_editor_candidate_is_saved_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        candidate: DraftEditorCandidateActivationBindingV1,
        selector: DraftEditorCurrentSelectorV1,
    ) -> Result<bool, DraftEditorCandidatePublicationCommandErrorV1> {
        self.draft_editor_candidate_is_saved_with_access(
            ReadAccess::Candidate(store),
            candidate,
            selector,
        )
    }

    fn draft_editor_candidate_is_saved_with_access(
        &self,
        store: ReadAccess<'_>,
        candidate: DraftEditorCandidateActivationBindingV1,
        selector: DraftEditorCurrentSelectorV1,
    ) -> Result<bool, DraftEditorCandidatePublicationCommandErrorV1> {
        self.revision_with_access(store)
            .map_err(SyndicReadError::Read)?;
        if current_selector(self, store, selector.thread_id())? != selector {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "saved candidate checkpoint observation",
            }
            .into());
        }
        let key = DraftEditorCandidateSessionRecordKeyV1::head(
            candidate.draft_id(),
            candidate.session_id(),
        );
        let Some(DraftEditorCandidateSessionRecordV1::Head(head)) =
            self.point_with_access::<DraftEditorCandidateSessionsFamily>(
                store,
                key,
                point_limit(),
            )?
        else {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        };
        if head.lifecycle() != DraftEditorCandidateSessionLifecycleV1::Active
            || DraftEditorCandidateActivationBindingV1::from_head(&head) != candidate
            || head.thread_id() != selector.thread_id()
            || head.draft_id() != selector.draft_id()
            || head.published_selector_revision() != selector.selector_revision()
            || head.published_root() != selector.root()
            || head.published_history() != selector.history()
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        if head.active_operation().is_some() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::ActiveOperation);
        }
        if !session::idle_candidate_closure_is_exact_with_access(self, store, &head)? {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        let saved = has_saved_identity(&head);
        let last_head = self.point_with_access::<DraftEditorCandidateSessionsFamily>(
            store,
            key,
            point_limit(),
        )?;
        if last_head.as_ref() != Some(&DraftEditorCandidateSessionRecordV1::Head(head))
            || current_selector(self, store, selector.thread_id())? != selector
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "saved candidate checkpoint observation",
            }
            .into());
        }
        Ok(saved)
    }
}

fn current_selector(
    storage: &SyndicStorage,
    store: ReadAccess<'_>,
    thread: beryl_model::SyndicThreadId,
) -> Result<DraftEditorCurrentSelectorV1, DraftEditorCandidatePublicationCommandErrorV1> {
    let current = storage
        .current_draft_with_access(store, thread, point_limit())?
        .ok_or(DraftEditorCandidatePublicationCommandErrorV1::Invariant)?;
    Ok(DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    ))
}

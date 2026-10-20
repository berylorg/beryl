use super::*;
use crate::read::access::ReadAccess;
use beryl_home_store::HomeCandidateRecoveryAccess;

impl SyndicStorage {
    pub fn qualify_draft_editor_candidate_selector(
        &self,
        store: &HomeStore,
        candidate: DraftEditorCandidateActivationBindingV1,
        prior: DraftEditorCurrentSelectorV1,
    ) -> Result<DraftEditorCurrentSelectorV1, DraftEditorCandidatePublicationCommandErrorV1> {
        self.qualify_draft_editor_candidate_selector_with_access(
            ReadAccess::Ordinary(store),
            candidate,
            prior,
        )
    }

    pub fn qualify_draft_editor_candidate_selector_candidate(
        &self,
        store: &HomeCandidateRecoveryAccess<'_>,
        candidate: DraftEditorCandidateActivationBindingV1,
        prior: DraftEditorCurrentSelectorV1,
    ) -> Result<DraftEditorCurrentSelectorV1, DraftEditorCandidatePublicationCommandErrorV1> {
        self.qualify_draft_editor_candidate_selector_with_access(
            ReadAccess::Candidate(store),
            candidate,
            prior,
        )
    }

    fn qualify_draft_editor_candidate_selector_with_access(
        &self,
        store: ReadAccess<'_>,
        candidate: DraftEditorCandidateActivationBindingV1,
        prior: DraftEditorCurrentSelectorV1,
    ) -> Result<DraftEditorCurrentSelectorV1, DraftEditorCandidatePublicationCommandErrorV1> {
        let revision = self
            .revision_with_access(store)
            .map_err(SyndicReadError::Read)?;
        let current = saved::current_selector(self, store, prior.thread_id())?;
        if current.thread_id() != prior.thread_id()
            || current.thread_revision() < prior.thread_revision()
            || current.draft_id() != prior.draft_id()
            || current.selector_revision() != prior.selector_revision()
            || current.root() != prior.root()
            || current.history() != prior.history()
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "editor candidate checkpoint selector qualification",
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
            || head.thread_id() != prior.thread_id()
            || head.draft_id() != prior.draft_id()
            || head.published_selector_revision() != prior.selector_revision()
            || head.published_root() != prior.root()
            || head.published_history() != prior.history()
        {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        if head.active_operation().is_some() {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::ActiveOperation);
        }
        if !session::idle_candidate_closure_is_exact_with_access(self, store, &head)? {
            return Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant);
        }
        if self
            .revision_with_access(store)
            .map_err(SyndicReadError::Read)?
            != revision
        {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "editor candidate checkpoint selector qualification",
            }
            .into());
        }
        Ok(current)
    }
}

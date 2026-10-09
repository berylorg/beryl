use super::*;
use crate::composer_host::{
    ComposerHostDisposalExecutionIdentity, ComposerHostPublicationExecutionIdentity,
};
use crate::main_window::MainWindowComposerSelectionIdentity;
use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftEditorCandidateSessionV1, DraftRecord,
    SyndicPointReadLimit, SyndicStorage,
};

pub(super) struct OriginalDurableEvidence {
    selection: MainWindowComposerSelectionIdentity,
    before: DraftRecord,
    settled: Option<DraftRecord>,
    disposed: Option<Box<DraftEditorCandidateSessionV1>>,
}

impl OriginalDurableEvidence {
    pub(super) fn capture(
        home: &beryl_home_store::HomeStore,
        storage: &SyndicStorage,
        selection: MainWindowComposerSelectionIdentity,
    ) -> Box<Self> {
        let current = storage
            .current_draft(
                home,
                selection.claim().thread_id(),
                SyndicPointReadLimit::new(1024 * 1024).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(current.draft().thread_id(), selection.claim().thread_id());
        assert_eq!(
            current.draft().id(),
            selection.binding().candidate().draft_id()
        );
        Box::new(Self {
            selection,
            before: current.draft().clone(),
            settled: None,
            disposed: None,
        })
    }

    pub(super) fn verify_settled(
        &mut self,
        home: &beryl_home_store::HomeStore,
        storage: &SyndicStorage,
        cut: Cut,
        original_save: ComposerHostPublicationExecutionIdentity,
        original_disposal: Option<ComposerHostDisposalExecutionIdentity>,
    ) {
        assert!(self.settled.is_none());
        assert_eq!(original_save.draft_id, self.before.id());
        assert_eq!(
            original_save.session_id,
            self.selection.binding().candidate().session_id()
        );
        let current = self.read_draft(home, storage);
        assert_eq!(
            current.revision(),
            self.before.revision().checked_next().unwrap()
        );
        assert_eq!(current.history().key().draft_id(), original_save.draft_id);
        assert_eq!(
            current.history().key().session_id(),
            Some(original_save.session_id)
        );
        if matches!(cut, Cut::SaveNoncommit) {
            let recovery_operation = current
                .history()
                .key()
                .publication_operation_id()
                .expect("captured dirty checkpoint has an independent recovery publication");
            assert_ne!(recovery_operation, original_save.operation_id);
        } else {
            assert_eq!(
                current.history().key().publication_operation_id(),
                Some(original_save.operation_id)
            );
        }
        let captured = self.selection.binding().history();
        let published = current.history();
        assert_eq!(current.piece_root(), self.selection.binding().root());
        assert_eq!(published.root(), captured.root());
        assert_eq!(
            published.candidate_generation(),
            captured.candidate_generation()
        );
        assert_eq!(published.frontier_revision(), captured.frontier_revision());
        assert_eq!(published.byte_budget(), captured.byte_budget());
        assert_eq!(
            published.retention_policy_revision(),
            captured.retention_policy_revision()
        );
        assert_eq!(published.availability(), captured.availability());
        if cut.committed() {
            let disposed = self.read_disposed(home, storage);
            assert_eq!(disposed.thread_id(), self.before.thread_id());
            assert_eq!(disposed.draft_id(), original_save.draft_id);
            assert_eq!(disposed.session_id(), original_save.session_id);
            assert_eq!(disposed.published_selector_revision(), current.revision());
            assert_eq!(disposed.published_root(), current.piece_root());
            assert_eq!(disposed.published_history(), current.history());
            assert!(disposed.disposal_operation_id().is_some());
            if matches!(cut, Cut::DisposalCommitted) {
                let original_disposal =
                    original_disposal.expect("completed original disposal execution is missing");
                assert_eq!(
                    disposed.disposal_operation_id(),
                    Some(original_disposal.operation_id)
                );
            }
            self.disposed = Some(disposed);
        }
        self.settled = Some(current);
    }

    pub(super) fn verify_saved_noop(
        &mut self,
        home: &beryl_home_store::HomeStore,
        storage: &SyndicStorage,
        cut: Cut,
        original_disposal: ComposerHostDisposalExecutionIdentity,
    ) {
        assert!(matches!(cut, Cut::DisposalCommitted));
        assert!(self.settled.is_none());
        let current = self.read_draft(home, storage);
        assert_eq!(current, self.before);
        assert_eq!(current.piece_root(), self.selection.binding().root());
        assert_eq!(current.history(), self.selection.binding().history());
        let disposed = self.read_disposed(home, storage);
        assert_eq!(disposed.thread_id(), self.before.thread_id());
        assert_eq!(disposed.draft_id(), original_disposal.draft_id);
        assert_eq!(disposed.session_id(), original_disposal.session_id);
        assert_eq!(disposed.published_selector_revision(), current.revision());
        assert_eq!(disposed.published_root(), current.piece_root());
        assert_eq!(disposed.published_history(), current.history());
        assert_eq!(
            disposed.disposal_operation_id(),
            Some(original_disposal.operation_id)
        );
        self.disposed = Some(disposed);
        self.settled = Some(current);
    }

    pub(super) fn verify_unchanged(
        &self,
        home: &beryl_home_store::HomeStore,
        storage: &SyndicStorage,
    ) {
        assert_eq!(
            &self.read_draft(home, storage),
            self.settled.as_ref().unwrap()
        );
        if let Some(disposed) = &self.disposed {
            assert_eq!(&self.read_disposed(home, storage), disposed);
        }
    }

    fn read_draft(
        &self,
        home: &beryl_home_store::HomeStore,
        storage: &SyndicStorage,
    ) -> DraftRecord {
        let current = storage
            .current_draft(
                home,
                self.before.thread_id(),
                SyndicPointReadLimit::new(1024 * 1024).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(current.draft().id(), self.before.id());
        current.draft().clone()
    }

    fn read_disposed(
        &self,
        home: &beryl_home_store::HomeStore,
        storage: &SyndicStorage,
    ) -> Box<DraftEditorCandidateSessionV1> {
        let outcome = storage
            .draft_editor_candidate_session(
                home,
                self.before.id(),
                self.selection.binding().candidate().session_id(),
            )
            .unwrap();
        let DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) = outcome else {
            panic!("original predecessor session was not exactly disposed: {outcome:?}");
        };
        Box::new(head)
    }
}

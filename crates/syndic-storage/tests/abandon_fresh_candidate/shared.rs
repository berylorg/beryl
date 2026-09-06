use crate::support::*;

use syndic_storage::{
    DraftEditorCandidatePublicationEvidenceV1, DraftEditorCandidatePublicationOutcomeV1,
    DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1,
    DraftEditorCandidateSessionDisposeRequestV1, DraftRootHistoryPairV1,
    PreparedDraftEditorCandidatePublicationV1,
};

pub(crate) fn abandon_request(
    head: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> DraftEditorCandidateSessionDisposeRequestV1 {
    DraftEditorCandidateSessionDisposeRequestV1::new(
        head.draft_id(),
        head.session_id(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        head.session_generation(),
        DraftRootHistoryPairV1::new(head.newest_root(), head.newest_history()),
    )
}

pub(crate) fn head(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
) -> DraftEditorCandidateSessionV1 {
    match storage
        .draft_editor_candidate_session(store, session.draft_id(), session.session_id())
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head)
        | DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => head,
        other => panic!("session unavailable: {other:?}"),
    }
}

pub(crate) fn recover_if_failed(
    store: HomeStore,
    storage: SyndicStorage,
) -> (HomeStore, SyndicStorage) {
    if store.health().state() == HomeHealthState::Failed {
        let recovery = store.recover_same_home().unwrap();
        let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
        (recovery.publish(), storage)
    } else {
        (store, storage)
    }
}

pub(crate) fn prepare_candidate_publication(
    storage: &SyndicStorage,
    store: &HomeStore,
    selected: &syndic_storage::SyndicCurrentDraft,
    candidate: &DraftEditorCandidateSessionV1,
    operation: u8,
) -> PreparedDraftEditorCandidatePublicationV1 {
    let request = DraftEditorCandidatePublicationRequestV1::new(
        selector(selected),
        candidate.session_id(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        candidate.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(candidate.newest_root(), candidate.newest_history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(2),
    );
    let source = storage
        .capture_draft_editor_candidate_publication_source(
            store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                request.selector(),
                syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(candidate),
                request.operation_id(),
                request.published_at(),
            ),
        )
        .unwrap();
    storage
        .prepare_draft_editor_candidate_publication(store, source, request.evidence())
        .unwrap()
}

pub(crate) fn publish_candidate(
    storage: &SyndicStorage,
    store: &HomeStore,
    selected: &syndic_storage::SyndicCurrentDraft,
    candidate: &DraftEditorCandidateSessionV1,
    operation: u8,
) {
    let prepared = prepare_candidate_publication(storage, store, selected, candidate, operation);
    let outcome = execute(
        store,
        storage.publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
}

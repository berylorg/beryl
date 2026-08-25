use beryl_app::composer_host::ComposerHostBinding;
use beryl_home_store::HomeStore;
use syndic_storage::{
    CapturedDraftEditorCandidatePublicationSourceV1, DraftEditorCandidatePublicationEvidenceV1,
    DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1, DraftEditorCurrentSelectorV1,
    DraftRootHistoryPairV1, SyndicStorage, SyndicTimestamp,
};

use super::{base, composer};

pub fn selector(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: beryl_model::SyndicThreadId,
) -> DraftEditorCurrentSelectorV1 {
    let durable = base::current(storage, store, thread);
    DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        durable.thread().revision(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    )
}

pub fn publication_request(
    selector: DraftEditorCurrentSelectorV1,
    binding: ComposerHostBinding,
    operation: u64,
    published_at: u64,
) -> DraftEditorCandidatePublicationRequestV1 {
    DraftEditorCandidatePublicationRequestV1::new(
        selector,
        binding.candidate().session_id(),
        composer::operation_id(operation),
        binding.candidate().candidate_generation(),
        DraftRootHistoryPairV1::new(binding.root(), binding.history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(published_at),
    )
}

pub fn capture_source(
    store: &HomeStore,
    storage: SyndicStorage,
    request: DraftEditorCandidatePublicationRequestV1,
) -> CapturedDraftEditorCandidatePublicationSourceV1 {
    let session = storage
        .draft_editor_candidate_session(store, request.selector().draft_id(), request.session_id())
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    storage
        .capture_draft_editor_candidate_publication_source(
            store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                request.selector(),
                syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&session),
                request.operation_id(),
                request.published_at(),
            ),
        )
        .unwrap()
}

pub fn publish_source(
    store: &HomeStore,
    storage: SyndicStorage,
    source: CapturedDraftEditorCandidatePublicationSourceV1,
    request: DraftEditorCandidatePublicationRequestV1,
) -> syndic_storage::DraftEditorCandidatePublicationOutcomeV1 {
    let prepared = storage
        .prepare_draft_editor_candidate_publication(store, source, request.evidence())
        .unwrap();
    let outcome = base::execute(
        store,
        storage.publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
    );
    storage
        .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
        .unwrap()
}

pub fn publish_binding(
    store: &HomeStore,
    storage: SyndicStorage,
    selector: DraftEditorCurrentSelectorV1,
    binding: ComposerHostBinding,
    operation: u64,
    published_at: u64,
) {
    let request = publication_request(selector, binding, operation, published_at);
    let source = capture_source(store, storage, request);
    assert!(matches!(
        publish_source(store, storage, source, request),
        syndic_storage::DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
            | syndic_storage::DraftEditorCandidatePublicationOutcomeV1::ExactReplay(_)
    ));
}

pub fn dispose_binding(
    store: &HomeStore,
    storage: SyndicStorage,
    binding: ComposerHostBinding,
    operation: u64,
) {
    let session = storage
        .draft_editor_candidate_session(
            store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap();
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("candidate session was not active")
    };
    let request = syndic_storage::DraftEditorCandidateSessionDisposeRequestV1::new(
        session.draft_id(),
        session.session_id(),
        composer::operation_id(operation),
        session.session_generation(),
        DraftRootHistoryPairV1::new(session.published_root(), session.published_history()),
    );
    let prepared = storage
        .prepare_dispose_draft_editor_candidate_session(store, request)
        .unwrap();
    let outcome = base::execute(
        store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_disposal(store, &prepared, outcome)
            .unwrap(),
        syndic_storage::DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
            | syndic_storage::DraftEditorCandidateSessionDisposeOutcomeV1::ExactReplay(_)
    ));
}

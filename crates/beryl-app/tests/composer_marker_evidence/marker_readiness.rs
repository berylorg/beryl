use beryl_app::composer_host::{ComposerHostBinding, ComposerHostError, SyndicComposerHost};
use beryl_home_store::HomeStore;
use gpui_text_input::MutationBeginRequest;
use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftMarkerAdmissionOperationIdV1,
    DraftMarkerAdmissionOwnerV1, DraftPieceMarkerV1, SyndicStorage,
};

pub fn begin_with_markers(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    begin: MutationBeginRequest,
    markers: &[DraftPieceMarkerV1],
) -> Result<(), ComposerHostError> {
    if markers.is_empty() {
        return host.begin_mutation(store, binding, begin);
    }
    let storage = SyndicStorage::reacquire(store).unwrap();
    let DraftEditorCandidateSessionReadOutcomeV1::Active(session) = storage
        .draft_editor_candidate_session(
            store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap()
    else {
        panic!("marker fixture candidate session is inactive");
    };
    let mut bytes = [0; 16];
    bytes[8..].copy_from_slice(&begin.proposal().key().operation().get().to_be_bytes());
    let owner = DraftMarkerAdmissionOwnerV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes(bytes),
    );
    let readiness = storage
        .seed_draft_marker_writer_ready_targets_for_test(store, &session, owner, markers)
        .unwrap();
    host.test_begin_marker_mutation(store, binding, begin, readiness)
}

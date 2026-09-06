use super::{edit_support::commit_edit, publication_support::*, support::*};
use syndic_storage::{
    DraftEditorCandidatePublicationEvidenceV1, DraftEditorCandidatePublicationOutcomeV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1,
};

#[test]
fn captured_edited_checkpoint_publishes_without_relabeling_its_later_live_candidate_as_saved() {
    let (_home, store, storage, thread) = fixture("captured-older", 130, 65_536);
    let durable = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &durable, 132, 133);
    let first = commit_edit(&storage, &store, &opened, 134, "older");
    let captured = first.adopted_session();
    let source = storage
        .capture_draft_editor_candidate_publication_source(
            &store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                selector(&durable),
                DraftEditorCandidateActivationBindingV1::from_head(captured),
                DraftPieceOperationIdV1::from_bytes([135; 16]),
                SyndicTimestamp::from_unix_millis(3),
            ),
        )
        .unwrap();
    let later = commit_edit(&storage, &store, captured, 136, "newer");
    let latest = later.adopted_session();
    let prepared = storage
        .prepare_draft_editor_candidate_publication(
            &store,
            source,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        )
        .unwrap();
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let live = head(&storage, &store, latest);
    assert_eq!(live.newest_root(), latest.newest_root());
    assert_eq!(live.newest_history(), latest.newest_history());
    assert_eq!(
        live.newest_candidate_generation(),
        latest.newest_candidate_generation()
    );
    assert_eq!(
        live.published_candidate_generation(),
        captured.newest_candidate_generation()
    );
    let published = current(&storage, &store, thread);
    assert_eq!(published.draft().piece_root(), captured.newest_root());
    assert!(
        !storage
            .draft_editor_candidate_is_saved(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(&live),
                selector(&published)
            )
            .unwrap()
    );
}

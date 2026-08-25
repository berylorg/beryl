use crate::{shared::*, support::*};

use syndic_storage::{
    DraftEditorCandidatePublicationCommandErrorV1, DraftEditorCandidatePublicationOutcomeV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1,
    DraftEditorCandidateSessionAbandonFreshOutcomeV1,
};

#[test]
fn abandonment_and_mutation_serialize_to_one_winner() {
    let (_home, store, storage, thread) = fixture("abandon-race-mutation", 70, 65_536);
    let selected = current(storage, &store, thread);
    let opened = open_session(storage, &store, &selected, 72, 73);
    let mutation = transaction(storage, &store, &opened, 74, "x", point(1));
    let request = abandon_request(&opened, 75);
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let shared_revision = storage.revision(&store).unwrap();
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(shared_revision, mutation.prepared.clone()),
    ));
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(shared_revision, prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::NotFresh(_)
    ));

    let (_home, store, storage, thread) = fixture("abandon-race-wins", 80, 65_536);
    let selected = current(storage, &store, thread);
    let opened = open_session(storage, &store, &selected, 82, 83);
    let mutation = transaction(storage, &store, &opened, 84, "x", point(1));
    let request = abandon_request(&opened, 85);
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let shared_revision = storage.revision(&store).unwrap();
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(shared_revision, prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
    ));
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(shared_revision, mutation.prepared),
    ));
}

#[test]
fn abandonment_and_publication_serialize_in_both_orders() {
    let (_home, store, storage, thread) = fixture("publication-race-wins", 130, 65_536);
    let selected = current(storage, &store, thread);
    let original_selector = selector(&selected);
    let opened = open_session(storage, &store, &selected, 132, 133);
    let pristine_request = abandon_request(&opened, 134);
    let abandonment = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, pristine_request)
        .unwrap();
    let edit = transaction(storage, &store, &opened, 135, "x", point(1));
    build(storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let dirty = match settled(storage, &store, &edit).closure() {
        DraftPieceSettlementClosureV1::Committed(adoption) => adoption.adopted_session().clone(),
        other => panic!("publication race edit failed: {other:?}"),
    };
    let publication = prepare_candidate_publication(storage, &store, &selected, &dirty, 136);
    let published = execute(
        &store,
        storage
            .publish_draft_editor_candidate(storage.revision(&store).unwrap(), publication.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(&store, &publication, published)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let published_selector = selector(&current(storage, &store, thread));
    let published_head = head(storage, &store, &opened);
    assert_ne!(published_selector, original_selector);
    let abandoned = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            abandonment.clone(),
        ),
    );
    match storage
        .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &abandonment, abandoned)
        .unwrap()
    {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::NotFresh(observed) => {
            assert_eq!(observed, published_head)
        }
        other => panic!("published descendant did not reject stale abandonment: {other:?}"),
    }
    assert_eq!(head(storage, &store, &opened), published_head);
    assert_eq!(
        selector(&current(storage, &store, thread)),
        published_selector
    );

    let (_home, store, storage, thread) = fixture("abandonment-race-wins", 140, 65_536);
    let selected = current(storage, &store, thread);
    let original_selector = selector(&selected);
    let opened = open_session(storage, &store, &selected, 142, 143);
    let edit = transaction(storage, &store, &opened, 144, "x", point(1));
    let request = abandon_request(&opened, 145);
    let abandonment = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let shared_revision = storage.revision(&store).unwrap();
    let abandoned = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(shared_revision, abandonment.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(
                &store,
                &abandonment,
                abandoned,
            )
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(_)
    ));
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(shared_revision, edit.prepared),
    ));
    assert!(matches!(
        storage.capture_draft_editor_candidate_publication_source(
            &store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                original_selector,
                DraftEditorCandidateActivationBindingV1::from_head(&opened),
                DraftPieceOperationIdV1::from_bytes([146; 16]),
                SyndicTimestamp::from_unix_millis(3),
            ),
        ),
        Err(DraftEditorCandidatePublicationCommandErrorV1::Invariant)
    ));
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, opened.draft_id(), opened.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
    ));
    assert_eq!(
        selector(&current(storage, &store, thread)),
        original_selector
    );
}

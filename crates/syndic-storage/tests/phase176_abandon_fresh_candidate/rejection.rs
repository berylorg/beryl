use crate::{shared::*, support::*};

use syndic_storage::{
    DraftEditorCandidateSessionAbandonFreshOutcomeV1, DraftEditorCandidateSessionDisposeRequestV1,
    DraftRootHistoryPairV1, test_faults::publish_draft_edit_history_pair,
};

#[test]
fn stale_dirty_custody_and_disposed_sessions_do_not_abandon() {
    let (_home, store, storage, thread) = fixture("abandon-rejections", 30, 65_536);
    let selected = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &selected, 32, 33);
    let other_thread = create_thread(&storage, &store, 60, 65_536);
    let other = current(&storage, &store, other_thread);
    for request in [
        DraftEditorCandidateSessionDisposeRequestV1::new(
            opened.draft_id(),
            opened.session_id(),
            DraftPieceOperationIdV1::from_bytes([34; 16]),
            opened.session_generation() + 1,
            DraftRootHistoryPairV1::new(opened.newest_root(), opened.newest_history()),
        ),
        DraftEditorCandidateSessionDisposeRequestV1::new(
            opened.draft_id(),
            opened.session_id(),
            DraftPieceOperationIdV1::from_bytes([35; 16]),
            opened.session_generation(),
            DraftRootHistoryPairV1::new(opened.published_root(), opened.published_history()),
        ),
        DraftEditorCandidateSessionDisposeRequestV1::new(
            opened.draft_id(),
            opened.session_id(),
            DraftPieceOperationIdV1::from_bytes([39; 16]),
            opened.session_generation(),
            DraftRootHistoryPairV1::new(other.draft().piece_root(), other.draft().history()),
        ),
    ] {
        let prepared = storage
            .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
            .unwrap();
        let outcome = execute(
            &store,
            storage.abandon_fresh_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        assert!(matches!(
            storage
                .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome)
                .unwrap(),
            DraftEditorCandidateSessionAbandonFreshOutcomeV1::NotFresh(_)
        ));
        assert_eq!(head(&storage, &store, &opened), opened);
    }

    let custody = transaction(&storage, &store, &opened, 36, "x", point(1));
    committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), custody.prepared.clone()),
    ));
    let occupied = head(&storage, &store, &opened);
    let request = abandon_request(&occupied, 37);
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::NotFresh(_)
    ));
    assert_eq!(head(&storage, &store, &opened), occupied);

    let (_home, store, storage, thread) = fixture("abandon-dirty", 40, 65_536);
    let selected = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &selected, 42, 43);
    let transaction = transaction(&storage, &store, &opened, 44, "x", point(1));
    build(&storage, &store, &transaction);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(
            storage.revision(&store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
    let dirty = match settled(&storage, &store, &transaction).closure() {
        DraftPieceSettlementClosureV1::Committed(adoption) => adoption.adopted_session().clone(),
        other => panic!("edit did not commit: {other:?}"),
    };
    let request = abandon_request(&dirty, 38);
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::NotFresh(_)
    ));
    assert_eq!(head(&storage, &store, &opened), dirty);

    publish_candidate(&storage, &store, &selected, &dirty, 45);
    let published = head(&storage, &store, &opened);
    let request = abandon_request(&published, 46);
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::NotFresh(_)
    ));
    assert_eq!(head(&storage, &store, &opened), published);
}

#[test]
fn selector_drift_is_unrelated_and_abandonment_identity_is_exact() {
    let (_home, store, storage, thread) = fixture("abandon-identity", 50, 65_536);
    let selected = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &selected, 52, 53);
    let request = abandon_request(&opened, 54);
    let prepared = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, request)
        .unwrap();
    committed(execute(
        &store,
        publish_draft_edit_history_pair(
            &store,
            storage.clone(),
            selected.draft().clone(),
            selected.draft().piece_root(),
            selected.draft().history(),
        ),
    ));
    assert_ne!(
        selector(&current(&storage, &store, thread)),
        selector(&selected)
    );
    let selector_before_abandonment = selector(&current(&storage, &store, thread));
    let outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    let abandoned = match storage
        .reconcile_abandon_fresh_draft_editor_candidate_session(&store, &prepared, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::Abandoned(head) => head,
        other => panic!("selector drift blocked abandonment: {other:?}"),
    };
    assert_eq!(
        selector(&current(&storage, &store, thread)),
        selector_before_abandonment
    );

    let collision_request = DraftEditorCandidateSessionDisposeRequestV1::new(
        abandoned.draft_id(),
        abandoned.session_id(),
        request.operation_id(),
        abandoned.session_generation(),
        DraftRootHistoryPairV1::new(abandoned.newest_root(), abandoned.newest_history()),
    );
    let collision = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, collision_request)
        .unwrap();
    let collision_outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            collision.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(
                &store,
                &collision,
                collision_outcome,
            )
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::OccupiedIdentityCollision(_)
    ));

    let disposed_request = DraftEditorCandidateSessionDisposeRequestV1::new(
        abandoned.draft_id(),
        abandoned.session_id(),
        DraftPieceOperationIdV1::from_bytes([55; 16]),
        abandoned.session_generation(),
        DraftRootHistoryPairV1::new(abandoned.newest_root(), abandoned.newest_history()),
    );
    let disposed = storage
        .prepare_abandon_fresh_draft_editor_candidate_session(&store, disposed_request)
        .unwrap();
    let disposed_outcome = execute(
        &store,
        storage.abandon_fresh_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            disposed.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_abandon_fresh_draft_editor_candidate_session(
                &store,
                &disposed,
                disposed_outcome,
            )
            .unwrap(),
        DraftEditorCandidateSessionAbandonFreshOutcomeV1::AlreadyDisposed(_)
    ));
    assert_eq!(head(&storage, &store, &opened), abandoned);
}

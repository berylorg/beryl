use super::support::*;

#[test]
fn family_order_and_canonical_empty_creation_replay_reopen_are_exact() {
    let names = syndic_v7_family_names();
    assert_eq!(names.len(), 82);
    assert_eq!(names[0], "threads");
    assert_eq!(names[1], "image-label-authority-heads");
    assert_eq!(names[2], "draft-image-label-protection-heads");
    assert_eq!(names[12], "draft-marker-order-commitments");
    assert_eq!(names[13], "draft-marker-seals");
    assert_eq!(names[18], "draft-editor-candidate-sessions");
    assert_eq!(names[19], "draft-mutation-staging-heads");
    assert_eq!(names[20], "draft-mutation-staging-pages");
    assert_eq!(names[21], "draft-mutation-staging-progress");
    assert_eq!(names[22], "draft-edit-history-frontiers");
    assert_eq!(names[23], "draft-edit-history-transitions");
    assert_eq!(names[24], "draft-historical-root-adoptions");
    assert_eq!(names[25], "draft-composer-builds");
    assert_eq!(names[26], "draft-composer-materializations");

    let home = TestHome::new("canonical-empty");
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(&storage, &store, 1, 4_096);
    let before = current(&storage, &store, thread);
    assert_eq!(before.draft().history().candidate_generation(), 0);
    assert_eq!(before.draft().history().frontier_revision(), 0);
    assert_eq!(before.draft().history().root(), before.draft().piece_root());
    assert!(!before.draft().history().availability().undo_available());
    assert!(!before.draft().history().availability().redo_available());

    not_committed(execute(
        &store,
        storage.create_thread(storage.revision(&store).unwrap(), create_request(1, 4_096)),
    ));
    assert_eq!(current(&storage, &store, thread), before);
    drop(store);

    let mut reopened = open(&home);
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(current(&reopened_storage, &reopened, thread), before);

    let collision_home = TestHome::new("canonical-empty-collision");
    let mut collision_store = open(&collision_home);
    let collision_storage = SyndicStorage::register(&mut collision_store).unwrap();
    let collision_request = create_request(2, 4_096);
    committed(execute(
        &collision_store,
        occupy_canonical_empty_draft_edit_history(
            &collision_store,
            collision_storage.clone(),
            collision_request.draft_id(),
            DraftEditHistoryPolicyV1::new(4_097, 2).unwrap(),
        ),
    ));
    not_committed(execute(
        &collision_store,
        collision_storage.create_thread(
            collision_storage.revision(&collision_store).unwrap(),
            collision_request.clone(),
        ),
    ));
    assert!(
        collision_storage
            .current_draft(
                &collision_store,
                collision_request.thread_id(),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn session_forks_the_published_pair_and_missing_frontier_fails_closed() {
    let (_home, store, storage, thread) = fixture("session-fork", 10, 4_096);
    let durable = current(&storage, &store, thread);
    let request = open_request(&durable, 12, 13);
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    let session = match storage
        .reconcile_draft_editor_candidate_session_open(&store, &prepared, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
        other => panic!("fresh open did not win: {other:?}"),
    };
    assert_eq!(session.durable_base_root(), durable.draft().piece_root());
    assert_eq!(session.durable_base_history(), durable.draft().history());
    assert_eq!(session.published_root(), durable.draft().piece_root());
    assert_eq!(session.published_history(), durable.draft().history());
    assert_eq!(session.newest_root(), durable.draft().piece_root());
    assert_eq!(session.newest_history().root(), session.newest_root());
    assert_eq!(session.newest_history().candidate_generation(), 0);
    assert_eq!(
        session.newest_history().key().session_id(),
        Some(session.session_id())
    );

    let replay = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_open(&store, &prepared, replay)
            .unwrap(),
        DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) if head == session
    ));

    committed(execute(
        &store,
        delete_draft_edit_history_frontier(&store, storage.clone(), session.newest_history().key()),
    ));
    let missing_replay = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(
        storage
            .reconcile_draft_editor_candidate_session_open(&store, &prepared, missing_replay,)
            .is_err()
    );
}

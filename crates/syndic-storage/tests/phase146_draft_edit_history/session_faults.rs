use super::support::*;

#[test]
fn session_replay_rejects_replaced_wrong_root_and_corrupt_frontiers() {
    for (name, seed, wrong_root) in [
        ("session-replaced", 40, false),
        ("session-wrong-root", 50, true),
    ] {
        let (_home, store, storage, thread) = fixture(name, seed, 4_096);
        let durable = current(storage, &store, thread);
        let request = open_request(&durable, seed.wrapping_add(2), seed.wrapping_add(3));
        let prepared = storage
            .prepare_open_draft_editor_candidate_session(&store, request)
            .unwrap();
        let session = match storage
            .reconcile_draft_editor_candidate_session_open(
                &store,
                &prepared,
                execute(
                    &store,
                    storage.open_draft_editor_candidate_session(
                        storage.revision(&store).unwrap(),
                        prepared.clone(),
                    ),
                ),
            )
            .unwrap()
        {
            DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
            other => panic!("fresh open did not win: {other:?}"),
        };
        let replacement = if wrong_root {
            let foreign_root = canonical_empty_draft_piece_root_v1(
                SyndicDraftId::from_bytes([seed.wrapping_add(9); 16]),
                durable.draft().revision(),
                DraftPieceOperationIdV1::from_bytes([seed.wrapping_add(10); 16]),
            );
            canonical_empty_draft_edit_history_v1(
                foreign_root.reference(),
                DraftEditHistoryPolicyV1::new(4_096, 1).unwrap(),
            )
            .fork_session(session.session_id())
            .unwrap()
        } else {
            canonical_empty_draft_edit_history_v1(
                session.newest_root(),
                DraftEditHistoryPolicyV1::new(4_097, 2).unwrap(),
            )
            .fork_session(session.session_id())
            .unwrap()
        };
        committed(execute(
            &store,
            replace_draft_edit_history_frontier(
                &store,
                storage,
                session.newest_history().key(),
                replacement,
            ),
        ));
        let replay = execute(
            &store,
            storage.open_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        assert!(
            storage
                .reconcile_draft_editor_candidate_session_open(&store, &prepared, replay)
                .is_err()
        );
    }

    let (_home, store, storage, thread) = fixture("session-corrupt", 60, 4_096);
    let durable = current(storage, &store, thread);
    let request = open_request(&durable, 62, 63);
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(&store, request)
        .unwrap();
    let session = match storage
        .reconcile_draft_editor_candidate_session_open(
            &store,
            &prepared,
            execute(
                &store,
                storage.open_draft_editor_candidate_session(
                    storage.revision(&store).unwrap(),
                    prepared.clone(),
                ),
            ),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
        other => panic!("fresh open did not win: {other:?}"),
    };
    inject_draft_edit_history_frontier_digest_corruption(
        &store,
        storage,
        session.newest_history().key(),
    )
    .unwrap();
    let replay = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(
        storage
            .reconcile_draft_editor_candidate_session_open(&store, &prepared, replay)
            .is_err()
    );
}

#[test]
fn candidate_session_point_reads_authenticate_history_after_restart() {
    for target in 0_u8..4 {
        let seed = 80_u8.wrapping_add(target * 10);
        let (home, store, storage, thread) = fixture("session-point-history", seed, 4_096);
        let durable = current(storage, &store, thread);
        let opened = open_session(
            storage,
            &store,
            &durable,
            seed.wrapping_add(2),
            seed.wrapping_add(3),
        );
        let (expected, settlement) = if target == 0 {
            (opened.clone(), None)
        } else {
            let edit = transaction(
                storage,
                &store,
                &opened,
                seed.wrapping_add(4),
                "history",
                point(7),
            );
            build(storage, &store, &edit);
            committed(execute(
                &store,
                storage.settle_draft_piece_edit(
                    storage.revision(&store).unwrap(),
                    edit.prepared.clone(),
                ),
            ));
            let settlement = settled(storage, &store, &edit);
            let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
                panic!("history fixture did not commit")
            };
            (adoption.adopted_session().clone(), Some(settlement))
        };
        drop(store);

        let mut reopened =
            HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
        let storage = SyndicStorage::register(&mut reopened).unwrap();
        assert!(matches!(
            storage
                .draft_editor_candidate_session(
                    &reopened,
                    expected.draft_id(),
                    expected.session_id(),
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Active(head) if head == expected
        ));

        let corruption = match target {
            0 | 1 => delete_draft_edit_history_frontier(
                &reopened,
                storage,
                expected.newest_history().key(),
            ),
            2 => {
                let settlement = settlement.as_ref().unwrap();
                let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure()
                else {
                    unreachable!()
                };
                delete_draft_edit_history_record(
                    &reopened,
                    storage,
                    DraftEditHistoryRecordDeletion::Transition(adoption.transition().key()),
                )
            }
            3 => {
                let settlement = settlement.as_ref().unwrap();
                let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure()
                else {
                    unreachable!()
                };
                let (replacement, _) = alternative_ordinary_draft_edit_history(
                    adoption.predecessor_history(),
                    adoption.adopted_session().newest_candidate_generation(),
                    adoption.adopted_root().reference(),
                    adoption.transition().before_caret(),
                    adoption.transition().before_selection(),
                    point(6),
                    adoption.transition().after_selection(),
                    adoption.transition().operation_id(),
                );
                replace_draft_edit_history_transition(
                    &reopened,
                    storage,
                    adoption.transition().key(),
                    replacement,
                )
            }
            _ => unreachable!(),
        };
        committed(execute(&reopened, corruption));
        assert!(matches!(
            storage
                .draft_editor_candidate_session(
                    &reopened,
                    expected.draft_id(),
                    expected.session_id(),
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
        ));
        drop(reopened);

        let mut restarted =
            HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
        let storage = SyndicStorage::register(&mut restarted).unwrap();
        assert!(matches!(
            storage
                .draft_editor_candidate_session(
                    &restarted,
                    expected.draft_id(),
                    expected.session_id(),
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
        ));
    }
}

#[test]
fn candidate_ranges_fail_closed_on_wrong_missing_and_replaced_history() {
    let (_home, store, storage, thread) = fixture("range-wrong-history", 120, 4_096);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 122, 123);
    let edit = transaction(storage, &store, &session, 124, "range", point(5));
    build(storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let settlement = settled(storage, &store, &edit);
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        panic!("range fixture did not commit")
    };
    let head = adoption.adopted_session();
    let wrong = DraftEditorCandidateActivationBindingV1::new(
        head.draft_id(),
        head.session_id(),
        head.session_generation(),
        head.newest_candidate_generation(),
        head.newest_root(),
        session.newest_history(),
        head.logical_extent(),
    );
    assert!(matches!(
        storage.candidate_draft_piece_text_demand(
            &store,
            wrong,
            DraftPieceTextDemandV1::Forward(0),
            64,
        ),
        Err(DraftPieceRangeSourceErrorV1::StaleCandidate)
    ));

    committed(execute(
        &store,
        delete_draft_edit_history_frontier(&store, storage, head.newest_history().key()),
    ));
    assert!(matches!(
        storage.candidate_draft_piece_marker_demand(
            &store,
            DraftEditorCandidateActivationBindingV1::from_head(head),
            DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(0),
                DraftPieceMarkerDirectionV1::Forward,
                None,
                1,
                65_536,
            ),
        ),
        Err(DraftPieceRangeSourceErrorV1::Invariant)
    ));

    let (_home, store, storage, thread) = fixture("range-replaced-history", 130, 4_096);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 132, 133);
    let edit = transaction(storage, &store, &session, 134, "range", point(5));
    build(storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let settlement = settled(storage, &store, &edit);
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        panic!("range replacement fixture did not commit")
    };
    let (replacement, _) = alternative_ordinary_draft_edit_history(
        adoption.predecessor_history(),
        adoption.adopted_session().newest_candidate_generation(),
        adoption.adopted_root().reference(),
        adoption.transition().before_caret(),
        adoption.transition().before_selection(),
        point(4),
        adoption.transition().after_selection(),
        adoption.transition().operation_id(),
    );
    committed(execute(
        &store,
        replace_draft_edit_history_transition(
            &store,
            storage,
            adoption.transition().key(),
            replacement,
        ),
    ));
    assert!(matches!(
        storage.candidate_draft_piece_marker_edge_proof(
            &store,
            DraftEditorCandidateActivationBindingV1::from_head(adoption.adopted_session()),
            DraftPieceMarkerEdgeProofRequestV1::Absence { anchor: 0 },
            64,
        ),
        Err(DraftPieceRangeSourceErrorV1::Invariant)
    ));
}

#[test]
fn session_identity_collision_is_not_exact_replay() {
    let (_home, store, storage, thread) = fixture("session-collision", 70, 4_096);
    let durable = current(storage, &store, thread);
    let request = open_request(&durable, 72, 73);
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(&store, request)
        .unwrap();
    let _session = match storage
        .reconcile_draft_editor_candidate_session_open(
            &store,
            &prepared,
            execute(
                &store,
                storage.open_draft_editor_candidate_session(
                    storage.revision(&store).unwrap(),
                    prepared.clone(),
                ),
            ),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) => head,
        other => panic!("fresh open did not win: {other:?}"),
    };
    let conflicting_selector = DraftEditorCurrentSelectorV1::new(
        durable.thread().id(),
        ThreadRevision::new(2).unwrap(),
        durable.draft().id(),
        durable.draft().revision(),
        durable.draft().piece_root(),
        durable.draft().history(),
    );
    let conflicting_request = DraftEditorCandidateSessionOpenRequestV1::new(
        conflicting_selector,
        request.session_id(),
        request.operation_id(),
    );
    let conflicting = storage
        .prepare_open_draft_editor_candidate_session(&store, conflicting_request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            conflicting.clone(),
        ),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_open(&store, &conflicting, outcome)
            .unwrap(),
        DraftEditorCandidateSessionOpenOutcomeV1::OccupiedIdentityCollision(_)
    ));
}

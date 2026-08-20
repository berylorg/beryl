use super::support::*;

#[test]
fn ordinary_commit_appends_one_exact_transition_and_frontier() {
    let (_home, store, storage, thread) = fixture("ordinary", 20, 4_096);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 22, 23);
    let edit = transaction(storage, &store, &session, 24, "history", point(7));
    build(storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let settlement = settled(storage, &store, &edit);
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        panic!("ordinary edit was not committed");
    };
    let DraftPieceSettlementOutcomeV1::Committed { history, .. } = settlement.outcome() else {
        panic!("ordinary edit has non-committed outcome");
    };
    assert_eq!(
        adoption.predecessor_history().reference(),
        session.newest_history()
    );
    assert_eq!(
        adoption.transition().predecessor_root(),
        session.newest_root()
    );
    assert_eq!(
        adoption.transition().successor_root(),
        adoption.adopted_root().reference()
    );
    assert_eq!(adoption.transition().before_caret(), point(0));
    assert_eq!(adoption.transition().before_selection(), point(0));
    assert_eq!(adoption.transition().after_caret(), point(7));
    assert_eq!(adoption.transition().after_selection(), point(7));
    assert_eq!(adoption.adopted_history().reference(), *history);
    assert_eq!(
        adoption.adopted_history().reference().frontier_revision(),
        1
    );
    assert_eq!(adoption.adopted_session().newest_history(), *history);
    assert_eq!(
        adoption.adopted_history().journal_head(),
        Some(adoption.transition().reference())
    );
    assert_eq!(
        adoption.adopted_history().undo_head(),
        Some(adoption.transition().reference())
    );
    assert_eq!(adoption.adopted_history().redo_head(), None);

    let replay = execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    );
    replay_succeeded(replay);
    assert_eq!(settled(storage, &store, &edit), settlement);
}

#[test]
fn predecessor_caret_and_directed_selection_are_authenticated() {
    let (_home, store, storage, thread) = fixture("predecessor-positions", 140, 8_192);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 142, 143);
    let seed = transaction(storage, &store, &session, 144, "history", point(7));
    build(storage, &store, &seed);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), seed.prepared.clone()),
    ));
    let seed_settlement = settled(storage, &store, &seed);
    let DraftPieceSettlementClosureV1::Committed(seed_adoption) = seed_settlement.closure() else {
        panic!("position seed did not commit")
    };

    let directed = transaction_with_positions(
        storage,
        &store,
        seed_adoption.adopted_session(),
        145,
        "next",
        point(7),
        point(0),
        point(4),
        point(0),
    );
    build(storage, &store, &directed);
    committed(execute(
        &store,
        storage
            .settle_draft_piece_edit(storage.revision(&store).unwrap(), directed.prepared.clone()),
    ));
    let directed_settlement = settled(storage, &store, &directed);
    let DraftPieceSettlementClosureV1::Committed(adoption) = directed_settlement.closure() else {
        panic!("directed position edit did not commit")
    };
    assert_eq!(adoption.transition().before_caret(), point(7));
    assert_eq!(adoption.transition().before_selection(), point(0));

    let before_revision = storage.revision(&store).unwrap();
    let before_history = adoption.adopted_session().newest_history();
    let replacements = vec![DraftPieceReplacementV1::new(
        point(0),
        point(0),
        vec![DraftPieceV1::Text("x".to_owned())],
    )];
    for (operation, predecessor_caret, predecessor_selection, expected) in [
        (
            146,
            point(12),
            point(0),
            DraftPieceRejectedReasonV1::InvalidUtf8Boundary,
        ),
        (
            147,
            point(0),
            DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
            DraftPieceRejectedReasonV1::InvalidGapWitness,
        ),
    ] {
        let header = DraftPieceEditHeaderV1::new(
            adoption.adopted_session().draft_id(),
            adoption.adopted_session().session_id(),
            adoption.adopted_session().newest_candidate_generation(),
            adoption.adopted_session().newest_root(),
            adoption.adopted_session().newest_history(),
            DraftPieceOperationIdV1::from_bytes([operation; 16]),
            predecessor_caret,
            predecessor_selection,
            point(0),
            point(0),
            1,
            canonical_draft_piece_fragment_chain_v1(&replacements),
        );
        assert!(matches!(
            storage.prepare_draft_piece_edit(&store, header, adoption.adopted_session()),
            Err(DraftPiecePrepareErrorV1::Rejected(reason)) if reason == expected
        ));
        assert_eq!(storage.revision(&store).unwrap(), before_revision);
        let DraftEditorCandidateSessionReadOutcomeV1::Active(current) = storage
            .draft_editor_candidate_session(
                &store,
                adoption.adopted_session().draft_id(),
                adoption.adopted_session().session_id(),
            )
            .unwrap()
        else {
            panic!("invalid position changed the candidate session")
        };
        assert_eq!(current.newest_history(), before_history);
    }

    let foreign_root = canonical_empty_draft_piece_root_v1(
        SyndicDraftId::from_bytes([149; 16]),
        durable.draft().revision(),
        DraftPieceOperationIdV1::from_bytes([150; 16]),
    );
    let wrong_root_header = DraftPieceEditHeaderV1::new(
        adoption.adopted_session().draft_id(),
        adoption.adopted_session().session_id(),
        adoption.adopted_session().newest_candidate_generation(),
        foreign_root.reference(),
        adoption.adopted_session().newest_history(),
        DraftPieceOperationIdV1::from_bytes([151; 16]),
        point(0),
        point(0),
        point(0),
        point(0),
        1,
        canonical_draft_piece_fragment_chain_v1(&replacements),
    );
    assert!(matches!(
        storage.prepare_draft_piece_edit(&store, wrong_root_header, adoption.adopted_session(),),
        Err(DraftPiecePrepareErrorV1::InvalidRoot)
    ));
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
}

#[test]
fn stale_complete_pair_is_rejected_without_changing_current_history() {
    let (_home, store, storage, thread) = fixture("complete-pair-conflict", 160, 4_096);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 162, 163);
    let winner = transaction(storage, &store, &session, 164, "winner", point(6));
    let stale = transaction(storage, &store, &session, 165, "stale", point(5));
    build(storage, &store, &winner);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), winner.prepared.clone()),
    ));
    let winner_settlement = settled(storage, &store, &winner);
    let DraftPieceSettlementClosureV1::Committed(adoption) = winner_settlement.closure() else {
        panic!("winner did not commit")
    };
    not_committed(execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), stale.prepared.clone()),
    ));
    let DraftEditorCandidateSessionReadOutcomeV1::Active(after) = storage
        .draft_editor_candidate_session(&store, session.draft_id(), session.session_id())
        .unwrap()
    else {
        panic!("candidate session disappeared after complete-pair conflict")
    };
    assert_eq!(
        after.newest_root(),
        adoption.adopted_session().newest_root()
    );
    assert_eq!(
        after.newest_history(),
        adoption.adopted_session().newest_history()
    );
}

use syndic_storage::{
    DraftEditHistoryTransitionV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftHistoricalRootAdoptionErrorReasonV1, DraftHistoricalRootAdoptionOutcomeV1,
    DraftHistoricalRootAdoptionReconciliationV1, DraftHistoricalRootAdoptionRequestV1,
    DraftHistoricalRootAdoptionStatusV1, DraftHistoricalRootDirectionV1,
    DraftPieceSettlementClosureV1,
};

use super::support::*;

fn committed_adoption(
    storage: syndic_storage::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    direction: DraftHistoricalRootDirectionV1,
    operation: u8,
) -> (
    syndic_storage::DraftHistoricalRootAdoptionProofV1,
    syndic_storage::PreparedDraftHistoricalRootAdoptionV1,
) {
    let prepared = prepare_historical_selection(storage, store, session, operation, direction);
    let outcome = execute(
        store,
        storage.adopt_draft_historical_root(revision(storage, store), prepared.clone()),
    );
    assert!(matches!(
        &outcome,
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    match storage
        .reconcile_draft_historical_root_adoption(store, &prepared, outcome)
        .unwrap()
    {
        DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
            DraftHistoricalRootAdoptionOutcomeV1::Committed(proof),
        ) => (proof, prepared),
        value => panic!("historical adoption did not commit: {value:?}"),
    }
}

fn undo_request(
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    transition: &DraftEditHistoryTransitionV1,
    operation: u8,
) -> DraftHistoricalRootAdoptionRequestV1 {
    DraftHistoricalRootAdoptionRequestV1::new(
        session.draft_id(),
        session.session_id(),
        operation_id(operation),
        session.newest_history(),
        transition.reference(),
        DraftHistoricalRootDirectionV1::Undo,
        transition.predecessor_root(),
        transition.before_caret(),
        transition.before_selection(),
    )
}

fn committed_edit(
    storage: syndic_storage::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    operation: u8,
    text: &str,
    before: u64,
    after: u64,
) -> syndic_storage::DraftPieceCommittedAdoptionV1 {
    let edit = transaction(
        storage,
        store,
        session,
        operation,
        text,
        point(before),
        point(after),
    );
    let settlement = settle(storage, store, &edit);
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        panic!("edit did not commit")
    };
    adoption.clone()
}

#[test]
fn undo_redo_and_branch_adopt_existing_roots_without_current_publication() {
    let (_home, store, storage, thread) = fixture("undo-redo-branch", 162);
    let durable_before = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable_before, 3, 4);
    let first = transaction(storage, &store, &session, 5, "one", point(0), point(3));
    let first_settlement = settle(storage, &store, &first);
    let DraftPieceSettlementClosureV1::Committed(first_adoption) = first_settlement.closure()
    else {
        panic!("first edit did not commit")
    };
    let second = transaction(
        storage,
        &store,
        first_adoption.adopted_session(),
        6,
        "two",
        point(3),
        point(6),
    );
    let second_settlement = settle(storage, &store, &second);
    let DraftPieceSettlementClosureV1::Committed(second_adoption) = second_settlement.closure()
    else {
        panic!("second edit did not commit")
    };

    for (operation, contribution) in [(17, 0_u8), (18, 1_u8), (19, 2_u8)] {
        let prepared = prepare_historical_selection(
            storage,
            &store,
            second_adoption.adopted_session(),
            operation,
            DraftHistoricalRootDirectionV1::Undo,
        );
        let outcome = match contribution {
            0 => execute(
                &store,
                storage.reject_draft_historical_root_adoption(
                    revision(storage, &store),
                    prepared.clone(),
                ),
            ),
            1 => execute(
                &store,
                storage.cancel_draft_historical_root_adoption(
                    revision(storage, &store),
                    prepared.clone(),
                ),
            ),
            _ => execute(
                &store,
                storage.error_draft_historical_root_adoption(
                    revision(storage, &store),
                    prepared.clone(),
                    DraftHistoricalRootAdoptionErrorReasonV1::InvalidAuthority,
                ),
            ),
        };
        assert!(matches!(
            &outcome,
            beryl_home_store::CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        assert!(matches!(
            storage
                .reconcile_draft_historical_root_adoption(&store, &prepared, outcome)
                .unwrap(),
            DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
                DraftHistoricalRootAdoptionOutcomeV1::Rejected(_)
                    | DraftHistoricalRootAdoptionOutcomeV1::Cancelled(_)
                    | DraftHistoricalRootAdoptionOutcomeV1::Error(_)
            )
        ));
    }

    let (undo, _) = committed_adoption(
        storage,
        &store,
        second_adoption.adopted_session(),
        DraftHistoricalRootDirectionV1::Undo,
        7,
    );
    let undo_settlement = undo.settlement();
    #[cfg(feature = "test-faults")]
    assert_eq!(
        syndic_storage::test_faults::roundtrip_draft_historical_root_adoption(undo_settlement),
        Some(undo_settlement.clone())
    );
    let undo_session = undo_settlement.successor_candidate().unwrap();
    let undo_history = undo_settlement.successor_history().unwrap();
    assert_eq!(
        undo_session.newest_root(),
        first_adoption.adopted_root().reference()
    );
    assert!(undo_history.undo_head().is_some());
    assert!(undo_history.redo_head().is_some());

    let (redo, _) = committed_adoption(
        storage,
        &store,
        undo_session,
        DraftHistoricalRootDirectionV1::Redo,
        8,
    );
    let redo_settlement = redo.settlement();
    let redo_session = redo_settlement.successor_candidate().unwrap();
    assert_eq!(
        redo_session.newest_root(),
        second_adoption.adopted_root().reference()
    );
    assert_eq!(
        redo_session.newest_history(),
        redo_settlement.successor_history().unwrap().reference()
    );

    let (second_undo, _) = committed_adoption(
        storage,
        &store,
        redo_session,
        DraftHistoricalRootDirectionV1::Undo,
        9,
    );
    let branch_session = second_undo.settlement().successor_candidate().unwrap();
    let branch = transaction(
        storage,
        &store,
        branch_session,
        10,
        "branch",
        point(3),
        point(9),
    );
    let branch_settlement = settle(storage, &store, &branch);
    let DraftPieceSettlementClosureV1::Committed(branch_adoption) = branch_settlement.closure()
    else {
        panic!("branch did not commit")
    };
    assert!(branch_adoption.adopted_history().redo_head().is_none());
    assert_eq!(branch_adoption.transition().before_caret(), point(3));
    assert_eq!(branch_adoption.transition().after_caret(), point(9));

    let durable_after = current(storage, &store, thread);
    assert_eq!(
        durable_after.draft().piece_root(),
        durable_before.draft().piece_root()
    );
    assert_eq!(
        durable_after.draft().history(),
        durable_before.draft().history()
    );
}

#[test]
fn every_successful_undo_redo_and_branch_is_exact_after_reopen() {
    let (home, mut store, mut storage, thread) = fixture("successful-reopen", 165);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 40, 41);
    let first = committed_edit(storage, &store, &session, 42, "one", 0, 3);
    let second = committed_edit(storage, &store, first.adopted_session(), 43, "two", 3, 6);
    (store, storage) = reopen(&home, store);
    let second_session = match storage
        .draft_editor_candidate_session(
            &store,
            second.adopted_session().draft_id(),
            second.adopted_session().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        value => panic!("reopened session is not active: {value:?}"),
    };

    let (undo, undo_prepared) = committed_adoption(
        storage,
        &store,
        &second_session,
        DraftHistoricalRootDirectionV1::Undo,
        44,
    );
    (store, storage) = reopen(&home, store);
    let undo_replay = execute(
        &store,
        storage.adopt_draft_historical_root(revision(storage, &store), undo_prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_historical_root_adoption(&store, &undo_prepared, undo_replay)
            .unwrap(),
        DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
            DraftHistoricalRootAdoptionOutcomeV1::Committed(_)
        )
    ));

    let undo_settlement = undo.settlement();
    let (redo, redo_prepared) = committed_adoption(
        storage,
        &store,
        undo_settlement.successor_candidate().unwrap(),
        DraftHistoricalRootDirectionV1::Redo,
        45,
    );
    (store, storage) = reopen(&home, store);
    let redo_replay = execute(
        &store,
        storage.adopt_draft_historical_root(revision(storage, &store), redo_prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_historical_root_adoption(&store, &redo_prepared, redo_replay)
            .unwrap(),
        DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
            DraftHistoricalRootAdoptionOutcomeV1::Committed(_)
        )
    ));

    let redo_settlement = redo.settlement();
    let (second_undo, second_undo_prepared) = committed_adoption(
        storage,
        &store,
        redo_settlement.successor_candidate().unwrap(),
        DraftHistoricalRootDirectionV1::Undo,
        46,
    );
    (store, storage) = reopen(&home, store);
    let second_undo_replay = execute(
        &store,
        storage
            .adopt_draft_historical_root(revision(storage, &store), second_undo_prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_historical_root_adoption(
                &store,
                &second_undo_prepared,
                second_undo_replay,
            )
            .unwrap(),
        DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
            DraftHistoricalRootAdoptionOutcomeV1::Committed(_)
        )
    ));

    let branch = committed_edit(
        storage,
        &store,
        second_undo.settlement().successor_candidate().unwrap(),
        47,
        "branch",
        3,
        9,
    );
    assert!(branch.adopted_history().redo_head().is_none());
    (store, storage) = reopen(&home, store);
    assert_eq!(
        storage
            .draft_editor_candidate_session(
                &store,
                branch.adopted_session().draft_id(),
                branch.adopted_session().session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(branch.adopted_session().clone())
    );
    assert_eq!(current(storage, &store, thread), durable);
}

#[test]
fn stale_conflict_exact_replay_and_byte_disagreeing_identity_survive_reopen() {
    let (home, store, storage, thread) = fixture("identity-conflict", 163);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 20, 21);
    let first = committed_edit(storage, &store, &session, 22, "one", 0, 3);
    let second = committed_edit(storage, &store, first.adopted_session(), 23, "two", 3, 6);
    let stale_intent = historical_selection_intent(
        second.adopted_session(),
        24,
        DraftHistoricalRootDirectionV1::Undo,
    );
    let stale = prepare_historical_selection(
        storage,
        &store,
        second.adopted_session(),
        24,
        DraftHistoricalRootDirectionV1::Undo,
    );
    let stale_request = stale.request();
    let third = committed_edit(
        storage,
        &store,
        second.adopted_session(),
        25,
        "three",
        6,
        11,
    );
    assert!(
        storage
            .prepare_draft_historical_root_selection(&store, stale_intent)
            .is_err()
    );
    let current_binding =
        syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(third.adopted_session());
    let stale_frontier = syndic_storage::DraftEditorCandidateActivationBindingV1::new(
        current_binding.draft_id(),
        current_binding.session_id(),
        current_binding.session_generation(),
        current_binding.candidate_generation(),
        current_binding.root(),
        second.adopted_session().newest_history(),
        current_binding.logical_extent(),
    );
    assert!(
        storage
            .prepare_draft_historical_root_selection(
                &store,
                syndic_storage::DraftHistoricalRootSelectionIntentV1::new(
                    stale_frontier,
                    operation_id(26),
                    DraftHistoricalRootDirectionV1::Undo,
                ),
            )
            .is_err()
    );
    let disagreeing = prepare_historical_selection(
        storage,
        &store,
        third.adopted_session(),
        24,
        DraftHistoricalRootDirectionV1::Undo,
    );

    committed(execute(
        &store,
        storage.adopt_draft_historical_root(revision(storage, &store), stale.clone()),
    ));
    let (store, storage) = reopen(&home, store);
    assert!(matches!(
        storage
            .draft_historical_root_adoption_status(&store, stale_request)
            .unwrap(),
        DraftHistoricalRootAdoptionStatusV1::Settled(
            DraftHistoricalRootAdoptionOutcomeV1::Conflict(_)
        )
    ));

    let replay = execute(
        &store,
        storage.adopt_draft_historical_root(revision(storage, &store), stale.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_historical_root_adoption(&store, &stale, replay)
            .unwrap(),
        DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
            DraftHistoricalRootAdoptionOutcomeV1::Conflict(_)
        )
    ));
    let collision = execute(
        &store,
        storage.adopt_draft_historical_root(revision(storage, &store), disagreeing.clone()),
    );
    assert!(matches!(
        collision,
        beryl_home_store::CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        storage
            .reconcile_draft_historical_root_adoption(&store, &disagreeing, collision)
            .unwrap(),
        DraftHistoricalRootAdoptionReconciliationV1::Collision
    ));
    assert_eq!(
        storage
            .draft_editor_candidate_session(
                &store,
                third.adopted_session().draft_id(),
                third.adopted_session().session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(third.adopted_session().clone())
    );
    assert_eq!(current(storage, &store, thread), durable);
}

#[test]
fn settlement_only_outcomes_preserve_candidate_and_history_across_reopen() {
    let (home, mut store, mut storage, thread) = fixture("terminal-reopen", 164);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 30, 31);
    let first = committed_edit(storage, &store, &session, 32, "one", 0, 3);
    let source = first.adopted_session().clone();

    for (operation, terminal) in [(33, 0_u8), (34, 1_u8), (35, 2_u8)] {
        let prepared = prepare_historical_selection(
            storage,
            &store,
            &source,
            operation,
            DraftHistoricalRootDirectionV1::Undo,
        );
        let request = prepared.request();
        let outcome = match terminal {
            0 => execute(
                &store,
                storage.reject_draft_historical_root_adoption(
                    revision(storage, &store),
                    prepared.clone(),
                ),
            ),
            1 => execute(
                &store,
                storage.cancel_draft_historical_root_adoption(
                    revision(storage, &store),
                    prepared.clone(),
                ),
            ),
            _ => execute(
                &store,
                storage.error_draft_historical_root_adoption(
                    revision(storage, &store),
                    prepared.clone(),
                    DraftHistoricalRootAdoptionErrorReasonV1::InvalidAuthority,
                ),
            ),
        };
        committed(outcome);
        (store, storage) = reopen(&home, store);
        assert!(matches!(
            storage
                .draft_historical_root_adoption_status(&store, request)
                .unwrap(),
            DraftHistoricalRootAdoptionStatusV1::Settled(
                DraftHistoricalRootAdoptionOutcomeV1::Rejected(_)
                    | DraftHistoricalRootAdoptionOutcomeV1::Cancelled(_)
                    | DraftHistoricalRootAdoptionOutcomeV1::Error(_)
            )
        ));
        assert_eq!(
            storage
                .draft_editor_candidate_session(&store, source.draft_id(), source.session_id())
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Active(source.clone())
        );
        assert_eq!(current(storage, &store, thread), durable);
    }
}

#[test]
fn wrong_draft_session_transition_direction_target_and_detached_sibling_fail_closed() {
    let (_home, store, storage, thread) = fixture("wrong-routes", 166);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 50, 51);
    let first = committed_edit(storage, &store, &session, 52, "one", 0, 3);
    let second = committed_edit(storage, &store, first.adopted_session(), 53, "two", 3, 6);
    let valid = undo_request(second.adopted_session(), second.transition(), 54);
    assert!(
        storage
            .prepare_draft_historical_root_adoption(&store, valid)
            .is_ok()
    );

    let wrong_draft = DraftHistoricalRootAdoptionRequestV1::new(
        beryl_model::SyndicDraftId::from_bytes([251; 16]),
        valid.key().session_id(),
        valid.key().operation_id(),
        valid.source_history(),
        valid.selected_transition(),
        valid.direction(),
        valid.target_root(),
        valid.caret(),
        valid.selection(),
    );
    let wrong_session = DraftHistoricalRootAdoptionRequestV1::new(
        valid.key().draft_id(),
        syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([252; 16]),
        valid.key().operation_id(),
        valid.source_history(),
        valid.selected_transition(),
        valid.direction(),
        valid.target_root(),
        valid.caret(),
        valid.selection(),
    );
    let wrong_transition = DraftHistoricalRootAdoptionRequestV1::new(
        valid.key().draft_id(),
        valid.key().session_id(),
        valid.key().operation_id(),
        valid.source_history(),
        first.transition().reference(),
        valid.direction(),
        first.transition().predecessor_root(),
        first.transition().before_caret(),
        first.transition().before_selection(),
    );
    let wrong_direction = DraftHistoricalRootAdoptionRequestV1::new(
        valid.key().draft_id(),
        valid.key().session_id(),
        valid.key().operation_id(),
        valid.source_history(),
        valid.selected_transition(),
        DraftHistoricalRootDirectionV1::Redo,
        valid.target_root(),
        valid.caret(),
        valid.selection(),
    );
    let wrong_target = DraftHistoricalRootAdoptionRequestV1::new(
        valid.key().draft_id(),
        valid.key().session_id(),
        valid.key().operation_id(),
        valid.source_history(),
        valid.selected_transition(),
        valid.direction(),
        second.transition().successor_root(),
        valid.caret(),
        valid.selection(),
    );
    let wrong_caret = DraftHistoricalRootAdoptionRequestV1::new(
        valid.key().draft_id(),
        valid.key().session_id(),
        valid.key().operation_id(),
        valid.source_history(),
        valid.selected_transition(),
        valid.direction(),
        valid.target_root(),
        point(1),
        valid.selection(),
    );
    let wrong_selection = DraftHistoricalRootAdoptionRequestV1::new(
        valid.key().draft_id(),
        valid.key().session_id(),
        valid.key().operation_id(),
        valid.source_history(),
        valid.selected_transition(),
        valid.direction(),
        valid.target_root(),
        valid.caret(),
        point(1),
    );
    for request in [
        wrong_draft,
        wrong_session,
        wrong_transition,
        wrong_direction,
        wrong_target,
        wrong_caret,
        wrong_selection,
    ] {
        assert!(
            storage
                .prepare_draft_historical_root_adoption(&store, request)
                .is_err()
        );
    }

    let sibling = open_session(storage, &store, &durable, 55, 56);
    let sibling_edit = committed_edit(storage, &store, &sibling, 57, "sibling", 0, 7);
    let detached = DraftHistoricalRootAdoptionRequestV1::new(
        valid.key().draft_id(),
        valid.key().session_id(),
        operation_id(58),
        valid.source_history(),
        sibling_edit.transition().reference(),
        DraftHistoricalRootDirectionV1::Undo,
        sibling_edit.transition().predecessor_root(),
        sibling_edit.transition().before_caret(),
        sibling_edit.transition().before_selection(),
    );
    assert!(
        storage
            .prepare_draft_historical_root_adoption(&store, detached)
            .is_err()
    );
    assert_eq!(
        storage
            .draft_editor_candidate_session(
                &store,
                second.adopted_session().draft_id(),
                second.adopted_session().session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(second.adopted_session().clone())
    );
    assert_eq!(current(storage, &store, thread), durable);
}

#[test]
fn sixty_four_slot_witness_remains_bounded_and_authenticates_deep_undo() {
    let (home, store, storage, thread) = fixture("bounded-witness", 167);
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 60, 61);
    let mut latest = None;
    for index in 0_u8..65 {
        let adoption = committed_edit(
            storage,
            &store,
            &session,
            62_u8.wrapping_add(index),
            "x",
            u64::from(index),
            u64::from(index) + 1,
        );
        session = adoption.adopted_session().clone();
        latest = Some(adoption);
    }
    let latest = latest.unwrap();
    assert_eq!(latest.transition().journal_depth(), 65);
    assert_eq!(latest.transition().ancestor_witness().slots().len(), 64);
    assert!(latest.transition().ancestor_witness().bitmap() != 0);
    for (level, slot) in latest
        .transition()
        .ancestor_witness()
        .slots()
        .iter()
        .enumerate()
    {
        assert_eq!(
            slot.is_some(),
            latest.transition().ancestor_witness().bitmap() & (1_u64 << level) != 0
        );
    }

    let (undo, _) = committed_adoption(
        storage,
        &store,
        latest.adopted_session(),
        DraftHistoricalRootDirectionV1::Undo,
        200,
    );
    assert_eq!(
        undo.settlement()
            .successor_transition()
            .unwrap()
            .journal_depth(),
        66
    );
    let (store, storage) = reopen(&home, store);
    assert_eq!(
        storage
            .draft_editor_candidate_session(
                &store,
                undo.settlement().successor_candidate().unwrap().draft_id(),
                undo.settlement()
                    .successor_candidate()
                    .unwrap()
                    .session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(
            undo.settlement().successor_candidate().unwrap().clone()
        )
    );
    assert_eq!(current(storage, &store, thread), durable);
}

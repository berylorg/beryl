use beryl_home_store::HomeHealthState;
use beryl_home_store::test_faults::FaultPoint;
use syndic_storage::test_faults::{
    DraftEditHistoryRecordDeletion, DraftPieceImmutableDeletion,
    alternative_ordinary_draft_edit_history, delete_draft_edit_history_record,
    delete_draft_piece_immutable_record, draft_edit_history_stored_charge_components,
    draft_piece_immutable_snapshot, inject_draft_edit_history_frontier_digest_corruption,
    replace_draft_edit_history_transition, syndic_v7_family_names,
};
use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftHistoricalRootAdoptionOutcomeV1,
    DraftHistoricalRootAdoptionReconciliationV1, DraftHistoricalRootDirectionV1,
    DraftPieceCommittedAdoptionV1, DraftPieceSettlementClosureV1, SyndicStorage,
};

use super::support::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HistoricalCommand {
    Adopt,
    Conflict,
    Reject,
    Cancel,
    Error(syndic_storage::DraftHistoricalRootAdoptionErrorReasonV1),
}

fn edit(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    operation: u8,
    text: &str,
    before: u64,
    after: u64,
) -> DraftPieceCommittedAdoptionV1 {
    let transaction = transaction(
        storage,
        store,
        session,
        operation,
        text,
        point(before),
        point(after),
    );
    let settlement = settle(storage, store, &transaction);
    let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
        panic!("edit did not commit")
    };
    adoption.clone()
}

fn undo_prepared(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    operation: u8,
) -> syndic_storage::PreparedDraftHistoricalRootAdoptionV1 {
    prepare_historical_selection(
        storage,
        store,
        session,
        operation,
        DraftHistoricalRootDirectionV1::Undo,
    )
}

fn historical(
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    direction: DraftHistoricalRootDirectionV1,
    operation: u8,
) -> Option<syndic_storage::DraftHistoricalRootAdoptionProofV1> {
    let prepared = match storage
        .prepare_draft_historical_root_selection(
            store,
            historical_selection_intent(session, operation, direction),
        )
        .ok()?
    {
        syndic_storage::DraftHistoricalRootSelectionV1::Prepared(prepared) => prepared,
        syndic_storage::DraftHistoricalRootSelectionV1::Unavailable => return None,
    };
    let outcome = execute(
        store,
        storage.adopt_draft_historical_root(revision(storage, store), prepared.clone()),
    );
    match storage
        .reconcile_draft_historical_root_adoption(store, &prepared, outcome)
        .ok()?
    {
        DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
            DraftHistoricalRootAdoptionOutcomeV1::Committed(proof),
        ) => Some(proof),
        _ => None,
    }
}

#[test]
fn every_four_record_commit_cut_recovers_only_exact_old_or_exact_new_after_reopen() {
    for (name, seed, cut, committed_at_cut) in [
        ("before", 170, FaultPoint::BeforeCommit, false),
        (
            "after-commit",
            180,
            FaultPoint::AfterCommitBeforePersist,
            true,
        ),
        ("after-persist", 190, FaultPoint::AfterPersist, true),
        (
            "before-verification",
            200,
            FaultPoint::BeforeVerification,
            true,
        ),
    ] {
        let (home, store, storage, faults, thread) = fault_fixture(name, seed);
        let durable = current(&storage, &store, thread);
        let session = open_session(
            &storage,
            &store,
            &durable,
            seed.wrapping_add(2),
            seed.wrapping_add(3),
        );
        let first = edit(
            &storage,
            &store,
            &session,
            seed.wrapping_add(4),
            "one",
            0,
            3,
        );
        let second = edit(
            &storage,
            &store,
            first.adopted_session(),
            seed.wrapping_add(5),
            "two",
            3,
            6,
        );
        let prepared = prepare_historical_selection(
            &storage,
            &store,
            second.adopted_session(),
            seed.wrapping_add(6),
            DraftHistoricalRootDirectionV1::Undo,
        );
        let old_session = second.adopted_session().clone();
        faults.fail_next(cut);
        let outcome = execute(
            &store,
            storage.adopt_draft_historical_root(revision(&storage, &store), prepared.clone()),
        );
        let (store, storage) = if store.health().state() == HomeHealthState::Failed {
            let recovery = store.recover_same_home().unwrap();
            let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            (recovery.publish(), storage)
        } else {
            (store, storage)
        };
        let reconciled = storage
            .reconcile_draft_historical_root_adoption(&store, &prepared, outcome)
            .unwrap();
        if committed_at_cut {
            assert!(matches!(
                reconciled,
                DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
                    DraftHistoricalRootAdoptionOutcomeV1::Committed(_)
                )
            ));
        } else {
            assert_eq!(
                reconciled,
                DraftHistoricalRootAdoptionReconciliationV1::ExactOld
            );
        }
        let (store, storage) = reopen(&home, store);
        if !committed_at_cut {
            assert_eq!(
                storage
                    .draft_editor_candidate_session(
                        &store,
                        old_session.draft_id(),
                        old_session.session_id(),
                    )
                    .unwrap(),
                DraftEditorCandidateSessionReadOutcomeV1::Active(old_session)
            );
        }
        assert_eq!(current(&storage, &store, thread), durable);
    }
}

#[test]
fn adoption_reuses_immutable_roots_and_preserves_canonical_records_and_current_publication() {
    let (home, store, storage, thread) = fixture("no-copy", 210);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 212, 213);
    let first = edit(&storage, &store, &session, 214, "one", 0, 3);
    let second = edit(&storage, &store, first.adopted_session(), 215, "two", 3, 6);
    let source = second.transition().successor_root();
    let target = second.transition().predecessor_root();
    let source_snapshot = draft_piece_immutable_snapshot(&store, storage.clone(), source).unwrap();
    let target_snapshot = draft_piece_immutable_snapshot(&store, storage.clone(), target).unwrap();
    let prepared = undo_prepared(&storage, &store, second.adopted_session(), 216);
    let outcome = execute(
        &store,
        storage.adopt_draft_historical_root(revision(&storage, &store), prepared.clone()),
    );
    let DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
        DraftHistoricalRootAdoptionOutcomeV1::Committed(proof),
    ) = storage
        .reconcile_draft_historical_root_adoption(&store, &prepared, outcome)
        .unwrap()
    else {
        panic!("adoption did not commit")
    };
    assert_eq!(proof.successor_candidate().unwrap().newest_root(), target);
    let (store, storage) = reopen(&home, store);
    assert_eq!(
        draft_piece_immutable_snapshot(&store, storage.clone(), source).unwrap(),
        source_snapshot
    );
    assert_eq!(
        draft_piece_immutable_snapshot(&store, storage.clone(), target).unwrap(),
        target_snapshot
    );
    assert_eq!(current(&storage, &store, thread), durable);
    let names = syndic_v7_family_names();
    assert_eq!(names.len(), 86);
    assert_eq!(names[0], "threads");
    assert_eq!(names[1], "image-label-authority-heads");
    assert_eq!(names[2], "draft-image-label-protection-heads");
    assert_eq!(names[24], "draft-historical-root-adoptions");
    assert_eq!(names[25], "draft-composer-builds");
    assert_eq!(names[26], "draft-composer-materializations");
}

#[test]
fn missing_or_corrupt_root_transition_and_frontier_fail_without_successor_publication() {
    for (name, seed, corruption) in [
        ("missing-root", 220, 0_u8),
        ("missing-transition", 225, 1_u8),
        ("missing-frontier", 230, 2_u8),
        ("corrupt-frontier", 235, 3_u8),
        ("replaced-transition", 240, 4_u8),
    ] {
        let (home, store, storage, thread) = fixture(name, seed);
        let durable = current(&storage, &store, thread);
        let session = open_session(
            &storage,
            &store,
            &durable,
            seed.wrapping_add(2),
            seed.wrapping_add(3),
        );
        let first = edit(
            &storage,
            &store,
            &session,
            seed.wrapping_add(4),
            "one",
            0,
            3,
        );
        let second = edit(
            &storage,
            &store,
            first.adopted_session(),
            seed.wrapping_add(5),
            "two",
            3,
            6,
        );
        let _prepared = prepare_historical_selection(
            &storage,
            &store,
            second.adopted_session(),
            seed.wrapping_add(6),
            DraftHistoricalRootDirectionV1::Undo,
        );
        let target_root = second.transition().predecessor_root();
        let selected_transition = second.transition().reference();
        let source_history = second.adopted_session().newest_history();
        match corruption {
            0 => committed(execute(
                &store,
                delete_draft_piece_immutable_record(
                    &store,
                    &storage,
                    target_root,
                    DraftPieceImmutableDeletion::Root,
                ),
            )),
            1 => committed(execute(
                &store,
                delete_draft_edit_history_record(
                    &store,
                    storage.clone(),
                    DraftEditHistoryRecordDeletion::Transition(selected_transition.key()),
                ),
            )),
            2 => committed(execute(
                &store,
                delete_draft_edit_history_record(
                    &store,
                    storage.clone(),
                    DraftEditHistoryRecordDeletion::Frontier(source_history.key()),
                ),
            )),
            3 => inject_draft_edit_history_frontier_digest_corruption(
                &store,
                storage.clone(),
                source_history.key(),
            )
            .unwrap(),
            _ => {
                let (replacement, _) = alternative_ordinary_draft_edit_history(
                    first.adopted_history(),
                    second.adopted_session().newest_candidate_generation(),
                    second.adopted_root().reference(),
                    second.transition().before_caret(),
                    second.transition().before_selection(),
                    second.transition().after_caret(),
                    second.transition().after_selection(),
                    operation_id(seed.wrapping_add(20)),
                );
                committed(execute(
                    &store,
                    replace_draft_edit_history_transition(
                        &store,
                        storage.clone(),
                        selected_transition.key(),
                        replacement,
                    ),
                ));
            }
        }
        assert!(
            storage
                .prepare_draft_historical_root_selection(
                    &store,
                    historical_selection_intent(
                        second.adopted_session(),
                        seed.wrapping_add(7),
                        DraftHistoricalRootDirectionV1::Undo,
                    ),
                )
                .is_err()
        );
        let (store, storage) = reopen(&home, store);
        assert!(
            storage
                .prepare_draft_historical_root_selection(
                    &store,
                    historical_selection_intent(
                        second.adopted_session(),
                        seed.wrapping_add(7),
                        DraftHistoricalRootDirectionV1::Undo,
                    ),
                )
                .is_err()
        );
        if store.health().state() == HomeHealthState::Failed {
            assert!(
                storage
                    .current_draft(
                        &store,
                        thread,
                        syndic_storage::SyndicPointReadLimit::new(65_536).unwrap(),
                    )
                    .is_err(),
                "corrupt historical authority must remain behind the failed read gate"
            );
        } else {
            assert_eq!(current(&storage, &store, thread), durable);
        }
    }
}

#[test]
fn every_historical_outcome_reconciles_with_its_public_terminal_family_after_reopen() {
    let expected = [
        HistoricalCommand::Adopt,
        HistoricalCommand::Reject,
        HistoricalCommand::Conflict,
        HistoricalCommand::Cancel,
        HistoricalCommand::Error(
            syndic_storage::DraftHistoricalRootAdoptionErrorReasonV1::InvalidAuthority,
        ),
    ];
    for (index, expected_outcome) in expected.into_iter().enumerate() {
        let seed = 240_u8.wrapping_add(index as u8 * 3);
        let (home, store, storage, thread) = fixture(&format!("outcome-{index}"), seed);
        let durable = current(&storage, &store, thread);
        let session = open_session(
            &storage,
            &store,
            &durable,
            seed.wrapping_add(1),
            seed.wrapping_add(2),
        );
        let first = edit(
            &storage,
            &store,
            &session,
            seed.wrapping_add(3),
            "one",
            0,
            3,
        );
        let prepared = undo_prepared(
            &storage,
            &store,
            first.adopted_session(),
            seed.wrapping_add(4),
        );
        if expected_outcome == HistoricalCommand::Conflict {
            let _ = edit(
                &storage,
                &store,
                first.adopted_session(),
                seed.wrapping_add(5),
                "two",
                3,
                6,
            );
        }
        let contribution = match expected_outcome {
            HistoricalCommand::Adopt | HistoricalCommand::Conflict => {
                storage.adopt_draft_historical_root(revision(&storage, &store), prepared.clone())
            }
            HistoricalCommand::Reject => storage.reject_draft_historical_root_adoption(
                revision(&storage, &store),
                prepared.clone(),
            ),
            HistoricalCommand::Cancel => storage.cancel_draft_historical_root_adoption(
                revision(&storage, &store),
                prepared.clone(),
            ),
            HistoricalCommand::Error(reason) => storage.error_draft_historical_root_adoption(
                revision(&storage, &store),
                prepared.clone(),
                reason,
            ),
        };
        committed(execute(&store, contribution));
        let (store, storage) = reopen(&home, store);
        let replay = match expected_outcome {
            HistoricalCommand::Adopt | HistoricalCommand::Conflict => {
                storage.adopt_draft_historical_root(revision(&storage, &store), prepared.clone())
            }
            HistoricalCommand::Reject => storage.reject_draft_historical_root_adoption(
                revision(&storage, &store),
                prepared.clone(),
            ),
            HistoricalCommand::Cancel => storage.cancel_draft_historical_root_adoption(
                revision(&storage, &store),
                prepared.clone(),
            ),
            HistoricalCommand::Error(reason) => storage.error_draft_historical_root_adoption(
                revision(&storage, &store),
                prepared.clone(),
                reason,
            ),
        };
        let DraftHistoricalRootAdoptionReconciliationV1::ExactNew(outcome) = storage
            .reconcile_draft_historical_root_adoption(&store, &prepared, execute(&store, replay))
            .unwrap()
        else {
            panic!("outcome {index} was not durably settled")
        };
        assert!(matches!(
            (expected_outcome, outcome),
            (
                HistoricalCommand::Adopt,
                DraftHistoricalRootAdoptionOutcomeV1::Committed(_)
            ) | (
                HistoricalCommand::Reject,
                DraftHistoricalRootAdoptionOutcomeV1::Rejected(_)
            ) | (
                HistoricalCommand::Conflict,
                DraftHistoricalRootAdoptionOutcomeV1::Conflict(_)
            ) | (
                HistoricalCommand::Cancel,
                DraftHistoricalRootAdoptionOutcomeV1::Cancelled(_)
            ) | (
                HistoricalCommand::Error(_),
                DraftHistoricalRootAdoptionOutcomeV1::Error(_)
            )
        ));
    }
}

#[test]
fn retention_floor_evicts_only_crossed_heads_and_keeps_the_adoption_head() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("floor-measure", 12);
    let durable = current(&measure_storage, &measure_store, measure_thread);
    let session = open_session(&measure_storage, &measure_store, &durable, 14, 15);
    let first = edit(&measure_storage, &measure_store, &session, 16, "a", 0, 1);
    let second = edit(
        &measure_storage,
        &measure_store,
        first.adopted_session(),
        17,
        "b",
        1,
        2,
    );
    let components =
        draft_edit_history_stored_charge_components(second.adopted_history(), second.transition())
            .unwrap();
    let budget = components[0] + components[2] + components[3] + components[5];

    let (home, store, storage, thread) =
        fixture_with_history_budget("floor-adoption", 22, budget * 2);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 24, 25);
    let first = edit(&storage, &store, &session, 26, "a", 0, 1);
    let second = edit(&storage, &store, first.adopted_session(), 27, "b", 1, 2);
    let third = edit(&storage, &store, second.adopted_session(), 28, "c", 2, 3);
    assert_ne!(
        third.adopted_history().oldest_eligible(),
        Some(first.transition().reference())
    );
    assert_eq!(
        third.adopted_history().undo_head(),
        Some(third.transition().reference())
    );
    let prepared = undo_prepared(&storage, &store, third.adopted_session(), 29);
    let outcome = execute(
        &store,
        storage.adopt_draft_historical_root(revision(&storage, &store), prepared.clone()),
    );
    let reconciled = storage
        .reconcile_draft_historical_root_adoption(&store, &prepared, outcome)
        .unwrap();
    let DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
        DraftHistoricalRootAdoptionOutcomeV1::Committed(proof),
    ) = reconciled
    else {
        panic!("bounded adoption did not commit: {reconciled:?}")
    };
    assert!(third.transition().prior_undo().is_some());
    let unavailable_source = proof.successor_candidate().unwrap().clone();
    assert!(matches!(
        storage.prepare_draft_historical_root_selection(
            &store,
            historical_selection_intent(
                &unavailable_source,
                33,
                DraftHistoricalRootDirectionV1::Undo,
            ),
        ),
        Ok(syndic_storage::DraftHistoricalRootSelectionV1::Unavailable)
    ));
    assert_eq!(
        storage
            .draft_editor_candidate_session(
                &store,
                unavailable_source.draft_id(),
                unavailable_source.session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(unavailable_source)
    );
    let (store, storage) = reopen(&home, store);
    assert!(matches!(
        storage
            .reconcile_draft_historical_root_adoption(
                &store,
                &prepared,
                execute(
                    &store,
                    storage
                        .adopt_draft_historical_root(revision(&storage, &store), prepared.clone(),),
                ),
            )
            .unwrap(),
        DraftHistoricalRootAdoptionReconciliationV1::ExactNew(
            DraftHistoricalRootAdoptionOutcomeV1::Committed(_)
        )
    ));
    let redo = historical(
        &storage,
        &store,
        proof.successor_candidate().unwrap(),
        DraftHistoricalRootDirectionV1::Redo,
        30,
    )
    .expect("floor-filtered undo successor accepts a subsequent redo");
    let (store, storage) = reopen(&home, store);
    let undo = historical(
        &storage,
        &store,
        redo.successor_candidate().unwrap(),
        DraftHistoricalRootDirectionV1::Undo,
        31,
    )
    .expect("reopened redo successor accepts a subsequent undo");
    let (store, storage) = reopen(&home, store);
    let branch = edit(
        &storage,
        &store,
        undo.successor_candidate().unwrap(),
        32,
        "branch",
        2,
        8,
    );
    assert!(branch.adopted_history().redo_head().is_none());
    assert!(
        branch.adopted_history().retained_encoded_bytes() <= branch.adopted_history().byte_budget()
    );
    let (store, storage) = reopen(&home, store);
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
    assert_eq!(current(&storage, &store, thread), durable);
}

#[test]
fn redo_prior_floor_filter_survives_reopen_historical_continuation_and_branch() {
    let (home, store, storage, thread) = fixture_with_history_budget("redo-prior-floor", 42, 4_300);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 44, 45);
    let first = edit(&storage, &store, &session, 46, "a", 0, 1);
    let second = edit(&storage, &store, first.adopted_session(), 47, "b", 1, 2);
    let third = edit(&storage, &store, second.adopted_session(), 48, "c", 2, 3);

    let undo_one = historical(
        &storage,
        &store,
        third.adopted_session(),
        DraftHistoricalRootDirectionV1::Undo,
        49,
    )
    .expect("first bounded undo commits");
    let (store, storage) = reopen(&home, store);

    let undo_two = historical(
        &storage,
        &store,
        undo_one.successor_candidate().unwrap(),
        DraftHistoricalRootDirectionV1::Undo,
        50,
    )
    .expect("second bounded undo commits after reopen");
    let (store, storage) = reopen(&home, store);

    let redo = historical(
        &storage,
        &store,
        undo_two.successor_candidate().unwrap(),
        DraftHistoricalRootDirectionV1::Redo,
        51,
    )
    .expect("redo with a floor-crossing prior redo link commits");
    let (store, storage) = reopen(&home, store);

    let undo = historical(
        &storage,
        &store,
        redo.successor_candidate().unwrap(),
        DraftHistoricalRootDirectionV1::Undo,
        52,
    )
    .expect("floor-filtered redo successor accepts a subsequent undo");
    let (store, storage) = reopen(&home, store);
    let branch = edit(
        &storage,
        &store,
        undo.successor_candidate().unwrap(),
        53,
        "branch",
        1,
        7,
    );
    assert!(branch.adopted_history().redo_head().is_none());
    assert!(
        branch.adopted_history().retained_encoded_bytes() <= branch.adopted_history().byte_budget()
    );
    let (store, storage) = reopen(&home, store);
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
    assert_eq!(current(&storage, &store, thread), durable);
}

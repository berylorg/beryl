use super::support::*;

#[test]
fn budget_exhaustion_is_typed_noncommit_and_preserves_history() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("budget-measure", 30, 4_096);
    let measure_durable = current(&measure_storage, &measure_store, measure_thread);
    let measure_session = open_session(&measure_storage, &measure_store, &measure_durable, 32, 33);
    let measure_edit = transaction(
        &measure_storage,
        &measure_store,
        &measure_session,
        34,
        "x",
        point(1),
    );
    build(&measure_storage, &measure_store, &measure_edit);
    committed(execute(
        &measure_store,
        measure_storage.settle_draft_piece_edit(
            measure_storage.revision(&measure_store).unwrap(),
            measure_edit.prepared.clone(),
        ),
    ));
    let measured_settlement = settled(&measure_storage, &measure_store, &measure_edit);
    let DraftPieceSettlementClosureV1::Committed(measured_adoption) = measured_settlement.closure()
    else {
        panic!("measurement edit was not committed");
    };
    let [
        frontier_outer_key,
        frontier_embedded_key,
        frontier_value,
        transition_outer_key,
        transition_embedded_key,
        transition_value,
    ] = draft_edit_history_stored_charge_components(
        measured_adoption.adopted_history(),
        measured_adoption.transition(),
    )
    .unwrap();
    assert_eq!(frontier_outer_key, frontier_embedded_key);
    assert_eq!(transition_outer_key, transition_embedded_key);
    assert_eq!(
        [
            frontier_outer_key,
            frontier_embedded_key,
            frontier_value,
            transition_outer_key,
            transition_embedded_key,
            transition_value,
        ],
        [33, 33, 687, 40, 40, 760],
    );
    let exact_budget = frontier_outer_key
        .checked_add(frontier_value)
        .and_then(|value| value.checked_add(transition_outer_key))
        .and_then(|value| value.checked_add(transition_value))
        .unwrap();
    assert_eq!(
        measured_adoption.adopted_history().retained_encoded_bytes(),
        exact_budget
    );
    assert_eq!(
        measured_adoption.transition().cumulative_encoded_bytes(),
        transition_outer_key + transition_value
    );

    let (_exact_home, exact_store, exact_storage, exact_thread) =
        fixture("budget-exact", 35, exact_budget);
    let exact_durable = current(&exact_storage, &exact_store, exact_thread);
    let exact_session = open_session(&exact_storage, &exact_store, &exact_durable, 37, 38);
    let exact_edit = transaction(
        &exact_storage,
        &exact_store,
        &exact_session,
        39,
        "x",
        point(1),
    );
    build(&exact_storage, &exact_store, &exact_edit);
    committed(execute(
        &exact_store,
        exact_storage.settle_draft_piece_edit(
            exact_storage.revision(&exact_store).unwrap(),
            exact_edit.prepared.clone(),
        ),
    ));
    let exact_settlement = settled(&exact_storage, &exact_store, &exact_edit);
    let DraftPieceSettlementClosureV1::Committed(exact_adoption) = exact_settlement.closure()
    else {
        panic!("exact-fit edit was not committed");
    };
    assert_eq!(
        exact_adoption.adopted_history().retained_encoded_bytes(),
        exact_budget
    );

    let (_home, store, storage, thread) = fixture("budget-under", 40, exact_budget - 1);
    let durable = current(&storage, &store, thread);
    let session = open_session(&storage, &store, &durable, 42, 43);
    let edit = transaction(&storage, &store, &session, 44, "x", point(1));
    build(&storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let settlement = settled(&storage, &store, &edit);
    assert!(matches!(
        settlement.outcome(),
        DraftPieceSettlementOutcomeV1::Error(DraftPieceErrorReasonV1::HistoryCapacityUnavailable)
    ));
    let DraftPieceSettlementClosureV1::Noncommit(noncommit) = settlement.closure() else {
        panic!("budget exhaustion wrote a commit closure");
    };
    assert_eq!(
        noncommit.observed_history().reference(),
        session.newest_history()
    );
    assert_eq!(
        noncommit.observed_session().newest_history(),
        session.newest_history()
    );
    assert_eq!(
        noncommit.observed_session().newest_root(),
        session.newest_root()
    );
}

#[test]
fn generation_frontier_cumulative_and_encoded_size_overflow_are_typed() {
    let (_home, store, storage, thread) = fixture("overflow", 170, 4_096);
    let durable = current(&storage, &store, thread);
    let errors = draft_edit_history_overflow_errors(
        durable.draft().piece_root(),
        DraftEditorCandidateSessionIdV1::from_bytes([172; 16]),
        DraftPieceOperationIdV1::from_bytes([173; 16]),
        point(0),
    );
    assert_eq!(
        errors,
        [
            DraftEditHistoryAppendErrorV1::GenerationOverflow,
            DraftEditHistoryAppendErrorV1::FrontierRevisionOverflow,
            DraftEditHistoryAppendErrorV1::CumulativePositionOverflow,
            DraftEditHistoryAppendErrorV1::EncodedSizeOverflow,
        ]
    );
}

#[test]
fn adoption_crash_cuts_reconcile_to_old_or_exact_complete_pair() {
    for (name, seed, fault, expected_at_cut) in [
        ("cut-before-commit", 80, FaultPoint::BeforeCommit, false),
        (
            "cut-after-commit",
            90,
            FaultPoint::AfterCommitBeforePersist,
            true,
        ),
        ("cut-after-persist", 100, FaultPoint::AfterPersist, true),
        (
            "cut-before-verification",
            110,
            FaultPoint::BeforeVerification,
            true,
        ),
    ] {
        let (_home, store, storage, faults, thread) = fault_fixture(name, seed, 4_096);
        let durable = current(&storage, &store, thread);
        let session = open_session(
            &storage,
            &store,
            &durable,
            seed.wrapping_add(2),
            seed.wrapping_add(3),
        );
        let edit = transaction(
            &storage,
            &store,
            &session,
            seed.wrapping_add(4),
            "crash-cut",
            point(9),
        );
        build(&storage, &store, &edit);
        faults.fail_next(fault);
        let outcome = execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        );
        let (store, storage) = if store.health().state() == HomeHealthState::Failed {
            let recovery = store.recover_same_home().unwrap();
            let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            (recovery.publish(), storage)
        } else {
            (store, storage)
        };
        let fragments = edit.fragments.clone();
        let reconciled = storage
            .reconcile_draft_piece_command_outcome(&store, &edit.prepared, outcome, |start| {
                fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .cloned()
                    .collect()
            })
            .unwrap();
        if expected_at_cut {
            let DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(
                DraftPieceSettlementProofV1::Settlement(committed),
            )) = reconciled
            else {
                panic!("committed crash cut did not reconcile to the exact terminal pair")
            };
            let DraftPieceSettlementOutcomeV1::Committed {
                successor, history, ..
            } = committed.outcome()
            else {
                panic!("committed reconciliation returned a noncommit settlement")
            };
            assert_eq!(history.root(), *successor);
        } else {
            assert!(matches!(
                reconciled,
                DraftPieceReconciledCommandV1::Pending(_)
            ));
            let retry = execute(
                &store,
                storage.settle_draft_piece_edit(
                    storage.revision(&store).unwrap(),
                    edit.prepared.clone(),
                ),
            );
            let fragments = edit.fragments.clone();
            assert!(matches!(
                storage
                    .reconcile_draft_piece_command_outcome(&store, &edit.prepared, retry, |start| {
                        fragments
                            .iter()
                            .skip((start - 1) as usize)
                            .cloned()
                            .collect()
                    },)
                    .unwrap(),
                DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(
                    _
                ))
            ));
        }
    }
}

#[test]
fn adoption_reconciliation_rejects_missing_replaced_or_corrupt_closure_records() {
    for (name, seed, target) in [
        ("missing-adopted-root", 120, 0_u8),
        ("missing-transition", 130, 1_u8),
        ("missing-adopted-frontier", 140, 2_u8),
        ("missing-settlement", 150, 3_u8),
        ("wrong-transition", 180, 4_u8),
        ("wrong-successor-frontier", 190, 5_u8),
        ("corrupt-settlement-closure", 200, 6_u8),
    ] {
        let (_home, store, storage, faults, thread) = fault_fixture(name, seed, 4_096);
        let durable = current(&storage, &store, thread);
        let session = open_session(
            &storage,
            &store,
            &durable,
            seed.wrapping_add(2),
            seed.wrapping_add(3),
        );
        let edit = transaction(
            &storage,
            &store,
            &session,
            seed.wrapping_add(4),
            "fault",
            point(5),
        );
        build(&storage, &store, &edit);
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let outcome = execute(
            &store,
            storage
                .settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
        );
        assert!(matches!(outcome, CommandOutcome::Indeterminate { .. }));
        let settlement = settled(&storage, &store, &edit);
        let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
            panic!("fault fixture did not commit")
        };
        let deletion = match target {
            0 => delete_draft_piece_immutable_record(
                &store,
                &storage,
                adoption.adopted_root().reference(),
                DraftPieceImmutableDeletion::Root,
            ),
            1 => delete_draft_edit_history_record(
                &store,
                storage.clone(),
                DraftEditHistoryRecordDeletion::Transition(adoption.transition().key()),
            ),
            2 => delete_draft_edit_history_record(
                &store,
                storage.clone(),
                DraftEditHistoryRecordDeletion::Frontier(
                    adoption.adopted_history().reference().key(),
                ),
            ),
            3 => delete_draft_piece_immutable_record(
                &store,
                &storage,
                adoption.adopted_root().reference(),
                DraftPieceImmutableDeletion::Settlement,
            ),
            4 => {
                let (wrong_transition, _) = alternative_ordinary_draft_edit_history(
                    adoption.predecessor_history(),
                    adoption.adopted_session().newest_candidate_generation(),
                    adoption.adopted_root().reference(),
                    adoption.transition().before_caret(),
                    adoption.transition().before_selection(),
                    point(4),
                    adoption.transition().after_selection(),
                    adoption.transition().operation_id(),
                );
                assert_eq!(wrong_transition.key(), adoption.transition().key());
                assert_ne!(wrong_transition.digest(), adoption.transition().digest());
                replace_draft_edit_history_transition(
                    &store,
                    storage.clone(),
                    adoption.transition().key(),
                    wrong_transition,
                )
            }
            5 => {
                let wrong_root = canonical_empty_draft_piece_root_v1(
                    session.draft_id(),
                    durable.draft().revision(),
                    DraftPieceOperationIdV1::from_bytes([201; 16]),
                );
                let (_, wrong_frontier) = alternative_ordinary_draft_edit_history(
                    adoption.predecessor_history(),
                    adoption.adopted_session().newest_candidate_generation(),
                    wrong_root.reference(),
                    adoption.transition().before_caret(),
                    adoption.transition().before_selection(),
                    adoption.transition().after_caret(),
                    adoption.transition().after_selection(),
                    adoption.transition().operation_id(),
                );
                assert_eq!(
                    wrong_frontier.reference().key(),
                    adoption.adopted_history().reference().key()
                );
                assert_ne!(
                    wrong_frontier.reference().root(),
                    adoption.adopted_root().reference()
                );
                replace_draft_edit_history_frontier(
                    &store,
                    storage.clone(),
                    adoption.adopted_history().reference().key(),
                    wrong_frontier,
                )
            }
            6 => {
                inject_draft_piece_settlement_closure_corruption(&store, &storage, settlement.key())
            }
            _ => unreachable!(),
        };
        committed(execute(&store, deletion));
        let fragments = edit.fragments.clone();
        assert!(
            storage
                .reconcile_draft_piece_command_outcome(&store, &edit.prepared, outcome, |start| {
                    fragments
                        .iter()
                        .skip((start - 1) as usize)
                        .cloned()
                        .collect()
                },)
                .is_err()
        );
    }
}

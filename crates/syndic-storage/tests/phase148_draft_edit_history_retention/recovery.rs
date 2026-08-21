use super::{common::commit_edit, support::*};

#[test]
fn eviction_commit_crash_cuts_recover_old_or_complete_successor() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("eviction-cut-measure", 120, 65_536);
    let durable = current(measure_storage, &measure_store, measure_thread);
    let session = open_session(measure_storage, &measure_store, &durable, 122, 123);
    let first = commit_edit(measure_storage, &measure_store, &session, 124, "a");
    let second = commit_edit(
        measure_storage,
        &measure_store,
        first.adopted_session(),
        125,
        "b",
    );
    let components =
        draft_edit_history_stored_charge_components(second.adopted_history(), second.transition())
            .unwrap();
    let budget = components[0] + components[2] + components[3] + components[5];

    for (name, seed, fault, committed_at_cut) in [
        ("eviction-before", 130, FaultPoint::BeforeCommit, false),
        (
            "eviction-after-commit",
            140,
            FaultPoint::AfterCommitBeforePersist,
            true,
        ),
        (
            "eviction-after-persist",
            150,
            FaultPoint::AfterPersist,
            true,
        ),
        (
            "eviction-before-verify",
            160,
            FaultPoint::BeforeVerification,
            true,
        ),
    ] {
        let (_home, store, storage, faults, thread) = fault_fixture(name, seed, budget);
        let durable = current(storage, &store, thread);
        let session = open_session(
            storage,
            &store,
            &durable,
            seed.wrapping_add(2),
            seed.wrapping_add(3),
        );
        let first = commit_edit(storage, &store, &session, seed.wrapping_add(4), "a");
        let edit = transaction(
            storage,
            &store,
            first.adopted_session(),
            seed.wrapping_add(5),
            "b",
            point(1),
        );
        build(storage, &store, &edit);
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
        if committed_at_cut {
            let DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(
                DraftPieceSettlementProofV1::Settlement(settlement),
            )) = reconciled
            else {
                panic!("eviction crash cut did not recover the committed successor")
            };
            let DraftPieceSettlementClosureV1::Committed(adoption) = settlement.closure() else {
                panic!("eviction crash cut returned a noncommit closure")
            };
            assert_eq!(
                adoption.adopted_history().oldest_eligible(),
                Some(adoption.transition().reference())
            );
            assert_eq!(adoption.adopted_history().retained_encoded_bytes(), budget);
        } else {
            assert!(matches!(
                reconciled,
                DraftPieceReconciledCommandV1::Pending(_)
            ));
            let DraftEditorCandidateSessionReadOutcomeV1::Active(value) = storage
                .draft_editor_candidate_session(
                    &store,
                    first.adopted_session().draft_id(),
                    first.adopted_session().session_id(),
                )
                .unwrap()
            else {
                panic!("pre-commit crash cut did not retain the active predecessor")
            };
            assert_eq!(value.newest_root(), first.adopted_session().newest_root());
            assert_eq!(
                value.newest_history(),
                first.adopted_session().newest_history()
            );
        }
    }
}

#[test]
fn capacity_unavailable_is_terminal_at_every_commit_crash_cut_without_a_successor_root() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("capacity-cut-measure", 188, 4_096);
    let durable = current(measure_storage, &measure_store, measure_thread);
    let session = open_session(measure_storage, &measure_store, &durable, 189, 190);
    let adoption = commit_edit(measure_storage, &measure_store, &session, 191, "capacity");
    let parts = draft_edit_history_stored_charge_components(
        adoption.adopted_history(),
        adoption.transition(),
    )
    .unwrap();
    let budget = parts[0] + parts[2] + parts[3] + parts[5] - 1;

    for (name, seed, fault) in [
        ("capacity-before", 192, FaultPoint::BeforeCommit),
        (
            "capacity-after-commit",
            200,
            FaultPoint::AfterCommitBeforePersist,
        ),
        ("capacity-after-persist", 208, FaultPoint::AfterPersist),
        (
            "capacity-before-verify",
            216,
            FaultPoint::BeforeVerification,
        ),
    ] {
        let (_home, store, storage, faults, thread) = fault_fixture(name, seed, budget);
        let durable = current(storage, &store, thread);
        let session = open_session(
            storage,
            &store,
            &durable,
            seed.wrapping_add(2),
            seed.wrapping_add(3),
        );
        let edit = transaction(
            storage,
            &store,
            &session,
            seed.wrapping_add(4),
            "capacity",
            point(8),
        );
        build(storage, &store, &edit);
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
        let settlement = match reconciled {
            DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(
                DraftPieceSettlementProofV1::Settlement(settlement),
            )) => settlement,
            DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Error(
                DraftPieceSettlementProofV1::Settlement(settlement),
            )) => settlement,
            DraftPieceReconciledCommandV1::Pending(_) => {
                committed(execute(
                    &store,
                    storage.settle_draft_piece_edit(
                        storage.revision(&store).unwrap(),
                        edit.prepared.clone(),
                    ),
                ));
                settled(storage, &store, &edit)
            }
            other => panic!("capacity crash cut did not reconcile: {other:?}"),
        };
        assert!(matches!(
            settlement.outcome(),
            DraftPieceSettlementOutcomeV1::Error(
                DraftPieceErrorReasonV1::HistoryCapacityUnavailable
            )
        ));
        let DraftPieceSettlementClosureV1::Noncommit(noncommit) = settlement.closure() else {
            panic!("capacity crash cut wrote a committed successor")
        };
        let proposed = noncommit
            .proposed_successor()
            .expect("completed capacity build retains its successor reference");
        assert!(!draft_edit_history_root_exists(&store, storage, proposed));
        assert_eq!(
            noncommit.observed_session().newest_root(),
            session.newest_root()
        );
        assert_eq!(
            noncommit.observed_session().newest_history(),
            session.newest_history()
        );
    }
}

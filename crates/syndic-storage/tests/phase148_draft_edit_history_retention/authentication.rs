use super::{common::commit_edit, support::*};

#[test]
fn one_byte_under_required_closure_is_distinct_and_writes_no_successor_pair() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("required-measure", 60, 4_096);
    let durable = current(measure_storage, &measure_store, measure_thread);
    let session = open_session(measure_storage, &measure_store, &durable, 62, 63);
    let adoption = commit_edit(measure_storage, &measure_store, &session, 64, "x");
    let components = draft_edit_history_stored_charge_components(
        adoption.adopted_history(),
        adoption.transition(),
    )
    .unwrap();
    let required = components[0] + components[2] + components[3] + components[5];

    let (_home, store, storage, thread) = fixture("required-under", 70, required - 1);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 72, 73);
    let edit = transaction(storage, &store, &session, 74, "x", point(1));
    build(storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let settlement = settled(storage, &store, &edit);
    assert!(matches!(
        settlement.outcome(),
        DraftPieceSettlementOutcomeV1::Error(DraftPieceErrorReasonV1::HistoryCapacityUnavailable)
    ));
    let DraftPieceSettlementClosureV1::Noncommit(noncommit) = settlement.closure() else {
        panic!("capacity outcome wrote a committed closure")
    };
    assert_eq!(
        noncommit.observed_session().newest_root(),
        session.newest_root()
    );
    assert_eq!(
        noncommit.observed_session().newest_history(),
        session.newest_history()
    );
    let proposed = noncommit
        .proposed_successor()
        .expect("completed build retains the proposed successor reference");
    assert!(!draft_edit_history_root_exists(&store, storage, proposed));
    assert!(draft_edit_history_root_exists(
        &store,
        storage,
        session.newest_root()
    ));
}
#[test]
fn policy_and_family_bounds_remain_exact() {
    assert!(DraftEditHistoryPolicyV1::new(0, 1).is_none());
    assert!(DraftEditHistoryPolicyV1::new(1, 0).is_none());
    assert!(DraftEditHistoryPolicyV1::new(1, 1).is_some());
    let names = syndic_v5_family_names();
    assert_eq!(names.len(), 78);
    assert_eq!(names[18], "draft-edit-history-frontiers");
    assert_eq!(names[19], "draft-edit-history-transitions");
    assert_eq!(names[20], "draft-historical-root-adoptions");
    assert_eq!(names[21], "draft-composer-builds");
    assert_eq!(names[22], "draft-composer-materializations");
}

#[test]
fn current_draft_fails_closed_on_missing_or_corrupt_retained_history() {
    let (_home, store, storage, thread) = fixture("current-missing-history", 71, 4_096);
    let durable = current(storage, &store, thread);
    committed(execute(
        &store,
        delete_draft_edit_history_frontier(&store, storage, durable.draft().history().key()),
    ));
    assert!(
        storage
            .current_draft(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
            .is_err()
    );

    let (_home, store, storage, thread) = fixture("current-corrupt-history", 75, 4_096);
    let durable = current(storage, &store, thread);
    inject_draft_edit_history_frontier_digest_corruption(
        &store,
        storage,
        durable.draft().history().key(),
    )
    .unwrap();
    assert!(
        storage
            .current_draft(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
            .is_err()
    );
}

#[test]
fn fabricated_cumulative_gaps_and_wrong_head_root_fail_closed() {
    let (_home, store, storage, thread) = fixture("no-head-gap", 78, 4_096);
    let durable = current(storage, &store, thread);
    let empty = canonical_empty_draft_edit_history_v1(
        durable.root().reference(),
        DraftEditHistoryPolicyV1::new(4_096, 1).unwrap(),
    );
    not_committed(execute(
        &store,
        replace_draft_edit_history_frontier(
            &store,
            storage,
            empty.reference().key(),
            draft_edit_history_no_head_gap(&empty),
        ),
    ));
    assert!(
        storage
            .current_draft(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
            .unwrap()
            .is_some()
    );

    let (_home, store, storage, thread) = fixture("first-transition-gap", 180, 4_096);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 181, 182);
    let adoption = commit_edit(storage, &store, &session, 183, "x");
    let (replacement, transition) =
        draft_edit_history_first_transition_gap(adoption.adopted_history(), adoption.transition());
    committed(execute(
        &store,
        replace_draft_edit_history_frontier_and_session(
            &store,
            storage,
            adoption.adopted_session().clone(),
            replacement,
            Some(transition),
        ),
    ));
    let gap_read = storage.draft_editor_candidate_session(
        &store,
        adoption.adopted_session().draft_id(),
        adoption.adopted_session().session_id(),
    );
    assert!(matches!(
        gap_read,
        Err(_) | Ok(DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure)
    ));

    let (_home, store, storage, thread) = fixture("wrong-head-root", 190, 4_096);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 191, 192);
    let first = commit_edit(storage, &store, &session, 193, "x");
    let second = commit_edit(storage, &store, first.adopted_session(), 194, "y");
    let replacement =
        draft_edit_history_wrong_head_root(second.adopted_history(), first.transition());
    committed(execute(
        &store,
        replace_draft_edit_history_frontier_and_session(
            &store,
            storage,
            second.adopted_session().clone(),
            replacement,
            None,
        ),
    ));
    assert!(matches!(
        storage
            .draft_editor_candidate_session(
                &store,
                second.adopted_session().draft_id(),
                second.adopted_session().session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));
}

#[test]
fn missing_immediate_journal_predecessor_fails_reopen_authentication() {
    let (_home, store, storage, thread) = fixture("missing-predecessor", 80, 65_536);
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 82, 83);
    let first = commit_edit(storage, &store, &session, 84, "first");
    let second = commit_edit(storage, &store, first.adopted_session(), 85, "second");
    committed(execute(
        &store,
        delete_draft_edit_history_record(
            &store,
            storage,
            DraftEditHistoryRecordDeletion::Transition(first.transition().key()),
        ),
    ));
    assert!(matches!(
        storage
            .draft_editor_candidate_session(
                &store,
                second.adopted_session().draft_id(),
                second.adopted_session().session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
    ));
}

#[test]
fn exact_accounting_and_availability_cannot_cross_the_floor() {
    let (_measure_home, measure_store, measure_storage, measure_thread) =
        fixture("cross-floor-measure", 90, 65_536);
    let durable = current(measure_storage, &measure_store, measure_thread);
    let session = open_session(measure_storage, &measure_store, &durable, 92, 93);
    let first = commit_edit(measure_storage, &measure_store, &session, 94, "a");
    let second = commit_edit(
        measure_storage,
        &measure_store,
        first.adopted_session(),
        95,
        "b",
    );
    let components =
        draft_edit_history_stored_charge_components(second.adopted_history(), second.transition())
            .unwrap();
    let saturated_budget = components[0] + components[2] + components[3] + components[5];

    for (name, seed, accounting) in [
        ("retained-accounting", 100, true),
        ("availability-floor", 110, false),
    ] {
        let (_home, store, storage, thread) = fixture(name, seed, saturated_budget);
        let durable = current(storage, &store, thread);
        let session = open_session(
            storage,
            &store,
            &durable,
            seed.wrapping_add(2),
            seed.wrapping_add(3),
        );
        let first = commit_edit(storage, &store, &session, seed.wrapping_add(4), "a");
        let second = commit_edit(
            storage,
            &store,
            first.adopted_session(),
            seed.wrapping_add(5),
            "b",
        );
        assert_eq!(
            second.adopted_history().oldest_eligible(),
            Some(second.transition().reference())
        );
        let replacement = if accounting {
            draft_edit_history_accounting_corruption(second.adopted_history())
        } else {
            draft_edit_history_availability_corruption(
                second.adopted_history(),
                first.transition().reference(),
            )
        };
        committed(execute(
            &store,
            replace_draft_edit_history_frontier_and_session(
                &store,
                storage,
                second.adopted_session().clone(),
                replacement,
                None,
            ),
        ));
        assert!(matches!(
            storage
                .draft_editor_candidate_session(
                    &store,
                    second.adopted_session().draft_id(),
                    second.adopted_session().session_id(),
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure
        ));
    }
}

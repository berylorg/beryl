use super::*;

#[test]
fn real_home_store_writer_cuts_select_one_exact_batch_state() {
    for (index, point) in [
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommitBeforePersist,
        FaultPoint::AfterPersist,
    ]
    .into_iter()
    .enumerate()
    {
        let faults = FaultController::new();
        let fixture =
            receiving_fault_fixture(&format!("fault-{index}"), 70 + index as u8, faults.clone());
        let prepared = prepare(&fixture, proposal_inputs(&["a", "b"]));
        let reconciliation = prepared.clone();
        faults.fail_next(point);
        let outcome = execute(
            &fixture.store,
            fixture.storage.draft_mutation_staging_page_batch(
                fixture.storage.revision(&fixture.store).unwrap(),
                prepared,
            ),
        );
        let expected = match point {
            FaultPoint::BeforeCommit => {
                assert!(matches!(
                    &outcome,
                    CommandOutcome::NotCommitted {
                        evidence: CommandError::Commit { .. },
                    }
                ));
                DraftMutationStagingReconcileV1::SourceSelected
            }
            FaultPoint::AfterCommitBeforePersist => {
                assert!(matches!(
                    &outcome,
                    CommandOutcome::Indeterminate {
                        failure: CommandError::Persistence { .. },
                        ..
                    }
                ));
                DraftMutationStagingReconcileV1::TargetSelected
            }
            FaultPoint::AfterPersist => {
                assert!(matches!(
                    &outcome,
                    CommandOutcome::Committed {
                        later_failure: Some(CommandError::Persistence { .. }),
                        ..
                    }
                ));
                DraftMutationStagingReconcileV1::TargetSelected
            }
            _ => unreachable!("only writer-consumed cuts are covered"),
        };
        let ReceivingFixture {
            home: _home,
            store,
            storage,
            ..
        } = fixture;
        let (store, storage) = if store.health().state() == HomeHealthState::Failed {
            let recovery = store.recover_same_home().unwrap();
            let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
            (recovery.publish(), storage)
        } else {
            (store, storage)
        };
        assert_eq!(
            storage
                .reconcile_draft_mutation_staging_page_batch(&store, &reconciliation)
                .unwrap(),
            expected,
            "fault cut {point:?}",
        );
    }
}

#[test]
fn equal_and_unequal_partial_prefixes_and_missing_target_fail_closed() {
    for (name, pages, receipts) in [
        ("page-prefix", 1, 0),
        ("equal-prefix", 1, 1),
        ("pages-only", 2, 0),
    ] {
        let fixture = receiving_fixture(name, 90 + pages as u8 + receipts as u8);
        let prepared = prepare(&fixture, proposal_inputs(&["a", "b"]));
        committed(execute(
            &fixture.store,
            inject_draft_mutation_staging_batch_prefix(
                &fixture.store,
                fixture.storage,
                &prepared,
                pages,
                receipts,
            ),
        ));
        assert!(
            fixture
                .storage
                .reconcile_draft_mutation_staging_page_batch(&fixture.store, &prepared)
                .is_err()
        );
        assert!(matches!(
            execute(
                &fixture.store,
                fixture.storage.draft_mutation_staging_page_batch(
                    fixture.storage.revision(&fixture.store).unwrap(),
                    prepared,
                ),
            ),
            CommandOutcome::NotCommitted {
                evidence: CommandError::ContributorValidation { .. },
            }
        ));
        assert_eq!(
            fixture
                .storage
                .draft_mutation_staging_head(&fixture.store, fixture.identity)
                .unwrap(),
            Some(fixture.head),
        );
    }

    let unequal = receiving_fixture("unequal-prefix", 99);
    let expected = prepare(&unequal, proposal_inputs(&["a", "b"]));
    let alternative = prepare(&unequal, proposal_inputs(&["different", "b"]));
    let expected_page = draft_mutation_staging_batch_target(&expected, 0).unwrap().0;
    let occupied_page = draft_mutation_staging_batch_target(&alternative, 0)
        .unwrap()
        .0;
    assert_eq!(expected_page.key(), occupied_page.key());
    assert_ne!(expected_page, occupied_page);
    committed(execute(
        &unequal.store,
        inject_draft_mutation_staging_occupied_page(
            &unequal.store,
            unequal.storage,
            occupied_page.clone(),
        ),
    ));
    assert_eq!(
        draft_mutation_staging_batch_target_records(&unequal.store, unequal.storage, &expected, 0,)
            .unwrap(),
        (Some(occupied_page.clone()), None),
    );
    assert_eq!(
        draft_mutation_staging_batch_target_records(&unequal.store, unequal.storage, &expected, 1,)
            .unwrap(),
        (None, None),
    );
    assert!(
        unequal
            .storage
            .reconcile_draft_mutation_staging_page_batch(&unequal.store, &expected)
            .is_err()
    );
    assert!(matches!(
        execute(
            &unequal.store,
            unequal.storage.draft_mutation_staging_page_batch(
                unequal.storage.revision(&unequal.store).unwrap(),
                expected.clone(),
            ),
        ),
        CommandOutcome::NotCommitted {
            evidence: CommandError::ContributorValidation { .. },
        }
    ));
    assert_eq!(
        draft_mutation_staging_batch_target_records(&unequal.store, unequal.storage, &expected, 0,)
            .unwrap(),
        (Some(occupied_page), None),
    );
    assert_eq!(
        draft_mutation_staging_batch_target_records(&unequal.store, unequal.storage, &expected, 1,)
            .unwrap(),
        (None, None),
    );
    assert_eq!(
        unequal
            .storage
            .draft_mutation_staging_head(&unequal.store, unequal.identity)
            .unwrap(),
        Some(unequal.head.clone()),
    );
    assert_eq!(
        unequal
            .storage
            .draft_editor_candidate_session(
                &unequal.store,
                unequal.session.draft_id(),
                unequal.session.session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(unequal.session.clone()),
    );

    let missing = receiving_fixture("missing-target", 100);
    let prepared = prepare(&missing, proposal_inputs(&["a", "b"]));
    let replay = prepared.clone();
    let missing_key = draft_mutation_staging_batch_target(&prepared, 0)
        .unwrap()
        .0
        .key();
    committed(execute(
        &missing.store,
        missing.storage.draft_mutation_staging_page_batch(
            missing.storage.revision(&missing.store).unwrap(),
            prepared,
        ),
    ));
    committed(execute(
        &missing.store,
        delete_draft_mutation_staging_page(&missing.store, missing.storage, missing_key),
    ));
    assert!(
        missing
            .storage
            .reconcile_draft_mutation_staging_page_batch(&missing.store, &replay)
            .is_err()
    );
    assert!(matches!(
        execute(
            &missing.store,
            missing.storage.draft_mutation_staging_page_batch(
                missing.storage.revision(&missing.store).unwrap(),
                replay,
            ),
        ),
        CommandOutcome::NotCommitted {
            evidence: CommandError::ContributorValidation { .. },
        }
    ));
}

#[test]
fn locally_exact_near_max_source_frontiers_reach_checked_overflow_paths() {
    let fixture = receiving_fixture("checked-overflow", 110);
    let input = |input_cursor, successor_cursor| {
        Box::new([DraftMutationStagingPageInputV1::new(
            DraftMutationStagingLaneV1::Source,
            input_cursor,
            successor_cursor,
            1,
            1024,
            Box::new([DraftMutationStagingPageItemV1::SourcePosition(point(
                input_cursor,
            ))]),
        )])
    };

    let item_overflow =
        draft_mutation_staging_locally_exact_source_head(&fixture.head, 0, 1, u64::MAX, 0);
    assert!(matches!(
        fixture.storage.prepare_draft_mutation_staging_page_batch(
            &item_overflow,
            &fixture.session,
            input(0, 1),
        ),
        Err(DraftMutationStagingErrorV1::Overflow)
    ));

    let byte_overflow =
        draft_mutation_staging_locally_exact_source_head(&fixture.head, 0, 1, 0, u64::MAX);
    assert!(matches!(
        fixture.storage.prepare_draft_mutation_staging_page_batch(
            &byte_overflow,
            &fixture.session,
            input(0, 1),
        ),
        Err(DraftMutationStagingErrorV1::Overflow)
    ));

    let ordinal_and_cursor_overflow = draft_mutation_staging_locally_exact_source_head(
        &fixture.head,
        u64::MAX - 1,
        u64::MAX,
        0,
        0,
    );
    assert!(matches!(
        fixture.storage.prepare_draft_mutation_staging_page_batch(
            &ordinal_and_cursor_overflow,
            &fixture.session,
            input(u64::MAX - 1, u64::MAX),
        ),
        Err(DraftMutationStagingErrorV1::Overflow)
    ));

    let exhausted_cursor =
        draft_mutation_staging_locally_exact_source_head(&fixture.head, u64::MAX, 1, 0, 0);
    assert!(matches!(
        fixture.storage.prepare_draft_mutation_staging_page_batch(
            &exhausted_cursor,
            &fixture.session,
            input(u64::MAX, u64::MAX),
        ),
        Err(DraftMutationStagingErrorV1::Invalid)
    ));
    assert_eq!(
        fixture
            .storage
            .draft_mutation_staging_head(&fixture.store, fixture.identity)
            .unwrap(),
        Some(fixture.head.clone()),
    );
    assert_eq!(
        fixture
            .storage
            .draft_editor_candidate_session(
                &fixture.store,
                fixture.session.draft_id(),
                fixture.session.session_id(),
            )
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Active(fixture.session),
    );
}

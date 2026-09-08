use super::*;

#[test]
fn initial_mapping_is_implicit_and_partial_mapping_reopens_before_cancellation() {
    let (home, store, storage, thread) = fixture("mapping-reopen-cancel", 10);
    let session = open_session(&storage, &store, &current(&storage, &store, thread), 11, 12);
    let identity = stage_text(&storage, &store, &session, 13, 32);
    let initial = staged_outcome_build_for_test(&storage, &store, identity);
    let mapping = draft_build_mapping_snapshot(&initial).unwrap();
    assert_eq!(
        (mapping.current_source_units, mapping.current_target_units),
        (0, 0)
    );
    assert_eq!(mapping.stage_tag, 0);
    assert!(
        draft_build_mapping_root_key_for_test(
            &storage,
            &store,
            &initial,
            DraftBuildMappingRootForTest::Current
        )
        .is_none()
    );
    let partial = advance_until(&storage, &store, identity, |_, mapping| {
        mapping.stage_tag == 20
    });
    let snapshot = draft_build_mapping_snapshot(&partial).unwrap();
    assert_eq!(snapshot.current_target_units, 0);
    assert_eq!(snapshot.pending_target_units, Some(32));
    let key = draft_build_mapping_root_key_for_test(
        &storage,
        &store,
        &partial,
        DraftBuildMappingRootForTest::Pending,
    )
    .unwrap();
    let bytes = draft_build_mapping_record_for_test(&storage, &store, key).unwrap();
    assert_unadopted(&storage, &store, &session);
    drop(storage);
    drop(store);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    assert!(matches!(
        storage
            .draft_mutation_staging_status(&store, identity)
            .unwrap(),
        DraftMutationStagingStatusV1::Building { .. }
    ));
    let reopened = staged_outcome_build_for_test(&storage, &store, identity);
    assert_eq!(reopened, partial);
    let command = storage
        .prepare_staged_draft_piece_terminal(
            &store,
            identity,
            reopened.progress_receipt(),
            Election::Cancel,
        )
        .unwrap();
    let completion = complete(command.submit(&store), &store);
    assert!(matches!(
        completion.result,
        DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Cancelled(_))
    ));
    let terminal = staged_outcome_build_for_test(&storage, &store, identity);
    assert_eq!(draft_build_mapping_snapshot(&terminal), Some(snapshot));
    assert_eq!(
        draft_build_mapping_record_for_test(&storage, &store, key),
        Some(bytes)
    );
    assert_unadopted(&storage, &store, &session);
}

#[test]
fn missing_and_substituted_pending_mapping_roots_refuse_reopen_and_construction() {
    for substituted in [false, true] {
        let (_home, store, storage, thread) =
            fixture("mapping-pending-corruption", 20 + u8::from(substituted));
        let session = open_session(&storage, &store, &current(&storage, &store, thread), 24, 25);
        let identity = stage_text(&storage, &store, &session, 26, 32);
        let build = advance_until(&storage, &store, identity, |_, mapping| {
            mapping.stage_tag == 20
        });
        let key = draft_build_mapping_root_key_for_test(
            &storage,
            &store,
            &build,
            DraftBuildMappingRootForTest::Pending,
        )
        .unwrap();
        if substituted {
            substitute_draft_build_mapping_record_for_test(&storage, &store, key);
        } else {
            delete_draft_build_mapping_record_for_test(&storage, &store, key);
        }
        let revision = store.home_revision().unwrap();
        assert!(
            storage
                .draft_mutation_staging_status(&store, identity)
                .is_err()
        );
        assert!(
            storage
                .prepare_staged_draft_piece_advance(&store, identity, build.progress_receipt())
                .is_err()
        );
        assert_eq!(store.home_revision().unwrap(), revision);
        assert_unadopted(&storage, &store, &session);
    }
}

#[test]
fn immediate_predecessor_mapping_root_remains_required_after_coherent_installation() {
    let (_home, store, storage, thread) = fixture("mapping-prior-root", 30);
    let session = open_session(&storage, &store, &current(&storage, &store, thread), 31, 32);
    let identity = stage_text(&storage, &store, &session, 33, 40_000);
    let build = advance_until(&storage, &store, identity, |build, mapping| {
        mapping.stage_tag == 0
            && mapping.current_target_units == 40_000
            && draft_build_mapping_root_key_for_test(
                &storage,
                &store,
                build,
                DraftBuildMappingRootForTest::PreviousCurrent,
            )
            .is_some()
    });
    let current = draft_build_mapping_root_key_for_test(
        &storage,
        &store,
        &build,
        DraftBuildMappingRootForTest::Current,
    )
    .unwrap();
    let prior = draft_build_mapping_root_key_for_test(
        &storage,
        &store,
        &build,
        DraftBuildMappingRootForTest::PreviousCurrent,
    )
    .unwrap();
    assert_ne!(current, prior);
    delete_draft_build_mapping_record_for_test(&storage, &store, prior);
    assert!(draft_build_mapping_record_for_test(&storage, &store, current).is_some());
    assert!(
        storage
            .draft_mutation_staging_status(&store, identity)
            .is_err()
    );
    assert!(
        storage
            .prepare_staged_draft_piece_advance(&store, identity, build.progress_receipt())
            .is_err()
    );
    assert_unadopted(&storage, &store, &session);
}

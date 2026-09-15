use super::*;

#[test]
fn same_root_competitors_keep_the_first_complete_mapping() {
    let (_home, store, storage, _) = fixture("same-root-competitors", 10);
    let first = materialization_key(matching_root(&storage, &store, 20), 21);
    let second = materialization_key(first.source(), 22);
    stage_partial_output(&storage, &store, first, true);
    begin(&storage, &store, second);
    reach(
        &storage,
        &store,
        second,
        DraftComposerBuildPhaseV1::ReadyToSeal,
    );
    let stale = storage
        .prepare_draft_composer_materialization_step(&store, second)
        .unwrap()
        .unwrap();
    let winner = materialize_existing(&storage, &store, first);
    assert!(matches!(
        execute(
            &store,
            storage
                .advance_draft_composer_materialization(storage.revision(&store).unwrap(), stale)
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        storage
            .draft_composer_materialization_status(&store, second)
            .unwrap(),
        DraftComposerMaterializationStatusV1::Sealed(winner)
    );
    assert!(
        storage
            .prepare_draft_composer_materialization_step(&store, second)
            .unwrap()
            .is_none()
    );
}

#[test]
fn stale_manifest_creation_and_append_require_fresh_preparation() {
    for planning in [true, false] {
        let (_home, store, storage, _) = fixture("stale-shared-frontier", 10);
        let first = materialization_key(matching_root(&storage, &store, 20), 21);
        let second = materialization_key(matching_root(&storage, &store, 30), 31);
        begin(&storage, &store, first);
        begin(&storage, &store, second);
        let target = if planning {
            DraftComposerBuildPhaseV1::Writing
        } else {
            DraftComposerBuildPhaseV1::Draining { final_chunk: false }
        };
        if !planning {
            reach(&storage, &store, first, DraftComposerBuildPhaseV1::Writing);
            reach(&storage, &store, second, DraftComposerBuildPhaseV1::Writing);
        }
        let stale = loop {
            let prepared = storage
                .prepare_draft_composer_materialization_step(&store, second)
                .unwrap()
                .unwrap();
            if prepared.next_phase() == Some(target) {
                break prepared;
            }
            committed(execute(
                &store,
                storage.advance_draft_composer_materialization(
                    storage.revision(&store).unwrap(),
                    prepared,
                ),
            ));
        };
        reach(&storage, &store, first, target);
        assert!(matches!(
            execute(
                &store,
                storage.advance_draft_composer_materialization(
                    storage.revision(&store).unwrap(),
                    stale
                )
            ),
            CommandOutcome::NotCommitted { .. }
        ));
        let original = materialize_existing(&storage, &store, first);
        assert_eq!(
            original.content(),
            materialize_existing(&storage, &store, second).content()
        );
    }
}

#[test]
fn replay_resumes_after_indeterminate_commits_at_every_step() {
    let (_home, store, storage, _, faults, _assets) = fixture_with_faults("replay-custody", 10);
    let first = materialization_key(matching_root(&storage, &store, 20), 21);
    let second = materialization_key(matching_root(&storage, &store, 30), 31);
    let original = materialize(&storage, &store, first);
    begin(&storage, &store, second);
    for _ in 0..128 {
        if let DraftComposerMaterializationStatusV1::Sealed(mapping) = storage
            .draft_composer_materialization_status(&store, second)
            .unwrap()
        {
            assert_eq!(original.content(), mapping.content());
            return;
        }
        let prepared = storage
            .prepare_draft_composer_materialization_step(&store, second)
            .unwrap()
            .unwrap();
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        assert!(matches!(
            execute(
                &store,
                storage.advance_draft_composer_materialization(
                    storage.revision(&store).unwrap(),
                    prepared
                )
            ),
            CommandOutcome::Indeterminate { .. }
        ));
    }
    panic!("replay did not converge from committed records");
}

#[test]
fn building_prefix_requires_chunks_but_can_complete_missing_indexes() {
    for corruption in [
        DraftComposerOutputCorruption::Chunk,
        DraftComposerOutputCorruption::ByteSpan,
        DraftComposerOutputCorruption::TextSpan,
        DraftComposerOutputCorruption::Piece,
    ] {
        let (_home, store, storage, _) = fixture("building-prefix-closure", 10);
        let first = materialization_key(matching_root(&storage, &store, 20), 21);
        let second = materialization_key(matching_root(&storage, &store, 30), 31);
        let partial = stage_partial_output(&storage, &store, first, true);
        committed(execute(
            &store,
            inject_draft_composer_output_corruption(
                &store,
                storage.clone(),
                partial.id(),
                corruption,
            ),
        ));
        if matches!(
            corruption,
            DraftComposerOutputCorruption::TextSpan | DraftComposerOutputCorruption::Piece
        ) {
            assert_eq!(
                materialize(&storage, &store, second).content().id(),
                partial.id()
            );
        } else {
            begin(&storage, &store, second);
            reach(&storage, &store, second, DraftComposerBuildPhaseV1::Writing);
            let mut refused = false;
            for _ in 0..128 {
                let prepared = storage
                    .prepare_draft_composer_materialization_step(&store, second)
                    .unwrap()
                    .unwrap();
                if matches!(
                    execute(
                        &store,
                        storage.advance_draft_composer_materialization(
                            storage.revision(&store).unwrap(),
                            prepared
                        )
                    ),
                    CommandOutcome::NotCommitted { .. }
                ) {
                    refused = true;
                    break;
                }
            }
            assert!(refused, "invalid shared prefix accepted: {corruption:?}");
        }
    }
}

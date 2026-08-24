use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandOutcome, CursorReadLimits, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    ExecutionBinding, ImageLabelOrdinal, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicDraftMarkerId, SyndicThreadId,
};
#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{
    DraftComposerBuildCorruption, DraftComposerOutputCorruption, DraftPieceDescendantCorruption,
    DraftPieceDescendantTarget, delete_draft_composer_origin_build, delete_draft_composer_source,
    draft_composer_build_truncation_is_rejected, draft_composer_full_carry_remaining_bytes,
    draft_composer_mapping_truncation_is_rejected, draft_composer_provisional_output,
    draft_composer_terminal_build_encoded_size, draft_composer_terminal_build_has_maximal_shape,
    inject_draft_composer_build_corruption, inject_draft_composer_chunk_corruption,
    inject_draft_composer_manifest_corruption, inject_draft_composer_mapping_corruption,
    inject_draft_composer_output_corruption, inject_draft_composer_prepared_chunk,
    inject_draft_piece_descendant_corruption, syndic_v5_family_names,
};
use syndic_storage::{
    CreateThread, DRAFT_COMPOSER_INPUT_MAX_BYTES, DRAFT_COMPOSER_READ_MAX_RECORDS,
    DRAFT_COMPOSER_RESIDENT_MAX_BYTES, DRAFT_COMPOSER_WRITE_MAX_RECORDS, DraftComposerBuildKeyV1,
    DraftComposerBuildPhaseV1, DraftComposerFormatV1, DraftComposerMaterializationOperationIdV1,
    DraftComposerMaterializationStatusV1, DraftCompositeGapWitnessV1, DraftCompositePositionV1,
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionOpenOutcomeV1,
    DraftEditorCandidateSessionOpenRequestV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftEditorCandidateSessionV1, DraftEditorCurrentSelectorV1, DraftPieceEditHeaderV1,
    DraftPieceMarkerV1, DraftPieceOperationIdV1, DraftPieceReplacementV1, DraftPieceV1,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase137-{name}-{}-{}",
            std::process::id(),
            NEXT_HOME.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        for attempt in 0..8 {
            match std::fs::remove_dir_all(&self.0) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(_) if attempt < 7 => {
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(_) => return,
            }
        }
    }
}

#[test]
fn terminal_lifecycles_leave_orphans_invisible_and_successor_can_seal() {
    let (_home, store, storage, thread) = fixture("terminal", 51);
    let root = storage
        .current_draft(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
        .draft()
        .piece_root();

    let cancelled = materialization_key(root, 61);
    committed(execute(
        &store,
        storage.begin_draft_composer_materialization(storage.revision(&store).unwrap(), cancelled),
    ));
    committed(execute(
        &store,
        storage.cancel_draft_composer_materialization(storage.revision(&store).unwrap(), cancelled),
    ));
    assert_eq!(
        storage
            .draft_composer_materialization_status(&store, cancelled)
            .unwrap(),
        DraftComposerMaterializationStatusV1::Cancelled,
    );

    let failed = materialization_key(root, 62);
    committed(execute(
        &store,
        storage.begin_draft_composer_materialization(storage.revision(&store).unwrap(), failed),
    ));
    committed(execute(
        &store,
        storage.fail_draft_composer_materialization(storage.revision(&store).unwrap(), failed),
    ));
    assert!(matches!(
        storage
            .draft_composer_materialization_status(&store, failed)
            .unwrap(),
        DraftComposerMaterializationStatusV1::Failed(_)
    ));

    let predecessor = materialization_key(root, 63);
    committed(execute(
        &store,
        storage
            .begin_draft_composer_materialization(storage.revision(&store).unwrap(), predecessor),
    ));
    let plan = storage
        .prepare_draft_composer_materialization_step(&store, predecessor)
        .unwrap()
        .unwrap();
    committed(execute(
        &store,
        storage.advance_draft_composer_materialization(storage.revision(&store).unwrap(), plan),
    ));
    let successor_id = DraftComposerMaterializationOperationIdV1::from_bytes([64; 16]);
    committed(execute(
        &store,
        storage.supersede_draft_composer_materialization(
            storage.revision(&store).unwrap(),
            predecessor,
            successor_id,
        ),
    ));
    assert_eq!(
        storage
            .draft_composer_materialization_status(&store, predecessor)
            .unwrap(),
        DraftComposerMaterializationStatusV1::Superseded(successor_id),
    );
    let successor =
        DraftComposerBuildKeyV1::new(root, DraftComposerFormatV1::ComposerV1, successor_id);
    let mapping = materialize(storage, &store, successor);
    assert_eq!(mapping.content().summary().encoded_bytes(), 9);
    assert_eq!(
        storage
            .content_chunks(
                &store,
                mapping.content().id(),
                None,
                CursorReadLimits::new(2, 65_536).unwrap(),
            )
            .unwrap()
            .records()[0]
            .bytes(),
        &[1, 0, 0, 0, 0, 0, 0, 0, 0],
    );
}

#[test]
fn multi_page_utf8_source_reopens_at_every_durable_frontier() {
    let (home, mut store, mut storage, thread) = fixture("reopen", 71);
    let mut pieces = Vec::new();
    for _ in 0..130 {
        pieces.push(DraftPieceV1::Text("x".to_owned()));
    }
    let root = replace_empty(storage, &store, thread, 72, pieces);
    assert!(root.summary().height() >= 2);
    let root = append_text(storage, &store, thread, 73, &"x".repeat(32_000));
    assert_eq!(root.summary().piece_count(), 131);
    let suffix = format!("{}💎z", "a".repeat(32_207));
    let root = append_text(storage, &store, thread, 74, &suffix);
    let key = materialization_key(root, 75);
    committed(execute(
        &store,
        storage.begin_draft_composer_materialization(storage.revision(&store).unwrap(), key),
    ));

    let mut steps = 0;
    let mut reopened_phases = 0_u8;
    let mapping = loop {
        if let DraftComposerMaterializationStatusV1::Sealed(mapping) = storage
            .draft_composer_materialization_status(&store, key)
            .unwrap()
        {
            break mapping;
        }
        let prepared = storage
            .prepare_draft_composer_materialization_step(&store, key)
            .unwrap()
            .unwrap();
        let phase_bit = match prepared.next_phase() {
            Some(DraftComposerBuildPhaseV1::Planning) => 1,
            Some(DraftComposerBuildPhaseV1::Writing) => 2,
            Some(DraftComposerBuildPhaseV1::Draining { .. }) => 4,
            Some(DraftComposerBuildPhaseV1::ReadyToSeal) => 8,
            None => 16,
        };
        assert!(prepared.input_payload_bytes() <= DRAFT_COMPOSER_INPUT_MAX_BYTES);
        assert!(prepared.written_record_count() <= DRAFT_COMPOSER_WRITE_MAX_RECORDS);
        assert!(prepared.resident_bytes() <= DRAFT_COMPOSER_RESIDENT_MAX_BYTES);
        committed(execute(
            &store,
            storage.advance_draft_composer_materialization(
                storage.revision(&store).unwrap(),
                prepared,
            ),
        ));
        steps += 1;
        if reopened_phases & phase_bit == 0 {
            reopened_phases |= phase_bit;
            drop(store);
            store =
                HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
            storage = SyndicStorage::register(&mut store).unwrap();
        }
        assert!(steps < 4096);
    };
    assert!(steps > 8);
    let chunks = storage
        .content_chunks(
            &store,
            mapping.content().id(),
            None,
            CursorReadLimits::new(4, 131_072).unwrap(),
        )
        .unwrap();
    assert_eq!(chunks.records().len(), 2);
    assert_eq!(chunks.records()[0].bytes().len(), 64_512);
    assert_eq!(chunks.records()[1].bytes().len(), 1_027);
    assert!(chunks.records()[1].bytes().ends_with("💎z".as_bytes()));
    assert_eq!(mapping.content().summary().encoded_bytes(), 65_539);
    assert_eq!(mapping.content().summary().logical_utf8_bytes(), 64_342);
    assert_eq!(reopened_phases, 31);
}

#[cfg(feature = "test-faults")]
#[test]
fn corrupt_build_mapping_manifest_and_output_are_rejected() {
    let names = syndic_v5_family_names();
    assert_eq!(names.len(), 80);
    assert_eq!(names[10], "draft-marker-order-commitments");
    assert_eq!(names[11], "draft-marker-seals");
    assert_eq!(names[14], "draft-piece-build-progress");
    assert_eq!(names[16], "draft-editor-candidate-sessions");
    assert_eq!(names[17], "draft-mutation-staging-heads");
    assert_eq!(names[18], "draft-mutation-staging-pages");
    assert_eq!(names[19], "draft-mutation-staging-progress");
    assert_eq!(names[20], "draft-edit-history-frontiers");
    assert_eq!(names[21], "draft-edit-history-transitions");
    assert_eq!(names[22], "draft-historical-root-adoptions");
    assert_eq!(names[23], "draft-composer-builds");
    assert_eq!(names[24], "draft-composer-materializations");

    for (name, corruption) in [
        ("build-cursor", DraftComposerBuildCorruption::Cursor),
        (
            "build-carry-frontier",
            DraftComposerBuildCorruption::CarryFrontier,
        ),
        (
            "build-marker-frontier",
            DraftComposerBuildCorruption::MarkerFrontier,
        ),
        (
            "build-planning-source-overflow",
            DraftComposerBuildCorruption::PlanningSourceOverflow,
        ),
        (
            "build-planning-cursor-mismatch",
            DraftComposerBuildCorruption::PlanningCursorMismatch,
        ),
        (
            "build-planning-eof-mismatch",
            DraftComposerBuildCorruption::PlanningEofMismatch,
        ),
        (
            "build-planning-piece-maximum",
            DraftComposerBuildCorruption::PlanningPieceMaximum,
        ),
        (
            "build-planning-maximum",
            DraftComposerBuildCorruption::PlanningMaximum,
        ),
        (
            "build-planning-digest-count",
            DraftComposerBuildCorruption::PlanningDigestCount,
        ),
        (
            "build-terminal-planning-maximum",
            DraftComposerBuildCorruption::TerminalPlanningMaximum,
        ),
        (
            "build-terminal-frontier",
            DraftComposerBuildCorruption::TerminalFrontier,
        ),
        ("build-summary", DraftComposerBuildCorruption::OutputSummary),
    ] {
        let (_home, store, storage, thread) = fixture(name, 81);
        let planning_corruption = matches!(
            corruption,
            DraftComposerBuildCorruption::PlanningSourceOverflow
                | DraftComposerBuildCorruption::PlanningCursorMismatch
                | DraftComposerBuildCorruption::PlanningEofMismatch
                | DraftComposerBuildCorruption::PlanningPieceMaximum
                | DraftComposerBuildCorruption::PlanningMaximum
                | DraftComposerBuildCorruption::PlanningDigestCount
                | DraftComposerBuildCorruption::TerminalPlanningMaximum
        );
        let root = if planning_corruption {
            let root = replace_empty(
                storage,
                &store,
                thread,
                80,
                vec![
                    DraftPieceV1::Text("first".to_owned()),
                    DraftPieceV1::Text("second".to_owned()),
                ],
            );
            assert_eq!(root.summary().piece_count(), 2);
            root
        } else {
            storage
                .current_draft(&store, thread, SyndicPointReadLimit::new(65_536).unwrap())
                .unwrap()
                .unwrap()
                .draft()
                .piece_root()
        };
        let key = materialization_key(root, 82);
        committed(execute(
            &store,
            storage.begin_draft_composer_materialization(storage.revision(&store).unwrap(), key),
        ));
        let DraftComposerMaterializationStatusV1::Building(_) = storage
            .draft_composer_materialization_status(&store, key)
            .unwrap()
        else {
            panic!("fixture build is not open")
        };
        assert!(draft_composer_build_truncation_is_rejected(
            &store, storage, key
        ));
        if matches!(
            corruption,
            DraftComposerBuildCorruption::CarryFrontier
                | DraftComposerBuildCorruption::OutputSummary
        ) {
            let prepared = storage
                .prepare_draft_composer_materialization_step(&store, key)
                .unwrap()
                .unwrap();
            committed(execute(
                &store,
                storage.advance_draft_composer_materialization(
                    storage.revision(&store).unwrap(),
                    prepared,
                ),
            ));
        }
        committed(execute(
            &store,
            inject_draft_composer_build_corruption(&store, storage, key, corruption),
        ));
        assert!(
            storage
                .draft_composer_materialization_status(&store, key)
                .is_err()
        );
    }

    let (_home, store, storage, thread) = fixture("mapping", 83);
    let root = replace_empty(
        storage,
        &store,
        thread,
        84,
        vec![DraftPieceV1::Text("sealed".to_owned())],
    );
    let key = materialization_key(root, 85);
    let mapping = materialize(storage, &store, key);
    assert!(draft_composer_mapping_truncation_is_rejected(&mapping));
    committed(execute(
        &store,
        inject_draft_composer_mapping_corruption(&store, storage, mapping.key()),
    ));
    assert!(
        storage
            .draft_composer_materialization_status(&store, key)
            .is_err()
    );

    let (_home, store, storage, thread) = fixture("manifest", 86);
    let root = replace_empty(
        storage,
        &store,
        thread,
        87,
        vec![DraftPieceV1::Text("sealed".to_owned())],
    );
    let key = materialization_key(root, 88);
    let mapping = materialize(storage, &store, key);
    committed(execute(
        &store,
        inject_draft_composer_manifest_corruption(&store, storage, mapping.content().id()),
    ));
    assert!(
        storage
            .draft_composer_materialization_status(&store, key)
            .is_err()
    );

    let (_home, store, storage, thread) = fixture("chunk", 89);
    let root = replace_empty(
        storage,
        &store,
        thread,
        90,
        vec![DraftPieceV1::Text("sealed text".to_owned())],
    );
    let mapping = materialize(storage, &store, materialization_key(root, 91));
    committed(execute(
        &store,
        inject_draft_composer_chunk_corruption(&store, storage, mapping.content().id()),
    ));
    assert!(
        storage
            .sealed_content_text_range(&store, mapping.content(), 0, 65_536)
            .is_err()
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn corrupted_authenticated_source_is_rejected_before_output_publication() {
    let (_home, store, storage, thread) = fixture("source-corruption", 96);
    let root = replace_empty(
        storage,
        &store,
        thread,
        97,
        (0..130)
            .map(|_| DraftPieceV1::Text("x".to_owned()))
            .collect(),
    );
    assert!(root.summary().height() >= 2);
    committed(execute(
        &store,
        inject_draft_piece_descendant_corruption(
            &store,
            storage,
            root,
            DraftPieceDescendantTarget::Sequence,
            DraftPieceDescendantCorruption::Digest,
        ),
    ));
    let key = materialization_key(root, 98);
    committed(execute(
        &store,
        storage.begin_draft_composer_materialization(storage.revision(&store).unwrap(), key),
    ));
    assert!(
        storage
            .prepare_draft_composer_materialization_step(&store, key)
            .is_err()
    );
    assert!(
        storage
            .draft_composer_materialization_status(&store, key)
            .is_err()
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn indeterminate_writer_custody_resumes_from_each_committed_record() {
    let home = TestHome::new("custody");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([92; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([93; 16]),
                execution(),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let root = replace_empty(
        storage,
        &store,
        thread,
        94,
        vec![DraftPieceV1::Text("custody".to_owned())],
    );
    let key = materialization_key(root, 95);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        execute(
            &store,
            storage.begin_draft_composer_materialization(storage.revision(&store).unwrap(), key),
        ),
        CommandOutcome::Indeterminate { .. }
    ));
    for _ in 0..32 {
        if matches!(
            storage
                .draft_composer_materialization_status(&store, key)
                .unwrap(),
            DraftComposerMaterializationStatusV1::Sealed(_)
        ) {
            return;
        }
        let prepared = storage
            .prepare_draft_composer_materialization_step(&store, key)
            .unwrap()
            .unwrap();
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        assert!(matches!(
            execute(
                &store,
                storage.advance_draft_composer_materialization(
                    storage.revision(&store).unwrap(),
                    prepared,
                ),
            ),
            CommandOutcome::Indeterminate { .. }
        ));
    }
    panic!("indeterminate materializer did not converge from durable records")
}

#[cfg(feature = "test-faults")]
#[test]
fn ownerless_partial_output_is_unreachable_until_atomic_seal() {
    let (_home, store, storage, thread) = fixture("preseal-unreachable", 101);
    let root = replace_empty(
        storage,
        &store,
        thread,
        102,
        vec![DraftPieceV1::Text("private until seal".to_owned())],
    );
    let cancelled = materialization_key(root, 103);
    let cancelled_output = stage_partial_output(storage, &store, cancelled, true);
    assert_public_content_unavailable(storage, &store, cancelled_output);
    committed(execute(
        &store,
        storage.cancel_draft_composer_materialization(storage.revision(&store).unwrap(), cancelled),
    ));
    assert_eq!(
        storage
            .draft_composer_materialization_status(&store, cancelled)
            .unwrap(),
        DraftComposerMaterializationStatusV1::Cancelled,
    );
    assert_public_content_unavailable(storage, &store, cancelled_output);

    let failed_root = apply_replacement(
        storage,
        &store,
        thread,
        104,
        DraftPieceReplacementV1::new(
            point(18),
            point(18),
            vec![DraftPieceV1::Text("x".to_owned())],
        ),
    );
    let failed = materialization_key(failed_root, 105);
    let failed_output = stage_partial_output(storage, &store, failed, false);
    assert_public_content_unavailable(storage, &store, failed_output);
    committed(execute(
        &store,
        storage.fail_draft_composer_materialization(storage.revision(&store).unwrap(), failed),
    ));
    assert!(matches!(
        storage
            .draft_composer_materialization_status(&store, failed)
            .unwrap(),
        DraftComposerMaterializationStatusV1::Failed(_)
    ));
    assert_public_content_unavailable(storage, &store, failed_output);

    let sealed_root = apply_replacement(
        storage,
        &store,
        thread,
        106,
        DraftPieceReplacementV1::new(
            point(19),
            point(19),
            vec![DraftPieceV1::Text("y".to_owned())],
        ),
    );
    let sealed = materialize(storage, &store, materialization_key(sealed_root, 107));
    assert_eq!(
        storage
            .content_manifest(
                &store,
                sealed.content().id(),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .unwrap()
            .unwrap()
            .sealed_reference(),
        Some(sealed.content()),
    );
    assert_eq!(
        storage
            .sealed_content_text_range(&store, sealed.content(), 0, 65_536)
            .unwrap()
            .unwrap()
            .text(),
        "private until sealxy",
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn advancing_step_has_not_committed_and_indeterminate_custody() {
    let home = TestHome::new("step-custody");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([106; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([107; 16]),
                execution(),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let root = replace_empty(
        storage,
        &store,
        thread,
        108,
        vec![DraftPieceV1::Text("custody transition".to_owned())],
    );
    let key = materialization_key(root, 109);
    committed(execute(
        &store,
        storage.begin_draft_composer_materialization(storage.revision(&store).unwrap(), key),
    ));
    let prepared = storage
        .prepare_draft_composer_materialization_step(&store, key)
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert!(matches!(
        execute(
            &store,
            storage.advance_draft_composer_materialization(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    let recovery = store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    committed(execute(
        &store,
        storage.advance_draft_composer_materialization(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    ));
    assert!(matches!(
        execute(
            &store,
            storage.advance_draft_composer_materialization(
                storage.revision(&store).unwrap(),
                prepared,
            ),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    let next = storage
        .prepare_draft_composer_materialization_step(&store, key)
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        execute(
            &store,
            storage
                .advance_draft_composer_materialization(storage.revision(&store).unwrap(), next,),
        ),
        CommandOutcome::Indeterminate { .. }
    ));
    let _ = materialize_existing(storage, &store, key);
}

#[cfg(feature = "test-faults")]
#[test]
fn output_natural_key_accepts_exact_replay_and_rejects_different_bytes() {
    for (name, seed, collision) in [
        ("natural-replay", 180, false),
        ("natural-collision", 190, true),
    ] {
        let (_home, store, storage, thread) = fixture(name, seed);
        let root = replace_empty(
            storage,
            &store,
            thread,
            seed + 1,
            vec![DraftPieceV1::Text("natural key bytes".to_owned())],
        );
        let key = materialization_key(root, seed + 2);
        committed(execute(
            &store,
            storage.begin_draft_composer_materialization(storage.revision(&store).unwrap(), key),
        ));
        let prepared = loop {
            let prepared = storage
                .prepare_draft_composer_materialization_step(&store, key)
                .unwrap()
                .unwrap();
            if matches!(
                prepared.next_phase(),
                Some(DraftComposerBuildPhaseV1::Draining { .. })
            ) {
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
        committed(execute(
            &store,
            inject_draft_composer_prepared_chunk(&store, storage, &prepared, collision),
        ));
        let outcome = execute(
            &store,
            storage.advance_draft_composer_materialization(
                storage.revision(&store).unwrap(),
                prepared,
            ),
        );
        if collision {
            assert!(matches!(outcome, CommandOutcome::NotCommitted { .. }));
        } else {
            committed(outcome);
            let _ = materialize_existing(storage, &store, key);
        }
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn sealed_reuse_requires_source_origin_and_output_closure() {
    for (name, corruption) in [
        ("missing-source", 0_u8),
        ("missing-origin", 1),
        ("corrupt-origin", 2),
        ("corrupt-source", 3),
    ] {
        let (_home, store, storage, thread) = fixture(name, 110 + corruption);
        let root = replace_empty(
            storage,
            &store,
            thread,
            120 + corruption,
            (0..130)
                .map(|_| DraftPieceV1::Text("x".to_owned()))
                .collect(),
        );
        let key = materialization_key(root, 130 + corruption);
        let mapping = materialize(storage, &store, key);
        let contribution = match corruption {
            0 => delete_draft_composer_source(&store, storage, root),
            1 => delete_draft_composer_origin_build(&store, storage, mapping),
            2 => inject_draft_composer_build_corruption(
                &store,
                storage,
                key,
                DraftComposerBuildCorruption::SealedLifecycle,
            ),
            3 => inject_draft_piece_descendant_corruption(
                &store,
                storage,
                root,
                DraftPieceDescendantTarget::Sequence,
                DraftPieceDescendantCorruption::Digest,
            ),
            _ => unreachable!(),
        };
        committed(execute(&store, contribution));
        let reuse = DraftComposerBuildKeyV1::new(
            root,
            DraftComposerFormatV1::ComposerV1,
            DraftComposerMaterializationOperationIdV1::from_bytes([140 + corruption; 16]),
        );
        assert!(
            storage
                .draft_composer_materialization_status(&store, reuse)
                .is_err()
        );
        if corruption == 3 {
            assert!(storage.revision(&store).is_err());
        } else {
            assert!(matches!(
                execute(
                    &store,
                    storage.begin_draft_composer_materialization(
                        storage.revision(&store).unwrap(),
                        reuse,
                    ),
                ),
                CommandOutcome::NotCommitted { .. }
            ));
        }
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn preseal_chunk_and_indexes_fail_closed() {
    for (index, corruption) in [
        DraftComposerOutputCorruption::Chunk,
        DraftComposerOutputCorruption::ByteSpan,
        DraftComposerOutputCorruption::TextSpan,
        DraftComposerOutputCorruption::Piece,
    ]
    .into_iter()
    .enumerate()
    {
        let (_home, store, storage, thread) = fixture("preseal-corruption", 150 + index as u8);
        let root = replace_empty(
            storage,
            &store,
            thread,
            160 + index as u8,
            vec![DraftPieceV1::Text("indexed output".to_owned())],
        );
        let key = materialization_key(root, 170 + index as u8);
        let output = stage_partial_output(storage, &store, key, true);
        committed(execute(
            &store,
            inject_draft_composer_output_corruption(&store, storage, output.id(), corruption),
        ));
        assert!(
            storage
                .draft_composer_materialization_status(&store, key)
                .is_err()
        );
        assert!(
            storage
                .prepare_draft_composer_materialization_step(&store, key)
                .is_err()
        );
    }
}

fn materialization_key(
    root: syndic_storage::DraftPieceRootReferenceV1,
    seed: u8,
) -> DraftComposerBuildKeyV1 {
    DraftComposerBuildKeyV1::new(
        root,
        DraftComposerFormatV1::ComposerV1,
        DraftComposerMaterializationOperationIdV1::from_bytes([seed; 16]),
    )
}

fn materialize(
    storage: SyndicStorage,
    store: &HomeStore,
    key: DraftComposerBuildKeyV1,
) -> syndic_storage::DraftComposerMaterializationRecordV1 {
    committed(execute(
        store,
        storage.begin_draft_composer_materialization(storage.revision(store).unwrap(), key),
    ));
    for _ in 0..4096 {
        if let DraftComposerMaterializationStatusV1::Sealed(mapping) = storage
            .draft_composer_materialization_status(store, key)
            .unwrap()
        {
            return mapping;
        }
        let prepared = storage
            .prepare_draft_composer_materialization_step(store, key)
            .unwrap()
            .unwrap();
        assert!(prepared.records_read() <= DRAFT_COMPOSER_READ_MAX_RECORDS);
        assert!(prepared.input_payload_bytes() <= DRAFT_COMPOSER_INPUT_MAX_BYTES);
        assert!(prepared.written_record_count() <= DRAFT_COMPOSER_WRITE_MAX_RECORDS);
        assert!(prepared.resident_bytes() <= DRAFT_COMPOSER_RESIDENT_MAX_BYTES);
        committed(execute(
            store,
            storage
                .advance_draft_composer_materialization(storage.revision(store).unwrap(), prepared),
        ));
    }
    panic!("materializer did not finish bounded work")
}

#[cfg(feature = "test-faults")]
fn materialize_existing(
    storage: SyndicStorage,
    store: &HomeStore,
    key: DraftComposerBuildKeyV1,
) -> syndic_storage::DraftComposerMaterializationRecordV1 {
    for _ in 0..4096 {
        if let DraftComposerMaterializationStatusV1::Sealed(mapping) = storage
            .draft_composer_materialization_status(store, key)
            .unwrap()
        {
            return mapping;
        }
        let prepared = storage
            .prepare_draft_composer_materialization_step(store, key)
            .unwrap()
            .unwrap();
        committed(execute(
            store,
            storage
                .advance_draft_composer_materialization(storage.revision(store).unwrap(), prepared),
        ));
    }
    panic!("existing materializer did not finish bounded work")
}

#[cfg(feature = "test-faults")]
fn stage_partial_output(
    storage: SyndicStorage,
    store: &HomeStore,
    key: DraftComposerBuildKeyV1,
    include_indexes: bool,
) -> syndic_storage::ContentReference {
    committed(execute(
        store,
        storage.begin_draft_composer_materialization(storage.revision(store).unwrap(), key),
    ));
    for _ in 0..32 {
        let phase = match storage
            .draft_composer_materialization_status(store, key)
            .unwrap()
        {
            DraftComposerMaterializationStatusV1::Building(phase) => phase,
            status => panic!("expected partial build, got {status:?}"),
        };
        let reached = if include_indexes {
            phase == DraftComposerBuildPhaseV1::ReadyToSeal
        } else {
            matches!(phase, DraftComposerBuildPhaseV1::Draining { .. })
        };
        if reached {
            return draft_composer_provisional_output(store, storage, key).unwrap();
        }
        let prepared = storage
            .prepare_draft_composer_materialization_step(store, key)
            .unwrap()
            .unwrap();
        committed(execute(
            store,
            storage
                .advance_draft_composer_materialization(storage.revision(store).unwrap(), prepared),
        ));
    }
    panic!("partial output state was not reached")
}

#[cfg(feature = "test-faults")]
fn assert_public_content_unavailable(
    storage: SyndicStorage,
    store: &HomeStore,
    content: syndic_storage::ContentReference,
) {
    assert!(
        storage
            .content_manifest(
                store,
                content.id(),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .is_err()
    );
    let limits = CursorReadLimits::new(8, 65_536).unwrap();
    assert!(
        storage
            .content_chunks(store, content.id(), None, limits)
            .is_err()
    );
    assert!(
        storage
            .content_byte_spans(store, content.id(), None, limits)
            .is_err()
    );
    assert!(
        storage
            .content_text_spans(store, content.id(), None, limits)
            .is_err()
    );
    assert!(
        storage
            .content_pieces(store, content.id(), None, limits)
            .is_err()
    );
    assert!(
        storage
            .sealed_content_text_range(store, content, 0, 65_536)
            .is_err()
    );
}

fn append_text_atom(output: &mut Vec<u8>, text: &str) {
    output.push(0);
    output.extend_from_slice(&(text.len() as u64).to_be_bytes());
    output.extend_from_slice(text.as_bytes());
}

fn append_marker_atom(output: &mut Vec<u8>, marker: DraftPieceMarkerV1) {
    output.push(1);
    output.extend_from_slice(marker.marker_id().as_bytes());
    output.extend_from_slice(&marker.label().get().to_be_bytes());
}

fn replace_empty(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    operation: u8,
    pieces: Vec<DraftPieceV1>,
) -> syndic_storage::DraftPieceRootReferenceV1 {
    apply_replacement(
        storage,
        store,
        thread,
        operation,
        DraftPieceReplacementV1::new(point(0), point(0), pieces),
    )
}

fn append_text(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    operation: u8,
    text: &str,
) -> syndic_storage::DraftPieceRootReferenceV1 {
    append_pieces(
        storage,
        store,
        thread,
        operation,
        vec![DraftPieceV1::Text(text.to_owned())],
    )
}

fn append_pieces(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    operation: u8,
    pieces: Vec<DraftPieceV1>,
) -> syndic_storage::DraftPieceRootReferenceV1 {
    let current = candidate_head(storage, store, thread);
    let end = point(current.newest_root().summary().logical_utf8_bytes());
    apply_replacement(
        storage,
        store,
        thread,
        operation,
        DraftPieceReplacementV1::new(end, end, pieces),
    )
}

fn apply_replacement(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    operation: u8,
    replacement: DraftPieceReplacementV1,
) -> syndic_storage::DraftPieceRootReferenceV1 {
    let current = candidate_head(storage, store, thread);
    let predecessor_positions = fixture_positions(&current);
    let caret = if replacement
        .inserted()
        .iter()
        .all(|piece| matches!(piece, DraftPieceV1::Marker(_)))
    {
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll)
    } else {
        point(0)
    };
    let replacements = vec![replacement];
    let header = DraftPieceEditHeaderV1::new(
        current.draft_id(),
        current.session_id(),
        current.newest_candidate_generation(),
        current.newest_root(),
        current.newest_history(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        predecessor_positions.caret,
        predecessor_positions.selection,
        caret,
        caret,
        1,
        canonical_draft_piece_fragment_chain_v1(&replacements),
    );
    let prepared = storage
        .prepare_draft_piece_edit(store, header, &current)
        .unwrap();
    let fragment = storage
        .prepare_draft_piece_fragment(
            &prepared,
            1,
            canonical_empty_draft_piece_fragment_chain_v1(),
            replacements.into_iter().next().unwrap(),
        )
        .unwrap();
    committed(execute(
        store,
        storage.begin_draft_piece_edit(storage.revision(store).unwrap(), prepared.clone()),
    ));
    committed(execute(
        store,
        storage.stage_draft_piece_fragment(
            storage.revision(store).unwrap(),
            prepared.clone(),
            fragment,
        ),
    ));
    for _ in 0..4096 {
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                store,
                current.draft_id(),
                current.session_id(),
                prepared.header().operation_id(),
            )
            .unwrap()
        else {
            break;
        };
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    let adopted = candidate_head(storage, store, thread);
    if adopted.newest_candidate_generation() == current.newest_candidate_generation() + 1 {
        remember_fixture_positions(
            &adopted,
            FixturePositions {
                caret,
                selection: caret,
            },
        );
    }
    adopted.newest_root()
}

#[derive(Clone, Copy)]
struct FixturePositions {
    caret: DraftCompositePositionV1,
    selection: DraftCompositePositionV1,
}

thread_local! {
    static FIXTURE_POSITIONS: std::cell::RefCell<Vec<(
        syndic_storage::DraftEditHistoryFrontierReferenceV1,
        FixturePositions,
    )>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn fixture_positions(session: &DraftEditorCandidateSessionV1) -> FixturePositions {
    if session.newest_candidate_generation() == 0 {
        return FixturePositions {
            caret: point(0),
            selection: point(0),
        };
    }
    FIXTURE_POSITIONS.with(|positions| {
        positions
            .borrow()
            .iter()
            .rev()
            .find(|(history, _)| *history == session.newest_history())
            .map(|(_, positions)| *positions)
            .expect("fixture must remember the exact positions of an adopted candidate head")
    })
}

fn remember_fixture_positions(
    session: &DraftEditorCandidateSessionV1,
    positions: FixturePositions,
) {
    FIXTURE_POSITIONS.with(|known| {
        known
            .borrow_mut()
            .push((session.newest_history(), positions));
    });
}

fn candidate_head(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
) -> DraftEditorCandidateSessionV1 {
    let durable = storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap();
    let session_id = DraftEditorCandidateSessionIdV1::from_bytes([0xE0; 16]);
    match storage
        .draft_editor_candidate_session(store, durable.draft().id(), session_id)
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        DraftEditorCandidateSessionReadOutcomeV1::Absent => {
            let request = DraftEditorCandidateSessionOpenRequestV1::new(
                DraftEditorCurrentSelectorV1::new(
                    durable.thread().id(),
                    durable.thread().revision(),
                    durable.draft().id(),
                    durable.draft().revision(),
                    durable.draft().piece_root(),
                    durable.draft().history(),
                ),
                session_id,
                DraftPieceOperationIdV1::from_bytes([0xE1; 16]),
            );
            let prepared = storage
                .prepare_open_draft_editor_candidate_session(store, request)
                .unwrap();
            let outcome = execute(
                store,
                storage.open_draft_editor_candidate_session(
                    storage.revision(store).unwrap(),
                    prepared.clone(),
                ),
            );
            match storage
                .reconcile_draft_editor_candidate_session_open(store, &prepared, outcome)
                .unwrap()
            {
                DraftEditorCandidateSessionOpenOutcomeV1::Opened(head)
                | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) => head,
                other => panic!("candidate session did not open: {other:?}"),
            }
        }
        other => panic!("candidate session is unavailable: {other:?}"),
    }
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}

fn execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([171; 16]),
        RootId::from_bytes([172; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-phase137",
        )
        .unwrap(),
    )
}

fn fixture(name: &str, seed: u8) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let creation = CreateThread::ordinary(
        thread,
        SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
        execution(),
        SyndicTimestamp::from_unix_millis(1),
        syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
    );
    committed(execute(
        &store,
        storage.create_thread(storage.revision(&store).unwrap(), creation),
    ));
    (home, store, storage, thread)
}

fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn committed(outcome: CommandOutcome) {
    match outcome {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        other => panic!("expected committed command, got {other:?}"),
    }
}

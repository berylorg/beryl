use super::*;

#[path = "reuse/custody.rs"]
mod custody;

fn matching_root(
    storage: &SyndicStorage,
    store: &HomeStore,
    seed: u8,
) -> syndic_storage::DraftPieceRootReferenceV1 {
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    committed(execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([seed; 16]),
                execution(),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    replace_empty(
        storage,
        store,
        thread,
        seed,
        vec![DraftPieceV1::Text("a".repeat(32_000))],
    );
    append_text(storage, store, thread, seed + 1, &"💎".repeat(8_000));
    append_text(storage, store, thread, seed + 2, &"z".repeat(2_000))
}

fn begin(storage: &SyndicStorage, store: &HomeStore, key: DraftComposerBuildKeyV1) {
    committed(execute(
        store,
        storage.begin_draft_composer_materialization(storage.revision(store).unwrap(), key),
    ));
}

fn step(storage: &SyndicStorage, store: &HomeStore, key: DraftComposerBuildKeyV1) {
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
        storage.advance_draft_composer_materialization(storage.revision(store).unwrap(), prepared),
    ));
}

fn reach(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: DraftComposerBuildKeyV1,
    phase: DraftComposerBuildPhaseV1,
) {
    for _ in 0..128 {
        if storage
            .draft_composer_materialization_status(store, key)
            .unwrap()
            == DraftComposerMaterializationStatusV1::Building(phase)
        {
            return;
        }
        step(storage, store, key);
    }
    panic!("did not reach {phase:?}");
}

#[test]
fn distinct_roots_replay_sealed_content_and_keep_the_sealed_reference() {
    let (_home, store, storage, _) = fixture("sealed-content-reuse", 10);
    let first = materialization_key(matching_root(&storage, &store, 20), 21);
    let second = materialization_key(matching_root(&storage, &store, 30), 31);
    let original = materialize(&storage, &store, first);
    let chunks = storage
        .content_chunks(
            &store,
            original.content().id(),
            None,
            CursorReadLimits::new(4, 131_072).unwrap(),
        )
        .unwrap();
    let reused = materialize(&storage, &store, second);
    assert_eq!(original.content(), reused.content());
    assert_ne!(original, reused);
    assert_eq!(
        chunks.records(),
        storage
            .content_chunks(
                &store,
                reused.content().id(),
                None,
                CursorReadLimits::new(4, 131_072).unwrap()
            )
            .unwrap()
            .records()
    );
    assert_eq!(
        DraftComposerMaterializationStatusV1::Sealed(original),
        storage
            .draft_composer_materialization_status(&store, materialization_key(first.source(), 22))
            .unwrap()
    );
}

#[test]
fn interleaved_roots_replay_a_shared_prefix_and_converge() {
    let (_home, store, storage, _) = fixture("interleaved-content-reuse", 10);
    let first = materialization_key(matching_root(&storage, &store, 20), 21);
    let second = materialization_key(matching_root(&storage, &store, 30), 31);
    stage_partial_output(&storage, &store, first, false);
    begin(&storage, &store, second);
    reach(
        &storage,
        &store,
        second,
        DraftComposerBuildPhaseV1::Draining { final_chunk: false },
    );
    for _ in 0..128 {
        let mut complete = true;
        for key in [first, second] {
            if !matches!(
                storage
                    .draft_composer_materialization_status(&store, key)
                    .unwrap(),
                DraftComposerMaterializationStatusV1::Sealed(_)
            ) {
                complete = false;
                step(&storage, &store, key);
            }
        }
        if complete {
            assert_eq!(
                materialize_existing(&storage, &store, first).content(),
                materialize_existing(&storage, &store, second).content()
            );
            return;
        }
    }
    panic!("interleaved builds did not finish");
}

#[test]
fn abandoned_prefix_is_reused_after_physical_reopen() {
    for terminal in 0..3 {
        let (home, store, storage, _) = fixture("abandoned-content-reuse", 10);
        let first = materialization_key(matching_root(&storage, &store, 20), 21);
        let second = materialization_key(matching_root(&storage, &store, 30), 31);
        let partial = stage_partial_output(&storage, &store, first, false);
        let revision = storage.revision(&store).unwrap();
        let contribution = match terminal {
            0 => storage.cancel_draft_composer_materialization(revision, first),
            1 => storage.fail_draft_composer_materialization(revision, first),
            _ => storage.supersede_draft_composer_materialization(
                revision,
                first,
                DraftComposerMaterializationOperationIdV1::from_bytes([22; 16]),
            ),
        };
        committed(execute(&store, contribution));
        drop(storage);
        drop(store);
        let mut candidate = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
            home.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let reused = materialize(&storage, &store, second);
        assert_eq!(partial.id(), reused.content().id());
        assert!(!matches!(
            storage
                .draft_composer_materialization_status(&store, first)
                .unwrap(),
            DraftComposerMaterializationStatusV1::Sealed(_)
        ));
    }
}

#[test]
fn stale_replay_and_drain_steps_refuse_a_changed_shared_manifest() {
    for phase in [
        DraftComposerBuildPhaseV1::Writing,
        DraftComposerBuildPhaseV1::Draining { final_chunk: false },
        DraftComposerBuildPhaseV1::ReadyToSeal,
    ] {
        let (_home, store, storage, _) = fixture("stale-content-reuse", 10);
        let first = materialization_key(matching_root(&storage, &store, 20), 21);
        let second = materialization_key(matching_root(&storage, &store, 30), 31);
        stage_partial_output(&storage, &store, first, true);
        begin(&storage, &store, second);
        reach(&storage, &store, second, phase);
        let before = storage
            .draft_composer_materialization_status(&store, second)
            .unwrap();
        let stale = storage
            .prepare_draft_composer_materialization_step(&store, second)
            .unwrap()
            .unwrap();
        let original = materialize_existing(&storage, &store, first);
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
        assert_eq!(
            before,
            storage
                .draft_composer_materialization_status(&store, second)
                .unwrap()
        );
        assert_eq!(
            original.content(),
            materialize_existing(&storage, &store, second).content()
        );
    }
}

#[test]
fn sealed_reuse_refuses_damaged_output_without_repair_or_mapping() {
    for corruption in [
        DraftComposerOutputCorruption::Chunk,
        DraftComposerOutputCorruption::ByteSpan,
        DraftComposerOutputCorruption::TextSpan,
        DraftComposerOutputCorruption::Piece,
    ] {
        let (_home, store, storage, _) = fixture("damaged-sealed-reuse", 10);
        let first = materialization_key(matching_root(&storage, &store, 20), 21);
        let second = materialization_key(matching_root(&storage, &store, 30), 31);
        let original = materialize(&storage, &store, first);
        committed(execute(
            &store,
            inject_draft_composer_output_corruption(
                &store,
                storage.clone(),
                original.content().id(),
                corruption,
            ),
        ));
        begin(&storage, &store, second);
        let mut refused = false;
        for _ in 0..128 {
            match storage.prepare_draft_composer_materialization_step(&store, second) {
                Err(_) => {
                    refused = true;
                    break;
                }
                Ok(Some(prepared)) => {
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
                Ok(None) => panic!("damaged output was accepted: {corruption:?}"),
            }
        }
        assert!(refused, "damaged output was not refused: {corruption:?}");
        assert!(!matches!(
            storage.draft_composer_materialization_status(&store, second),
            Ok(DraftComposerMaterializationStatusV1::Sealed(_))
        ));
    }
}

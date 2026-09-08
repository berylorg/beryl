use super::*;
use syndic_storage::DraftPieceBuildFrontierV1;

#[cfg(feature = "test-faults")]
#[path = "continuation_integrity.rs"]
mod continuation_integrity;

#[test]
fn ordinary_range_repair_crossing_a_leaf_boundary_removes_one_leaf_per_applying_command() {
    let (_home, store, storage, thread) = fixture("ordinary-range-quantum", 160);
    let seed = transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        161,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("a".to_owned()); 129],
        )],
        point(129),
    );
    run_transaction(&storage, &store, &seed, 162);
    let base = current(&storage, &store, thread);
    assert!(base.draft().piece_root().summary().height() >= 2);

    let edit = transaction(
        &storage,
        &store,
        &base,
        163,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(129),
            vec![DraftPieceV1::Text("b".to_owned())],
        )],
        point(1),
    );
    begin_and_stage(&storage, &store, &edit);

    let mut applying_steps = 0;
    let mut previous_end: Option<syndic_storage::DraftPieceBuildBoundaryV1> = None;
    for step in 0..512 {
        let before = open_build(&storage, &store, &edit);
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                &store,
                base.draft().id(),
                edit.session,
                edit.operation,
            )
            .unwrap_or_else(|error| panic!("preparation at step {step} failed: {error:?}"))
        else {
            break;
        };
        let before_bytes = before
            .working_roots()
            .sequence_summary()
            .logical_utf8_bytes();
        let applying = match before.frontier() {
            DraftPieceBuildFrontierV1::Applying {
                successor_start,
                successor_end,
                ..
            } if successor_start != successor_end => Some((successor_start, successor_end)),
            _ => None,
        };
        if let Some((start, end)) = applying {
            let work = advance
                .bounded_work()
                .expect("ordinary Applying command must expose its bounded work");
            assert!(work.point_attempts() <= 512);
            assert!(work.stored_structure_records() <= 256);
            assert!(work.encoded_bytes() <= 4_194_304);
            assert_eq!(start.inner(), 0);
            assert_eq!(end.inner(), 0);
            if let Some(previous_end) = previous_end {
                assert_eq!(end.rank() + 1, previous_end.rank());
            }
            previous_end = Some(end);
            applying_steps += 1;
        } else if !matches!(
            before.frontier(),
            DraftPieceBuildFrontierV1::Applying { .. }
        ) {
            assert!(advance.bounded_work().is_none());
        }
        let measured = advance.clone();
        let prepared_work = measured.bounded_work();
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        if let Some(work) = measured.bounded_work() {
            assert!(work.point_attempts() > prepared_work.unwrap().point_attempts());
            assert!(work.encoded_bytes() > prepared_work.unwrap().encoded_bytes());
            assert!(work.point_attempts() <= 512);
            assert!(work.stored_structure_records() <= 256);
            assert!(work.encoded_bytes() <= 4_194_304);
            assert!(work.peak_encoded_bytes() <= 4_194_304);
        }
        if applying.is_some() {
            let after = open_build(&storage, &store, &edit);
            assert_eq!(
                after
                    .working_roots()
                    .sequence_summary()
                    .logical_utf8_bytes(),
                before_bytes - 1,
            );
        }
    }
    assert_eq!(applying_steps, 129);
    let complete = open_build(&storage, &store, &edit);
    assert_eq!(complete.frontier(), DraftPieceBuildFrontierV1::Complete);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    remember_settled_transaction(&storage, &store, &edit);
    let root = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(root.summary().piece_count(), 1);
    assert_eq!(root.summary().height(), 1);
    assert!(root.root_node().is_some());
    assert_eq!(
        storage
            .draft_piece_text_demand(&store, root, DraftPieceTextDemandV1::Forward(0), 8)
            .unwrap()
            .bytes(),
        b"b"
    );
}

#[test]
fn same_leaf_utf8_range_preserves_one_leaf_and_empty_range_enters_inserting() {
    let (_home, store, storage, thread) = fixture("ordinary-range-utf8", 170);
    let initial = transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        171,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("αβγ".to_owned())],
        )],
        point(6),
    );
    run_transaction(&storage, &store, &initial, 172);
    let base = current(&storage, &store, thread);
    let middle = transaction(
        &storage,
        &store,
        &base,
        173,
        vec![DraftPieceReplacementV1::new(
            point(2),
            point(4),
            vec![DraftPieceV1::Text("X".to_owned())],
        )],
        point(3),
    );
    begin_and_stage(&storage, &store, &middle);
    let mut saw_same_leaf = false;
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            &store,
            base.draft().id(),
            middle.session,
            middle.operation,
        )
        .unwrap()
    {
        if let DraftPieceBuildFrontierV1::Applying {
            successor_start,
            successor_end,
            ..
        } = open_build(&storage, &store, &middle).frontier()
        {
            assert_eq!(successor_start.rank(), successor_end.rank());
            assert_eq!(successor_start.inner(), 2);
            assert_eq!(successor_end.inner(), if saw_same_leaf { 2 } else { 4 });
            if saw_same_leaf {
                assert_eq!(
                    open_build(&storage, &store, &middle)
                        .working_roots()
                        .sequence_summary()
                        .piece_count(),
                    1
                );
            }
            assert!(advance.bounded_work().is_some());
            saw_same_leaf = true;
        }
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
    }
    assert!(saw_same_leaf);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), middle.prepared.clone()),
    ));
    remember_settled_transaction(&storage, &store, &middle);
    let root = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(root.summary().piece_count(), 3);
    assert_eq!(
        storage
            .draft_piece_text_demand(&store, root, DraftPieceTextDemandV1::Forward(0), 16)
            .unwrap()
            .bytes(),
        "αXγ".as_bytes()
    );

    let base = current(&storage, &store, thread);
    let empty = transaction(
        &storage,
        &store,
        &base,
        174,
        vec![DraftPieceReplacementV1::new(
            point(2),
            point(2),
            vec![DraftPieceV1::Text("Z".to_owned())],
        )],
        point(3),
    );
    begin_and_stage(&storage, &store, &empty);
    let mut applying_steps = 0;
    advance_until_complete_observing(&storage, &store, &empty, &mut applying_steps);
    assert_eq!(applying_steps, 1);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), empty.prepared.clone()),
    ));
    remember_settled_transaction(&storage, &store, &empty);
    let root = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(
        storage
            .draft_piece_text_demand(&store, root, DraftPieceTextDemandV1::Forward(0), 16)
            .unwrap()
            .bytes(),
        "αZXγ".as_bytes()
    );
}

#[test]
fn later_leaf_end_prefix_trim_normalizes_the_remaining_end_cursor() {
    let (_home, store, storage, thread) = fixture("ordinary-range-end-prefix", 175);
    let seed = transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        176,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![
                DraftPieceV1::Text("α".to_owned()),
                DraftPieceV1::Text("β".to_owned()),
                DraftPieceV1::Text("γγ".to_owned()),
            ],
        )],
        point(8),
    );
    run_transaction(&storage, &store, &seed, 177);
    let base = current(&storage, &store, thread);
    let edit = transaction(
        &storage,
        &store,
        &base,
        178,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(6),
            vec![DraftPieceV1::Text("X".to_owned())],
        )],
        point(1),
    );
    begin_and_stage(&storage, &store, &edit);

    let mut observed = false;
    for step in 0..16 {
        let before = open_build(&storage, &store, &edit);
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                base.draft().id(),
                edit.session,
                edit.operation,
            )
            .unwrap_or_else(|error| panic!("preparation at step {step} failed: {error:?}"))
            .unwrap();
        if let DraftPieceBuildFrontierV1::Applying {
            successor_start,
            successor_end,
            ..
        } = before.frontier()
            && successor_end.rank() > successor_start.rank()
            && successor_end.inner() > 0
        {
            assert!(advance.bounded_work().is_some());
            committed(execute(&store, storage.advance_draft_piece_edit(advance)));
            match open_build(&storage, &store, &edit).frontier() {
                DraftPieceBuildFrontierV1::Applying {
                    successor_start: next_start,
                    successor_end: next_end,
                    ..
                } => {
                    assert_eq!(next_start, successor_start);
                    assert_eq!(next_end.rank(), successor_end.rank());
                    assert_eq!(next_end.inner(), 0);
                }
                other => panic!("end-prefix trim did not retain an Applying cursor: {other:?}"),
            }
            observed = true;
            break;
        }
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
    }
    assert!(observed);
    advance_until_complete(&storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    remember_settled_transaction(&storage, &store, &edit);
    let root = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(
        storage
            .draft_piece_text_demand(&store, root, DraftPieceTextDemandV1::Forward(0), 16)
            .unwrap()
            .bytes(),
        "Xγ".as_bytes()
    );
}

#[test]
fn start_leaf_tail_trim_normalizes_an_empty_range_before_inserting() {
    let (_home, store, storage, thread) = fixture("ordinary-range-start-tail", 185);
    let seed = transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        186,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![
                DraftPieceV1::Text("αβ".to_owned()),
                DraftPieceV1::Text("γ".to_owned()),
            ],
        )],
        point(6),
    );
    run_transaction(&storage, &store, &seed, 187);
    let base = current(&storage, &store, thread);
    let edit = transaction(
        &storage,
        &store,
        &base,
        188,
        vec![DraftPieceReplacementV1::new(
            point(2),
            point(4),
            vec![DraftPieceV1::Text("X".to_owned())],
        )],
        point(3),
    );
    begin_and_stage(&storage, &store, &edit);

    let mut observed = false;
    for step in 0..16 {
        let before = open_build(&storage, &store, &edit);
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                base.draft().id(),
                edit.session,
                edit.operation,
            )
            .unwrap_or_else(|error| panic!("preparation at step {step} failed: {error:?}"))
            .unwrap();
        if let DraftPieceBuildFrontierV1::Applying {
            successor_start,
            successor_end,
            ..
        } = before.frontier()
            && successor_end.rank() == successor_start.rank() + 1
            && successor_start.inner() > 0
            && successor_end.inner() == 0
        {
            assert!(advance.bounded_work().is_some());
            committed(execute(&store, storage.advance_draft_piece_edit(advance)));
            match open_build(&storage, &store, &edit).frontier() {
                DraftPieceBuildFrontierV1::Applying {
                    successor_start: next_start,
                    successor_end: next_end,
                    ..
                } => {
                    assert_eq!(next_start, next_end);
                    assert_eq!(next_end.rank(), successor_start.rank() + 1);
                    assert_eq!(next_end.inner(), 0);
                }
                other => panic!("start-tail trim did not empty Applying range: {other:?}"),
            }
            let empty = storage
                .prepare_draft_piece_build_advance(
                    &store,
                    base.draft().id(),
                    edit.session,
                    edit.operation,
                )
                .unwrap()
                .unwrap();
            assert!(empty.bounded_work().is_some());
            committed(execute(&store, storage.advance_draft_piece_edit(empty)));
            assert!(matches!(
                open_build(&storage, &store, &edit).frontier(),
                DraftPieceBuildFrontierV1::Inserting {
                    next_piece: 0,
                    next_byte: 0,
                    ..
                }
            ));
            observed = true;
            break;
        }
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
    }
    assert!(observed);
    advance_until_complete(&storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    remember_settled_transaction(&storage, &store, &edit);
    let root = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(
        storage
            .draft_piece_text_demand(&store, root, DraftPieceTextDemandV1::Forward(0), 16)
            .unwrap()
            .bytes(),
        "αXγ".as_bytes()
    );
}

#[test]
fn captured_domain_revision_rejects_an_applying_command_after_another_syndic_write() {
    let (_home, store, storage, thread) = fixture("ordinary-range-stale-domain", 190);
    let seed = transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        191,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".to_owned())],
        )],
        point(3),
    );
    run_transaction(&storage, &store, &seed, 192);
    let base = current(&storage, &store, thread);
    let edit = transaction(
        &storage,
        &store,
        &base,
        196,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(3),
            vec![DraftPieceV1::Text("X".to_owned())],
        )],
        point(1),
    );
    begin_and_stage(&storage, &store, &edit);

    let stale = loop {
        let before = open_build(&storage, &store, &edit);
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                base.draft().id(),
                edit.session,
                edit.operation,
            )
            .unwrap()
            .unwrap();
        if matches!(
            before.frontier(),
            DraftPieceBuildFrontierV1::Applying { .. }
        ) {
            assert!(advance.bounded_work().is_some());
            break advance;
        }
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
    };
    let other_thread = SyndicThreadId::from_bytes([194; 16]);
    let other_draft = SyndicDraftId::from_bytes([195; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                other_thread,
                other_draft,
                execution(),
                SyndicTimestamp::from_unix_millis(2),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    assert!(matches!(
        execute(&store, storage.advance_draft_piece_edit(stale)),
        CommandOutcome::NotCommitted { .. }
    ));
    advance_until_complete(&storage, &store, &edit);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    remember_settled_transaction(&storage, &store, &edit);
    let root = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(
        storage
            .draft_piece_text_demand(&store, root, DraftPieceTextDemandV1::Forward(0), 8)
            .unwrap()
            .bytes(),
        b"X"
    );
}

#[test]
fn partial_ordinary_range_repair_reopens_and_continues_from_persisted_progress() {
    let (home, store, storage, thread) = fixture("ordinary-range-reopen", 180);
    let seed = transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        181,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("a".to_owned()); 129],
        )],
        point(129),
    );
    run_transaction(&storage, &store, &seed, 182);
    let base = current(&storage, &store, thread);
    let edit = transaction(
        &storage,
        &store,
        &base,
        183,
        vec![DraftPieceReplacementV1::new(
            point(1),
            point(128),
            vec![DraftPieceV1::Text("b".to_owned())],
        )],
        point(2),
    );
    begin_and_stage(&storage, &store, &edit);

    let mut applied = 0;
    while applied < 3 {
        let before = open_build(&storage, &store, &edit);
        let advance = storage
            .prepare_draft_piece_build_advance(
                &store,
                base.draft().id(),
                edit.session,
                edit.operation,
            )
            .unwrap()
            .unwrap();
        let applying = matches!(
            before.frontier(),
            DraftPieceBuildFrontierV1::Applying { .. }
        );
        committed(execute(&store, storage.advance_draft_piece_edit(advance)));
        if applying {
            applied += 1;
        }
    }
    let persisted = open_build(&storage, &store, &edit);
    drop(store);

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(open_build(&storage, &reopened, &edit), persisted);
    advance_until_complete_for(
        &storage,
        &reopened,
        base.draft().id(),
        edit.session,
        edit.operation,
    );
    assert_eq!(
        open_build(&storage, &reopened, &edit).frontier(),
        DraftPieceBuildFrontierV1::Complete
    );
    committed(execute(
        &reopened,
        storage
            .settle_draft_piece_edit(storage.revision(&reopened).unwrap(), edit.prepared.clone()),
    ));
    remember_settled_transaction(&storage, &reopened, &edit);
    let root = current(&storage, &reopened, thread).draft().piece_root();
    assert_eq!(
        storage
            .draft_piece_text_demand(&reopened, root, DraftPieceTextDemandV1::Forward(0), 256)
            .unwrap()
            .bytes(),
        b"aba"
    );
}

fn open_build(
    storage: &SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> syndic_storage::DraftPieceBuildRecordV1 {
    match exact_status(storage, store, transaction) {
        DraftPieceOperationStatusV1::Open(build) | DraftPieceOperationStatusV1::Complete(build) => {
            build
        }
        other => panic!("operation did not retain an open or complete build: {other:?}"),
    }
}

fn advance_until_complete_observing(
    storage: &SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
    applying_steps: &mut usize,
) {
    for step in 0..64 {
        let before = open_build(storage, store, transaction);
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                store,
                transaction.prepared.header().draft_id(),
                transaction.session,
                transaction.operation,
            )
            .unwrap_or_else(|error| panic!("preparation at step {step} failed: {error:?}"))
        else {
            return;
        };
        if matches!(
            before.frontier(),
            DraftPieceBuildFrontierV1::Applying { .. }
        ) {
            *applying_steps += 1;
        }
        committed(execute(store, storage.advance_draft_piece_edit(advance)));
    }
    panic!("ordinary range repair did not complete within a bounded command count")
}

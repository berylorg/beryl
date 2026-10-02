use super::*;
use beryl_home_store::{
    CommandCancellation, TemporaryReadPool, TemporaryReadPoolLimits, TemporaryReadPoolUsage,
};
use syndic_storage::{
    DetachedDraftReadErrorV1, DetachedDraftReadLimitsV1, DetachedDraftReadSourceV1,
};

fn pool() -> TemporaryReadPool {
    TemporaryReadPool::new(TemporaryReadPoolLimits::default())
}
fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}
fn export(
    storage: &SyndicStorage,
    store: &HomeStore,
    current: &CandidateCurrent,
    pool: &TemporaryReadPool,
) -> DetachedDraftReadSourceV1 {
    storage
        .export_detached_draft_read_source(
            store,
            DraftEditorCandidateActivationBindingV1::from_head(&current.session),
            pool,
            DetachedDraftReadLimitsV1::default(),
            &CommandCancellation::new(),
        )
        .unwrap()
}
fn text_result(
    result: &syndic_storage::DraftPieceTextDemandResultV1,
) -> (
    u64,
    u64,
    Vec<u8>,
    DraftPieceTextEdgeFactV1,
    DraftPieceTextEdgeFactV1,
) {
    (
        result.start(),
        result.end(),
        result.bytes().to_vec(),
        result.preceding(),
        result.following(),
    )
}

#[test]
fn detached_unicode_demands_preserve_boundaries_and_remain_readable_after_home_close() {
    let (_home, store, storage, thread) = fixture("detached-unicode", 201);
    let initial = current(&storage, &store, thread);
    let text = "aé中😀\n\r\nz";
    let edit = transaction(
        &storage,
        &store,
        &initial,
        202,
        203,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text(text.into())],
        )],
        point(text.len() as u64),
    );
    run_transaction(&storage, &store, &edit, 2);
    let selected = current(&storage, &store, thread);
    let pool = pool();
    let source = export(&storage, &store, &selected, &pool);
    assert_eq!(
        source.binding(),
        DraftEditorCandidateActivationBindingV1::from_head(&selected.session)
    );
    for coordinate in 0..=text.len() as u64 + 1 {
        for demand in [
            DraftPieceTextDemandV1::Forward(coordinate),
            DraftPieceTextDemandV1::Backward(coordinate),
            DraftPieceTextDemandV1::Validate(coordinate),
        ] {
            for max in [4, 7, 16] {
                let ordinary = storage.draft_piece_text_demand(&store, source.root(), demand, max);
                let detached = source.text_demand(demand, max);
                match (ordinary, detached) {
                    (Ok(a), Ok(b)) => {
                        assert_eq!(text_result(&a), text_result(&b), "{demand:?}, {max}")
                    }
                    (
                        Err(DraftPieceRangeSourceErrorV1::Malformed(a)),
                        Err(DetachedDraftReadErrorV1::Malformed(b)),
                    ) => assert_eq!(a, b),
                    (a, b) => panic!("demand mismatch {demand:?}: {a:?}, {b:?}"),
                }
            }
        }
    }
    store.close().unwrap();
    assert_eq!(
        source
            .text_demand(DraftPieceTextDemandV1::Forward(0), 65_536)
            .unwrap()
            .bytes(),
        text.as_bytes()
    );
    let clone = source.clone();
    drop(source);
    assert_eq!(pool.usage().unwrap().sources, 1);
    assert_eq!(
        clone
            .text_demand(DraftPieceTextDemandV1::Backward(text.len() as u64), 65_536)
            .unwrap()
            .bytes(),
        text.as_bytes()
    );
    drop(clone);
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
}

#[test]
fn detached_empty_and_beyond_page_ranges_release_their_complete_reservation() {
    let (_home, store, storage, thread) = fixture("detached-empty", 204);
    let selected = current(&storage, &store, thread);
    let pool = pool();
    let source = export(&storage, &store, &selected, &pool);
    for demand in [
        DraftPieceTextDemandV1::Forward(0),
        DraftPieceTextDemandV1::Backward(0),
        DraftPieceTextDemandV1::Validate(0),
    ] {
        assert_eq!(
            text_result(
                &storage
                    .draft_piece_text_demand(&store, source.root(), demand, 4)
                    .unwrap()
            ),
            text_result(&source.text_demand(demand, 4).unwrap())
        );
    }
    assert!(
        source
            .marker_demand(DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(0),
                DraftPieceMarkerDirectionV1::Forward,
                None,
                1,
                1024
            ))
            .unwrap()
            .markers()
            .is_empty()
    );
    drop(source);
    let text = "x😀\n".repeat(25_000);
    for (ordinal, bytes) in text.as_bytes().chunks(30_000).enumerate() {
        let selected = current(&storage, &store, thread);
        let at = selected.draft.root.summary().logical_utf8_bytes();
        let edit = transaction(
            &storage,
            &store,
            &selected,
            205,
            220 + ordinal as u8,
            vec![DraftPieceReplacementV1::new(
                point(at),
                point(at),
                vec![DraftPieceV1::Text(
                    std::str::from_utf8(bytes).unwrap().into(),
                )],
            )],
            point(at + bytes.len() as u64),
        );
        run_transaction(&storage, &store, &edit, 2);
    }
    let selected = current(&storage, &store, thread);
    let source = export(&storage, &store, &selected, &pool);
    let page = source
        .text_demand(DraftPieceTextDemandV1::Forward(144_000), 4096)
        .unwrap();
    assert_eq!(
        page.bytes(),
        &text.as_bytes()[page.start() as usize..page.end() as usize]
    );
    store.close().unwrap();
    assert_eq!(
        source
            .text_demand(DraftPieceTextDemandV1::Validate(144_001), 4)
            .unwrap()
            .bytes(),
        "😀".as_bytes()
    );
    drop(source);
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
}

#[test]
fn detached_resource_refusal_and_cancellation_publish_no_source() {
    let (_home, store, storage, thread) = fixture("detached-refusal", 207);
    let selected = current(&storage, &store, thread);
    let binding = DraftEditorCandidateActivationBindingV1::from_head(&selected.session);
    let pool = TemporaryReadPool::new(TemporaryReadPoolLimits::new(31, 1, 65_536).unwrap());
    assert!(matches!(
        storage.export_detached_draft_read_source(
            &store,
            binding,
            &pool,
            DetachedDraftReadLimitsV1::default(),
            &CommandCancellation::new()
        ),
        Err(DetachedDraftReadErrorV1::Backing(_))
    ));
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    let pool = self::pool();
    let cancel = CommandCancellation::new();
    cancel.cancel();
    assert!(matches!(
        storage.export_detached_draft_read_source(
            &store,
            binding,
            &pool,
            DetachedDraftReadLimitsV1::default(),
            &cancel
        ),
        Err(DetachedDraftReadErrorV1::Cancelled)
    ));
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    let edit = transaction(
        &storage,
        &store,
        &selected,
        230,
        231,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".into())],
        )],
        point(3),
    );
    run_transaction(&storage, &store, &edit, 2);
    assert!(matches!(
        storage.export_detached_draft_read_source(
            &store,
            binding,
            &pool,
            DetachedDraftReadLimitsV1::default(),
            &CommandCancellation::new()
        ),
        Err(DetachedDraftReadErrorV1::Source(_))
    ));
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    store.close().unwrap();
}

#[cfg(feature = "test-faults")]
#[test]
fn detached_acquisition_cancellation_and_backing_failures_release_private_files() {
    use beryl_home_store::TemporaryReadFault;
    let (_home, store, storage, thread) = fixture("detached-failed-io", 208);
    let selected = current(&storage, &store, thread);
    let binding = DraftEditorCandidateActivationBindingV1::from_head(&selected.session);
    let pool = pool();
    for fault in [TemporaryReadFault::Create, TemporaryReadFault::Write] {
        pool.test_fail_next(fault);
        assert!(matches!(
            storage.export_detached_draft_read_source(
                &store,
                binding,
                &pool,
                DetachedDraftReadLimitsV1::default(),
                &CommandCancellation::new()
            ),
            Err(DetachedDraftReadErrorV1::Backing(_))
        ));
        assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    }
    let cancellation = CommandCancellation::new();
    let captured = cancellation.clone();
    arm_draft_piece_candidate_read_fault(move |_, _| captured.cancel());
    assert!(matches!(
        storage.export_detached_draft_read_source(
            &store,
            binding,
            &pool,
            DetachedDraftReadLimitsV1::default(),
            &cancellation
        ),
        Err(DetachedDraftReadErrorV1::Cancelled)
    ));
    assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    let source = export(&storage, &store, &selected, &pool);
    drop(source);
    store.close().unwrap();
}

#[cfg(feature = "test-faults")]
fn marker_root(
    storage: &SyndicStorage,
    store: &HomeStore,
    initial: &CandidateCurrent,
    marker_only: bool,
) -> CandidateCurrent {
    let thread = initial.durable.thread().id();
    let after =
        |anchor| DraftCompositePositionV1::new(anchor, DraftCompositeGapWitnessV1::AfterAll);
    for ordinal in 1..=18u8 {
        if marker_only && ordinal > 16 {
            break;
        }
        let selected = current(storage, store, thread);
        let extent = selected.draft.root.summary().logical_utf8_bytes();
        let source = if selected.draft.root.summary().marker_count() != 0 && extent == 0 {
            after(0)
        } else {
            point(extent)
        };
        let (replacement, end) = if ordinal == 17 {
            (
                DraftPieceReplacementV1::new(
                    source,
                    source,
                    vec![DraftPieceV1::Text("é\n".into())],
                ),
                point(3),
            )
        } else {
            let item = marker(209, u64::from(ordinal));
            (
                DraftPieceReplacementV1::new(source, source, vec![DraftPieceV1::Marker(item)])
                    .with_marker_effect(syndic_storage::DraftPieceMarkerEffectV1::Insert(
                        syndic_storage::DraftPieceMarkerInsertionV1::new(
                            extent,
                            item,
                            syndic_storage::DraftPieceMarkerEffectChargesV1::for_marker(item),
                        ),
                    )),
                after(extent),
            )
        };
        let header = DraftPieceEditHeaderV1::new(
            selected.draft.id(),
            selected.session.session_id(),
            selected.session.newest_candidate_generation(),
            selected.draft.root,
            selected.session.newest_history(),
            DraftPieceOperationIdV1::from_bytes([ordinal; 16]),
            selected.positions.caret,
            selected.positions.selection,
            end,
            end,
            1,
            canonical_draft_piece_fragment_chain_v1(std::slice::from_ref(&replacement)),
        );
        let prepared = storage
            .prepare_draft_piece_edit(store, header, &selected.session)
            .unwrap();
        let builder = storage.unadmitted_marker_builder_for_test();
        let fragment = builder
            .prepare_fragment(
                &prepared,
                1,
                canonical_empty_draft_piece_fragment_chain_v1(),
                replacement,
            )
            .unwrap();
        committed(execute(
            store,
            storage.begin_draft_piece_edit(storage.revision(store).unwrap(), prepared.clone()),
        ));
        committed(execute(
            store,
            builder.stage_fragment(storage.revision(store).unwrap(), prepared.clone(), fragment),
        ));
        for _ in 0..4096 {
            let Some(advance) = storage
                .prepare_draft_piece_build_advance(
                    store,
                    selected.draft.id(),
                    selected.session.session_id(),
                    prepared.header().operation_id(),
                )
                .unwrap()
            else {
                break;
            };
            committed(execute(store, storage.advance_draft_piece_edit(advance)));
        }
        committed(execute(
            store,
            storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
        ));
        let DraftEditorCandidateSessionReadOutcomeV1::Active(head) = storage
            .draft_editor_candidate_session(
                store,
                selected.draft.id(),
                selected.session.session_id(),
            )
            .unwrap()
        else {
            panic!("missing marker candidate");
        };
        remember_fixture_positions(
            &head,
            FixturePositions {
                caret: end,
                selection: end,
            },
        );
    }
    current(storage, store, thread)
}
#[cfg(feature = "test-faults")]
#[test]
fn detached_marker_pages_match_dense_same_anchor_direction_cursor_and_terminal_edges() {
    for marker_only in [true, false] {
        let (_home, store, storage, thread) =
            fixture("detached-markers", if marker_only { 211 } else { 213 });
        let initial = current(&storage, &store, thread);
        let selected = marker_root(&storage, &store, &initial, marker_only);
        let pool = pool();
        let source = export(&storage, &store, &selected, &pool);
        let extent = source.root().summary().logical_utf8_bytes();
        for scope in [
            DraftPieceMarkerScopeV1::ExactAnchor(0),
            DraftPieceMarkerScopeV1::ExactAnchor(extent),
            DraftPieceMarkerScopeV1::InclusiveRange {
                start: 0,
                end: extent,
            },
            DraftPieceMarkerScopeV1::Range {
                start: 0,
                end: extent,
            },
        ] {
            for direction in [
                DraftPieceMarkerDirectionV1::Forward,
                DraftPieceMarkerDirectionV1::Backward,
            ] {
                for cap in [48, 90, 120, 1024] {
                    let mut cursor = None;
                    for _ in 0..400 {
                        let demand =
                            DraftPieceMarkerDemandV1::new(scope, direction, cursor, 7, cap);
                        let ordinary = storage.draft_piece_marker_demand(
                            &store,
                            source.root(),
                            demand.clone(),
                        );
                        let detached = source.marker_demand(demand);
                        match (ordinary, detached) {
                            (Ok(a), Ok(b)) => {
                                assert_eq!(a.markers(), b.markers());
                                assert_eq!(a.preceding(), b.preceding());
                                assert_eq!(a.following(), b.following());
                                assert_eq!(a.continuation(), b.continuation());
                                assert_eq!(
                                    a.requested_side_complete(),
                                    b.requested_side_complete()
                                );
                                assert_eq!(a.retained_bytes(), b.retained_bytes());
                                cursor = b.continuation();
                                if cursor.is_none() {
                                    break;
                                }
                            }
                            (
                                Err(DraftPieceRangeSourceErrorV1::Limit),
                                Err(DetachedDraftReadErrorV1::Limits),
                            ) => break,
                            (a, b) => panic!("marker mismatch: {a:?}, {b:?}"),
                        }
                    }
                }
            }
        }
        let wrong = DraftCompositeSearchKeyV1::Marker {
            anchor: 0,
            order_key: 0,
            marker_id: SyndicDraftMarkerId::from_bytes([0; 16]),
        };
        assert!(matches!(
            source.marker_demand(DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(0),
                DraftPieceMarkerDirectionV1::Forward,
                Some(wrong),
                7,
                1024
            )),
            Err(DetachedDraftReadErrorV1::Malformed(
                DraftPieceMalformedRangeRequestV1::Cursor
            ))
        ));
        store.close().unwrap();
        assert_eq!(
            source
                .marker_demand(DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::ExactAnchor(0),
                    DraftPieceMarkerDirectionV1::Backward,
                    None,
                    3,
                    1024
                ))
                .unwrap()
                .markers()
                .len(),
            3
        );
        drop(source);
        assert_eq!(pool.usage().unwrap(), TemporaryReadPoolUsage::default());
    }
}

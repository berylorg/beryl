use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
#[cfg(feature = "test-faults")]
use beryl_model::DomainRevision;
use beryl_model::{
    ExecutionBinding, ImageLabelOrdinal, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicDraftMarkerId, SyndicThreadId,
};
#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{
    DraftPieceDescendantCorruption, DraftPieceDescendantTarget,
    arm_draft_piece_candidate_read_fault, delete_draft_piece_terminal_build,
    draft_piece_position_record_count, inject_draft_piece_descendant_corruption,
    inject_draft_piece_settlement_closure_corruption,
};
use syndic_storage::{
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1,
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionOpenOutcomeV1,
    DraftEditorCandidateSessionOpenRequestV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftEditorCandidateSessionV1, DraftEditorCurrentSelectorV1, DraftPieceBuildFragmentV1,
    DraftPieceEditHeaderV1, DraftPieceMarkerAtV1, DraftPieceMarkerDemandV1,
    DraftPieceMarkerDirectionV1, DraftPieceMarkerMoveV1, DraftPieceMarkerScopeV1,
    DraftPieceMarkerV1, DraftPieceOperationIdV1, DraftPieceOperationStatusV1,
    DraftPieceOperationVerificationV1, DraftPiecePrepareErrorV1, DraftPieceRejectedReasonV1,
    DraftPieceReplacementV1, DraftPieceSettlementOutcomeV1, DraftPieceTextDemandV1, DraftPieceV1,
    PreparedDraftPieceEditV1, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    canonical_draft_piece_fragment_chain_v1, canonical_empty_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_root_v1,
};
#[cfg(feature = "test-faults")]
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftPieceRangeSourceErrorV1,
    DraftPieceReconciledCommandV1, DraftPieceTransactionOutcomeV1,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase136-{name}-{}-{}",
            std::process::id(),
            NEXT_HOME.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone)]
struct Transaction {
    session: DraftEditorCandidateSessionIdV1,
    operation: DraftPieceOperationIdV1,
    prepared: PreparedDraftPieceEditV1,
    fragments: Vec<DraftPieceBuildFragmentV1>,
}

#[derive(Clone)]
struct CandidateCurrent {
    session: DraftEditorCandidateSessionV1,
    draft: CandidateDraft,
}

#[derive(Clone, Copy)]
struct CandidateDraft {
    id: SyndicDraftId,
    revision: beryl_model::DraftRevision,
    root: syndic_storage::DraftPieceRootReferenceV1,
}

impl CandidateCurrent {
    const fn draft(&self) -> &CandidateDraft {
        &self.draft
    }
}

impl CandidateDraft {
    const fn id(&self) -> SyndicDraftId {
        self.id
    }

    const fn revision(&self) -> beryl_model::DraftRevision {
        self.revision
    }

    const fn piece_root(&self) -> syndic_storage::DraftPieceRootReferenceV1 {
        self.root
    }
}

#[test]
fn canonical_empty_text_marker_move_duplicate_and_exact_history_are_preserved() {
    let (_home, store, storage, thread) = fixture("canonical-cases", 3);
    let initial = current(storage, &store, thread);
    let initial_root = initial.draft().piece_root();
    assert_eq!(
        initial_root,
        canonical_empty_draft_piece_root_v1(
            initial.draft().id(),
            initial.draft().revision(),
            initial_root.key().operation_id(),
        )
        .reference()
    );

    let marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([4; 16]),
        7,
        ImageLabelOrdinal::new(1).unwrap(),
    );
    let marker_only = transaction(
        storage,
        &initial,
        4,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Marker(marker)],
        )],
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
    );
    run_transaction(storage, &store, &marker_only, 2);
    let marker_only_current = current(storage, &store, thread);
    let marker_only_root = marker_only_current.draft().piece_root();
    assert_eq!(marker_only_root.summary().logical_utf8_bytes(), 0);
    assert_eq!(
        storage
            .draft_piece_marker_demand(
                &store,
                marker_only_root,
                DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::ExactAnchor(0),
                    DraftPieceMarkerDirectionV1::Forward,
                    None,
                    4,
                    65_536,
                ),
            )
            .unwrap()
            .markers(),
        &[DraftPieceMarkerAtV1::new(0, marker)]
    );
    assert!(
        storage
            .draft_piece_text_demand(
                &store,
                marker_only_root,
                DraftPieceTextDemandV1::Forward(0),
                4,
            )
            .unwrap()
            .bytes()
            .is_empty()
    );

    let text_before_marker = transaction(
        storage,
        &marker_only_current,
        5,
        vec![DraftPieceReplacementV1::new(
            DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll),
            DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll),
            vec![DraftPieceV1::Text("abcd".to_owned())],
        )],
        DraftCompositePositionV1::new(4, DraftCompositeGapWitnessV1::BeforeAll),
    );
    run_transaction(storage, &store, &text_before_marker, 3);
    let before_move = current(storage, &store, thread);
    let before_move_root = before_move.draft().piece_root();
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                before_move_root,
                DraftPieceTextDemandV1::Forward(0),
                16,
            )
            .unwrap()
            .bytes(),
        b"abcd"
    );
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                before_move_root,
                DraftPieceMarkerAtV1::new(4, marker),
            )
            .unwrap()
    );

    let move_marker = transaction(
        storage,
        &before_move,
        6,
        vec![
            DraftPieceReplacementV1::new(point(2), point(2), vec![DraftPieceV1::Marker(marker)])
                .with_moves(vec![DraftPieceMarkerMoveV1::new(
                    DraftPieceMarkerAtV1::new(4, marker),
                    marker,
                    2,
                )]),
            DraftPieceReplacementV1::new(
                DraftCompositePositionV1::new(4, DraftCompositeGapWitnessV1::BeforeAll),
                DraftCompositePositionV1::new(4, DraftCompositeGapWitnessV1::AfterAll),
                Vec::new(),
            ),
        ],
        DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
    );
    run_transaction(storage, &store, &move_marker, 4);
    let moved = current(storage, &store, thread);
    let moved_root = moved.draft().piece_root();
    assert!(
        storage
            .validate_draft_marker_location(
                &store,
                moved_root,
                DraftPieceMarkerAtV1::new(2, marker),
            )
            .unwrap()
    );
    assert!(
        !storage
            .validate_draft_marker_location(
                &store,
                moved_root,
                DraftPieceMarkerAtV1::new(4, marker),
            )
            .unwrap()
    );

    let duplicate = transaction(
        storage,
        &moved,
        7,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Marker(marker)],
        )],
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
    );
    begin_and_stage(storage, &store, &duplicate);
    let reason = loop {
        match storage.prepare_draft_piece_build_advance(
            &store,
            moved.draft().id(),
            duplicate.session,
            duplicate.operation,
        ) {
            Err(DraftPiecePrepareErrorV1::Rejected(reason)) => break reason,
            Ok(Some(advance)) => committed(execute(
                &store,
                storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
            )),
            Ok(None) => panic!("duplicate marker operation unexpectedly completed"),
            Err(error) => panic!("unexpected duplicate marker preparation error: {error:?}"),
        }
    };
    assert_eq!(reason, DraftPieceRejectedReasonV1::DuplicateMarkerIdentity);
    committed(execute(
        &store,
        storage.reject_draft_piece_edit(
            storage.revision(&store).unwrap(),
            duplicate.prepared.clone(),
            reason,
        ),
    ));
    assert!(matches!(
        exact_status(storage, &store, &duplicate),
        DraftPieceOperationStatusV1::Settled(settlement)
            if *settlement.outcome() == DraftPieceSettlementOutcomeV1::Rejected(reason)
    ));
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        moved_root
    );

    let moved_after_rejection = current(storage, &store, thread);
    let suffix = transaction(
        storage,
        &moved_after_rejection,
        8,
        vec![DraftPieceReplacementV1::new(
            point(3),
            point(4),
            vec![DraftPieceV1::Text("Z".to_owned())],
        )],
        point(4),
    );
    run_transaction(storage, &store, &suffix, 5);
    let suffix_current = current(storage, &store, thread);
    let suffix_root = suffix_current.draft().piece_root();
    assert_eq!(
        suffix_root.marker_index_root(),
        moved_root.marker_index_root()
    );
    assert_eq!(
        suffix_root.marker_index_summary(),
        moved_root.marker_index_summary()
    );
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                before_move_root,
                DraftPieceTextDemandV1::Forward(0),
                16,
            )
            .unwrap()
            .bytes(),
        b"abcd"
    );
    assert_eq!(
        storage
            .draft_piece_text_demand(&store, suffix_root, DraftPieceTextDemandV1::Forward(0), 16,)
            .unwrap()
            .bytes(),
        b"abcZ"
    );

    let text_only = transaction(
        storage,
        &suffix_current,
        9,
        vec![DraftPieceReplacementV1::new(
            DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::BeforeAll),
            DraftCompositePositionV1::new(2, DraftCompositeGapWitnessV1::AfterAll),
            Vec::new(),
        )],
        point(2),
    );
    run_transaction(storage, &store, &text_only, 6);
    let text_only_current = current(storage, &store, thread);
    let text_only_root = text_only_current.draft().piece_root();
    assert_eq!(text_only_root.summary().marker_count(), 0);
    assert!(text_only_root.marker_index_root().is_none());
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                text_only_root,
                DraftPieceTextDemandV1::Forward(0),
                16,
            )
            .unwrap()
            .bytes(),
        b"abcZ"
    );

    let empty = transaction(
        storage,
        &text_only_current,
        10,
        vec![DraftPieceReplacementV1::new(point(0), point(4), Vec::new())],
        point(0),
    );
    run_transaction(storage, &store, &empty, 7);
    let final_current = current(storage, &store, thread);
    let final_root = final_current.draft().piece_root();
    assert_ne!(final_root.key(), initial_root.key());
    assert_ne!(
        final_root.key().operation_id(),
        initial_root.key().operation_id()
    );
    assert_eq!(
        final_root.key().operation_id(),
        DraftPieceOperationIdV1::from_bytes([10; 16])
    );
    assert_eq!(final_root.combined_digest(), initial_root.combined_digest());
}

#[test]
fn fresh_process_resumes_only_from_durable_head_fragments_and_records() {
    let (home, store, storage, thread) = fixture("durable-resume", 10);
    let base = current(storage, &store, thread);
    let staged_transaction = transaction(
        storage,
        &base,
        11,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("recover".to_owned())],
        )],
        point(7),
    );
    begin_and_stage(storage, &store, &staged_transaction);
    let first = storage
        .prepare_draft_piece_build_advance(
            &store,
            base.draft().id(),
            staged_transaction.session,
            staged_transaction.operation,
        )
        .unwrap()
        .unwrap();
    committed(execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), first),
    ));
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        base.draft().piece_root()
    );
    let staged_session = staged_transaction.session;
    let staged_operation = staged_transaction.operation;
    drop(staged_transaction);
    drop(store);

    let mut reopened =
        HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    advance_until_complete_for(
        storage,
        &reopened,
        base.draft().id(),
        staged_session,
        staged_operation,
    );
    assert_eq!(
        current(storage, &reopened, thread).draft().piece_root(),
        base.draft().piece_root()
    );

    let recovered = transaction(
        storage,
        &base,
        11,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("recover".to_owned())],
        )],
        point(7),
    );
    assert!(matches!(
        exact_status(storage, &reopened, &recovered),
        DraftPieceOperationStatusV1::Complete(_)
    ));
    committed(execute(
        &reopened,
        storage.settle_draft_piece_edit(storage.revision(&reopened).unwrap(), recovered.prepared),
    ));
    let root = current(storage, &reopened, thread).draft().piece_root();
    assert_eq!(
        storage
            .draft_piece_text_demand(&reopened, root, DraftPieceTextDemandV1::Forward(0), 64,)
            .unwrap()
            .bytes(),
        b"recover"
    );
}

#[test]
fn exact_fragment_replay_and_header_collision_are_mutation_free() {
    let (_home, store, storage, thread) = fixture("exact-replay", 30);
    let base = current(storage, &store, thread);
    let accepted = transaction(
        storage,
        &base,
        31,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("x".to_owned())],
        )],
        point(1),
    );
    run_transaction(storage, &store, &accepted, 2);
    let revision = storage.revision(&store).unwrap();
    assert!(matches!(
        exact_status(storage, &store, &accepted),
        DraftPieceOperationStatusV1::Settled(_)
    ));
    assert!(matches!(
        execute(
            &store,
            storage.begin_draft_piece_edit(revision, accepted.prepared.clone()),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(storage.revision(&store).unwrap(), revision);

    let colliding = transaction(
        storage,
        &base,
        31,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("y".to_owned())],
        )],
        point(1),
    );
    let before = storage.revision(&store).unwrap();
    assert!(matches!(
        exact_status(storage, &store, &colliding),
        DraftPieceOperationStatusV1::Collision(_)
    ));
    assert_eq!(storage.revision(&store).unwrap(), before);
}

#[test]
fn settled_operation_replays_after_a_newer_build_replaces_the_draft_head() {
    let (_home, store, storage, thread) = fixture("settlement-head-replacement", 35);
    let base = current(storage, &store, thread);
    let first = transaction(
        storage,
        &base,
        36,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("first".to_owned())],
        )],
        point(5),
    );
    run_transaction(storage, &store, &first, 2);
    let first_settlement = exact_status(storage, &store, &first);
    let next_base = current(storage, &store, thread);
    let second = transaction(
        storage,
        &next_base,
        37,
        vec![DraftPieceReplacementV1::new(
            point(5),
            point(5),
            vec![DraftPieceV1::Text(" second".to_owned())],
        )],
        point(12),
    );
    begin_and_stage(storage, &store, &second);
    assert!(matches!(
        exact_status(storage, &store, &second),
        DraftPieceOperationStatusV1::Open(_)
    ));
    assert_eq!(exact_status(storage, &store, &first), first_settlement);
}

#[cfg(feature = "test-faults")]
#[test]
fn settlement_replay_requires_its_exact_terminal_build_after_newer_builds() {
    let (_home, store, storage, thread) = fixture("terminal-build-deletion", 38);
    let base = current(storage, &store, thread);
    let first = transaction(
        storage,
        &base,
        39,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("settled".to_owned())],
        )],
        point(7),
    );
    run_transaction(storage, &store, &first, 2);
    let next_base = current(storage, &store, thread);
    let second = transaction(
        storage,
        &next_base,
        40,
        vec![DraftPieceReplacementV1::new(
            point(7),
            point(7),
            vec![DraftPieceV1::Text(" head".to_owned())],
        )],
        point(12),
    );
    begin_and_stage(storage, &store, &second);
    committed(execute(
        &store,
        delete_draft_piece_terminal_build(
            &store,
            storage,
            syndic_storage::DraftPieceSettlementKeyV1::new(
                first.prepared.header().draft_id(),
                first.prepared.header().session_id(),
                first.prepared.header().operation_id(),
            ),
        ),
    ));
    assert!(matches!(
        execute(
            &store,
            storage.settle_draft_piece_edit(
                storage.revision(&store).unwrap(),
                first.prepared.clone(),
            ),
        ),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        storage.draft_piece_operation_status_page(&store, &first.prepared, 1, &first.fragments),
        Err(syndic_storage::SyndicReadError::Invariant(_))
    ));
}

#[test]
fn no_change_terminals_close_the_five_way_settlement() {
    let (_home, store, storage, thread) = fixture("terminals", 50);
    let base = current(storage, &store, thread);
    let rejected = transaction(
        storage,
        &base,
        51,
        vec![DraftPieceReplacementV1::new(point(0), point(0), Vec::new())],
        point(0),
    );
    begin_and_stage(storage, &store, &rejected);
    committed(execute(
        &store,
        storage.reject_draft_piece_edit(
            storage.revision(&store).unwrap(),
            rejected.prepared.clone(),
            syndic_storage::DraftPieceRejectedReasonV1::TreeLimit,
        ),
    ));
    let DraftPieceOperationStatusV1::Settled(settlement) = exact_status(storage, &store, &rejected)
    else {
        panic!("missing rejected settlement")
    };
    assert!(matches!(
        settlement.outcome(),
        DraftPieceSettlementOutcomeV1::Rejected(_)
    ));
    assert_eq!(
        current(storage, &store, thread).draft().piece_root(),
        base.draft().piece_root()
    );

    let after_rejection = current(storage, &store, thread);
    let failed = transaction(
        storage,
        &after_rejection,
        52,
        vec![DraftPieceReplacementV1::new(point(0), point(0), Vec::new())],
        point(0),
    );
    begin_and_stage(storage, &store, &failed);
    committed(execute(
        &store,
        storage.error_draft_piece_edit(
            storage.revision(&store).unwrap(),
            failed.prepared.clone(),
            syndic_storage::DraftPieceErrorReasonV1::ResourceLimit,
        ),
    ));
    let DraftPieceOperationStatusV1::Settled(settlement) = exact_status(storage, &store, &failed)
    else {
        panic!("missing error settlement")
    };
    assert!(matches!(
        settlement.outcome(),
        DraftPieceSettlementOutcomeV1::Error(_)
    ));
    #[cfg(feature = "test-faults")]
    {
        committed(execute(
            &store,
            inject_draft_piece_settlement_closure_corruption(
                &store,
                storage,
                syndic_storage::DraftPieceSettlementKeyV1::new(
                    failed.prepared.header().draft_id(),
                    failed.prepared.header().session_id(),
                    failed.prepared.header().operation_id(),
                ),
            ),
        ));
        let corrupted = storage.draft_piece_operation_status_page(
            &store,
            &failed.prepared,
            1,
            &failed.fragments,
        );
        assert!(
            corrupted.is_err(),
            "corrupted settlement was accepted: {corrupted:?}"
        );
    }
}

#[test]
fn cursor_reads_skip_long_marker_and_text_only_subtrees_with_fixed_bounds() {
    let (_home, store, storage, thread) = fixture("cursor-bounds", 70);
    let base = current(storage, &store, thread);
    let mut pieces = (0..200_u64)
        .map(|order| {
            DraftPieceV1::Marker(DraftPieceMarkerV1::new(
                SyndicDraftMarkerId::from_bytes([order as u8; 16]),
                order,
                ImageLabelOrdinal::new((order % 200) + 1).unwrap(),
            ))
        })
        .collect::<Vec<_>>();
    pieces.push(DraftPieceV1::Text("z".to_owned()));
    let transaction = transaction(
        storage,
        &base,
        71,
        vec![DraftPieceReplacementV1::new(point(0), point(0), pieces)],
        point(1),
    );
    run_transaction(storage, &store, &transaction, 2);
    let root = current(storage, &store, thread).draft().piece_root();
    let text = storage
        .draft_piece_text_demand(&store, root, DraftPieceTextDemandV1::Forward(0), 16)
        .unwrap();
    assert_eq!(text.bytes(), b"z");
    assert!(text.records_read() <= 16);
    let first = storage
        .draft_piece_marker_demand(
            &store,
            root,
            DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(0),
                DraftPieceMarkerDirectionV1::Forward,
                None,
                7,
                65_536,
            ),
        )
        .unwrap();
    assert_eq!(first.markers().len(), 7);
    assert!(first.records_read() <= 16);
    let second = storage
        .draft_piece_marker_demand(
            &store,
            root,
            DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(0),
                DraftPieceMarkerDirectionV1::Forward,
                first.continuation(),
                7,
                65_536,
            ),
        )
        .unwrap();
    assert_eq!(second.markers().len(), 7);
    assert_eq!(second.markers()[0].marker().order_key(), 7);
    assert!(second.records_read() <= 16);
}

#[cfg(feature = "test-faults")]
#[test]
fn composite_position_descent_is_fixed_bounded_at_two_tree_scales() {
    for (case, count) in [129_u64, 240].into_iter().enumerate() {
        let (_home, store, storage, thread) = fixture("position-descent-scales", 71 + case as u8);
        let base = current(storage, &store, thread);
        let pieces = (0..count)
            .map(|order| {
                let mut id = [0xA5; 16];
                id[..8].copy_from_slice(&order.to_be_bytes());
                DraftPieceV1::Marker(DraftPieceMarkerV1::new(
                    SyndicDraftMarkerId::from_bytes(id),
                    order,
                    ImageLabelOrdinal::new(order + 1).unwrap(),
                ))
            })
            .collect();
        let edit = transaction(
            storage,
            &base,
            73 + case as u8,
            vec![DraftPieceReplacementV1::new(point(0), point(0), pieces)],
            DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
        );
        run_transaction(storage, &store, &edit, 2);
        let root = current(storage, &store, thread).draft().piece_root();
        assert!(root.summary().height() >= 2);
        assert!(
            draft_piece_position_record_count(
                &store,
                storage,
                root,
                DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
            )
            .unwrap()
                <= 64
        );
    }
}

#[test]
fn deletion_of_more_than_256_markers_advances_in_bounded_commands() {
    let (_home, store, storage, thread) = fixture("large-delete", 90);
    let base = current(storage, &store, thread);
    let text = transaction(
        storage,
        &base,
        91,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("x".to_owned())],
        )],
        point(1),
    );
    run_transaction(storage, &store, &text, 2);
    let base = current(storage, &store, thread);
    let marker_pieces = |start: u64, count: u64| {
        (start..start + count)
            .map(|offset| {
                DraftPieceV1::Marker(DraftPieceMarkerV1::new(
                    SyndicDraftMarkerId::from_bytes([
                        (offset >> 8) as u8,
                        offset as u8,
                        3,
                        3,
                        3,
                        3,
                        3,
                        3,
                        3,
                        3,
                        3,
                        3,
                        3,
                        3,
                        3,
                        3,
                    ]),
                    offset + 1,
                    ImageLabelOrdinal::new(offset + 1).unwrap(),
                ))
            })
            .collect::<Vec<_>>()
    };
    let markers = transaction(
        storage,
        &base,
        92,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            marker_pieces(0, 200),
        )],
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
    );
    run_transaction(storage, &store, &markers, 3);
    let base = current(storage, &store, thread);
    let more_markers = transaction(
        storage,
        &base,
        93,
        vec![DraftPieceReplacementV1::new(
            point(1),
            point(1),
            marker_pieces(200, 101),
        )],
        DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
    );
    run_transaction(storage, &store, &more_markers, 4);
    let base = current(storage, &store, thread);
    assert_eq!(base.draft().piece_root().summary().marker_count(), 301);
    let delete = transaction(
        storage,
        &base,
        94,
        vec![DraftPieceReplacementV1::new(
            DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll),
            DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::AfterAll),
            Vec::new(),
        )],
        point(0),
    );
    run_transaction(storage, &store, &delete, 5);
    let root = current(storage, &store, thread).draft().piece_root();
    assert_eq!(root.summary().marker_count(), 0);
    assert_eq!(root.marker_index_summary().record_count(), 0);
    assert_eq!(root.summary().logical_utf8_bytes(), 0);
}

#[test]
fn one_64k_text_piece_resumes_at_durable_byte_offsets() {
    let (_home, store, storage, thread) = fixture("intra-text-frontier", 97);
    let base = current(storage, &store, thread);
    let seed = transaction(
        storage,
        &base,
        98,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("a".to_owned()); 129],
        )],
        point(129),
    );
    run_transaction(storage, &store, &seed, 2);
    let base = current(storage, &store, thread);
    assert!(base.draft().piece_root().summary().height() >= 2);
    let text = "x".repeat(65_536);
    let edit = transaction(
        storage,
        &base,
        99,
        vec![DraftPieceReplacementV1::new(
            point(129),
            point(129),
            vec![DraftPieceV1::Text(text.clone())],
        )],
        point(65_665),
    );
    begin_and_stage(storage, &store, &edit);
    let mut saw_intra_text_offset = false;
    loop {
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                &store,
                base.draft().id(),
                edit.session,
                edit.operation,
            )
            .unwrap()
        else {
            break;
        };
        if matches!(
            advance.frontier(),
            syndic_storage::DraftPieceBuildFrontierV1::Inserting { next_byte, .. }
                if next_byte > 0
        ) {
            saw_intra_text_offset = true;
        }
        assert!(advance.staged_record_count() <= 256);
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
    }
    assert!(saw_intra_text_offset);
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    let root = current(storage, &store, thread).draft().piece_root();
    assert_eq!(root.summary().logical_utf8_bytes(), 65_665);
    assert_eq!(
        storage
            .draft_piece_text_demand(&store, root, DraftPieceTextDemandV1::Forward(65_649), 16,)
            .unwrap()
            .bytes(),
        &text.as_bytes()[65_520..]
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn detached_descendant_replacements_are_rejected_under_unchanged_parent_links() {
    let corruptions = [
        DraftPieceDescendantCorruption::Digest,
        DraftPieceDescendantCorruption::Aggregate,
        DraftPieceDescendantCorruption::Envelope,
        DraftPieceDescendantCorruption::Height,
        DraftPieceDescendantCorruption::Shape,
        DraftPieceDescendantCorruption::AggregateOverflow,
        DraftPieceDescendantCorruption::EnvelopeGap,
        DraftPieceDescendantCorruption::EnvelopeOverlap,
        DraftPieceDescendantCorruption::EnvelopeOutOfParent,
    ];
    for (case, target) in [
        DraftPieceDescendantTarget::Sequence,
        DraftPieceDescendantTarget::MarkerIndex,
    ]
    .into_iter()
    .enumerate()
    {
        for (variant, corruption) in corruptions.into_iter().enumerate() {
            let seed = 110 + (case * corruptions.len() + variant) as u8;
            let (_home, store, storage, thread) = fixture("descendant-corruption", seed);
            let base = current(storage, &store, thread);
            let pieces = (0_u64..129)
                .map(|offset| {
                    DraftPieceV1::Marker(DraftPieceMarkerV1::new(
                        SyndicDraftMarkerId::from_bytes([
                            0,
                            offset as u8,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                            seed,
                        ]),
                        offset + 1,
                        ImageLabelOrdinal::new(offset + 1).unwrap(),
                    ))
                })
                .collect();
            let transaction = transaction(
                storage,
                &base,
                seed ^ 0x80,
                vec![DraftPieceReplacementV1::new(point(0), point(0), pieces)],
                DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::AfterAll),
            );
            run_transaction(storage, &store, &transaction, 20 + u64::from(seed));
            let root = current(storage, &store, thread).draft().piece_root();
            assert_eq!(root.summary().height(), 2);
            assert_eq!(root.marker_index_summary().height(), 2);
            committed(execute(
                &store,
                inject_draft_piece_descendant_corruption(&store, storage, root, target, corruption),
            ));
            let rejected = match target {
                DraftPieceDescendantTarget::Sequence => matches!(
                    storage.draft_piece_marker_demand(
                        &store,
                        root,
                        DraftPieceMarkerDemandV1::new(
                            DraftPieceMarkerScopeV1::ExactAnchor(0),
                            DraftPieceMarkerDirectionV1::Forward,
                            None,
                            1,
                            65_536,
                        ),
                    ),
                    Err(DraftPieceRangeSourceErrorV1::Invariant)
                ),
                DraftPieceDescendantTarget::MarkerIndex => matches!(
                    storage.draft_marker_identity(
                        &store,
                        root,
                        SyndicDraftMarkerId::from_bytes([
                            0, 0, seed, seed, seed, seed, seed, seed, seed, seed, seed, seed, seed,
                            seed, seed, seed,
                        ]),
                    ),
                    Err(DraftPiecePrepareErrorV1::InvalidRoot)
                ),
            };
            assert!(
                rejected,
                "target {target:?} corruption {corruption:?} was accepted"
            );
        }
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn candidate_read_detects_selector_drift_after_exact_traversal() {
    let (_home, store, storage, thread) = fixture("current-drift", 140);
    let base = current(storage, &store, thread);
    let initial = transaction(
        storage,
        &base,
        141,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("before".to_owned())],
        )],
        point(6),
    );
    run_transaction(storage, &store, &initial, 30);
    let base = current(storage, &store, thread);
    let successor = transaction(
        storage,
        &base,
        142,
        vec![DraftPieceReplacementV1::new(
            point(6),
            point(6),
            vec![DraftPieceV1::Text("after".to_owned())],
        )],
        point(11),
    );
    begin_and_stage(storage, &store, &successor);
    advance_until_complete(storage, &store, &successor);
    let active = match storage
        .draft_editor_candidate_session(&store, base.session.draft_id(), base.session.session_id())
        .unwrap()
    {
        syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        other => panic!("candidate session is not active: {other:?}"),
    };
    let binding = DraftEditorCandidateActivationBindingV1::from_head(&active);
    let prepared = successor.prepared.clone();
    arm_draft_piece_candidate_read_fault(move |store, storage| {
        committed(execute(
            store,
            storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
        ));
    });
    assert!(matches!(
        storage.candidate_draft_piece_text_demand(
            &store,
            binding,
            DraftPieceTextDemandV1::Forward(0),
            64,
        ),
        Err(DraftPieceRangeSourceErrorV1::ConcurrentChange)
    ));
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                current(storage, &store, thread).draft().piece_root(),
                DraftPieceTextDemandV1::Forward(0),
                64,
            )
            .unwrap()
            .bytes(),
        b"beforeafter"
    );
}

#[cfg(feature = "test-faults")]
#[test]
fn writer_outcomes_reconcile_to_pending_or_exact_terminal_state() {
    let home = TestHome::new("writer-custody");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([150; 16]);
    let draft = SyndicDraftId::from_bytes([151; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                execution(),
                SyndicTimestamp::from_unix_millis(1),
            ),
        ),
    ));
    let base = current(storage, &store, thread);
    let transaction = transaction(
        storage,
        &base,
        152,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("custody".to_owned())],
        )],
        point(7),
    );
    begin_and_stage(storage, &store, &transaction);
    let advance = storage
        .prepare_draft_piece_build_advance(
            &store,
            draft,
            transaction.session,
            transaction.operation,
        )
        .unwrap()
        .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let indeterminate = execute(
        &store,
        storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
    );
    assert!(matches!(
        &indeterminate,
        CommandOutcome::Indeterminate { .. }
    ));
    let fragments = transaction.fragments.clone();
    let reconciled = storage
        .reconcile_draft_piece_command_outcome(
            &store,
            &transaction.prepared,
            indeterminate,
            |start| {
                fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .take(256)
                    .cloned()
                    .collect()
            },
        )
        .unwrap();
    assert!(matches!(
        reconciled,
        DraftPieceReconciledCommandV1::Pending(_)
    ));
    advance_until_complete(storage, &store, &transaction);

    let stale = execute(
        &store,
        storage.settle_draft_piece_edit(
            DomainRevision::new(1).unwrap(),
            transaction.prepared.clone(),
        ),
    );
    assert!(matches!(&stale, CommandOutcome::NotCommitted { .. }));
    let fragments = transaction.fragments.clone();
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(&store, &transaction.prepared, stale, |start| {
                fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .take(256)
                    .cloned()
                    .collect()
            },)
            .unwrap(),
        DraftPieceReconciledCommandV1::Pending(DraftPieceOperationStatusV1::Complete(_))
    ));

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let settlement = execute(
        &store,
        storage.settle_draft_piece_edit(
            storage.revision(&store).unwrap(),
            transaction.prepared.clone(),
        ),
    );
    let fragments = transaction.fragments.clone();
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(
                &store,
                &transaction.prepared,
                settlement,
                |start| fragments
                    .iter()
                    .skip((start - 1) as usize)
                    .take(256)
                    .cloned()
                    .collect(),
            )
            .unwrap(),
        DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(_))
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn every_durable_frontier_survives_indeterminate_writer_custody() {
    let home = TestHome::new("frontier-custody");
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([160; 16]);
    let draft = SyndicDraftId::from_bytes([161; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                draft,
                execution(),
                SyndicTimestamp::from_unix_millis(1),
            ),
        ),
    ));
    let base = current(storage, &store, thread);
    let marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([162; 16]),
        1,
        ImageLabelOrdinal::new(1).unwrap(),
    );
    let seed = transaction(
        storage,
        &base,
        162,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![
                DraftPieceV1::Text("base".to_owned()),
                DraftPieceV1::Marker(marker),
            ],
        )],
        DraftCompositePositionV1::new(4, DraftCompositeGapWitnessV1::AfterAll),
    );
    run_transaction(storage, &store, &seed, 2);

    let base = current(storage, &store, thread);
    let edit = transaction(
        storage,
        &base,
        163,
        vec![DraftPieceReplacementV1::new(
            point(0),
            DraftCompositePositionV1::new(4, DraftCompositeGapWitnessV1::AfterAll),
            vec![DraftPieceV1::Text("x".repeat(65_536))],
        )],
        point(65_536),
    );
    let fragment_page = |start: u64| {
        edit.fragments
            .iter()
            .skip((start - 1) as usize)
            .take(256)
            .cloned()
            .collect()
    };

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let begun = execute(
        &store,
        storage.begin_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    );
    assert!(matches!(begun, CommandOutcome::Indeterminate { .. }));
    let DraftPieceReconciledCommandV1::Pending(DraftPieceOperationStatusV1::Open(build)) = storage
        .reconcile_draft_piece_command_outcome(&store, &edit.prepared, begun, fragment_page)
        .unwrap()
    else {
        panic!("begin custody did not recover the receiving frontier")
    };
    assert!(matches!(
        build.frontier(),
        syndic_storage::DraftPieceBuildFrontierV1::Receiving { .. }
    ));

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let staged = execute(
        &store,
        storage.stage_draft_piece_fragment(
            storage.revision(&store).unwrap(),
            edit.prepared.clone(),
            edit.fragments[0].clone(),
        ),
    );
    assert!(matches!(staged, CommandOutcome::Indeterminate { .. }));
    let DraftPieceReconciledCommandV1::Pending(DraftPieceOperationStatusV1::Open(build)) = storage
        .reconcile_draft_piece_command_outcome(&store, &edit.prepared, staged, fragment_page)
        .unwrap()
    else {
        panic!("fragment custody did not recover the move-reconciliation frontier")
    };
    assert!(matches!(
        build.frontier(),
        syndic_storage::DraftPieceBuildFrontierV1::ReconcilingMoves { .. }
    ));

    let mut removing = false;
    let mut applying = false;
    let mut inserting = false;
    let mut intra_text = false;
    let mut cross_validating = false;
    let mut complete = false;
    for _ in 0..64 {
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(&store, draft, edit.session, edit.operation)
            .unwrap()
        else {
            break;
        };
        match advance.frontier() {
            syndic_storage::DraftPieceBuildFrontierV1::Removing { .. } => removing = true,
            syndic_storage::DraftPieceBuildFrontierV1::Applying { .. } => applying = true,
            syndic_storage::DraftPieceBuildFrontierV1::Inserting { next_byte, .. } => {
                inserting = true;
                intra_text |= next_byte != 0;
            }
            syndic_storage::DraftPieceBuildFrontierV1::CrossValidating => cross_validating = true,
            syndic_storage::DraftPieceBuildFrontierV1::Complete => complete = true,
            _ => {}
        }
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let outcome = execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        );
        assert!(matches!(outcome, CommandOutcome::Indeterminate { .. }));
        assert!(matches!(
            storage
                .reconcile_draft_piece_command_outcome(
                    &store,
                    &edit.prepared,
                    outcome,
                    fragment_page,
                )
                .unwrap(),
            DraftPieceReconciledCommandV1::Pending(_)
        ));
    }
    assert!(removing && applying && inserting && intra_text && cross_validating && complete);

    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let settled = execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    );
    assert!(matches!(settled, CommandOutcome::Indeterminate { .. }));
    assert!(matches!(
        storage
            .reconcile_draft_piece_command_outcome(&store, &edit.prepared, settled, fragment_page,)
            .unwrap(),
        DraftPieceReconciledCommandV1::Terminal(DraftPieceTransactionOutcomeV1::Committed(_))
    ));
}

fn transaction(
    storage: SyndicStorage,
    current: &CandidateCurrent,
    operation: u8,
    replacements: Vec<DraftPieceReplacementV1>,
    caret: DraftCompositePositionV1,
) -> Transaction {
    let session = current.session.session_id();
    let operation = DraftPieceOperationIdV1::from_bytes([operation; 16]);
    let chain = canonical_draft_piece_fragment_chain_v1(&replacements);
    let header = DraftPieceEditHeaderV1::new(
        current.draft().id(),
        session,
        current.session.newest_candidate_generation(),
        current.draft().piece_root(),
        operation,
        caret,
        caret,
        replacements.len() as u64,
        chain,
    );
    let prepared = storage
        .prepare_draft_piece_edit(header, &current.session)
        .unwrap();
    let mut preceding = canonical_empty_draft_piece_fragment_chain_v1();
    let fragments = replacements
        .into_iter()
        .enumerate()
        .map(|(ordinal, replacement)| {
            let fragment = storage
                .prepare_draft_piece_fragment(&prepared, ordinal as u64 + 1, preceding, replacement)
                .unwrap();
            preceding = fragment.chain_digest();
            fragment
        })
        .collect();
    Transaction {
        session,
        operation,
        prepared,
        fragments,
    }
}

fn begin_and_stage(storage: SyndicStorage, store: &HomeStore, transaction: &Transaction) {
    committed(execute(
        store,
        storage.begin_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
    for fragment in &transaction.fragments {
        committed(execute(
            store,
            storage.stage_draft_piece_fragment(
                storage.revision(store).unwrap(),
                transaction.prepared.clone(),
                fragment.clone(),
            ),
        ));
    }
}

fn advance_until_complete(storage: SyndicStorage, store: &HomeStore, transaction: &Transaction) {
    advance_until_complete_for(
        storage,
        store,
        transaction.prepared.header().draft_id(),
        transaction.session,
        transaction.operation,
    );
}

fn advance_until_complete_for(
    storage: SyndicStorage,
    store: &HomeStore,
    draft_id: SyndicDraftId,
    session: DraftEditorCandidateSessionIdV1,
    operation: DraftPieceOperationIdV1,
) {
    let mut preceding = None;
    for step in 0..4096 {
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(store, draft_id, session, operation)
            .unwrap_or_else(|error| panic!("advance {step} after {preceding:?} failed: {error:?}"))
        else {
            return;
        };
        assert!(advance.staged_record_count() <= 256);
        preceding = Some(advance.frontier());
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    panic!("draft-piece build did not make bounded progress")
}

fn run_transaction(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
    _timestamp: u64,
) {
    begin_and_stage(storage, store, transaction);
    advance_until_complete(storage, store, transaction);
    committed(execute(
        store,
        storage.settle_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
}

fn exact_status(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> DraftPieceOperationStatusV1 {
    match storage
        .draft_piece_operation_status_page(store, &transaction.prepared, 1, &transaction.fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(status) => status,
        DraftPieceOperationVerificationV1::More { .. } => panic!("single test page was incomplete"),
    }
}

fn execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([171; 16]),
        RootId::from_bytes([172; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-phase136",
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
    let draft = SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]);
    let creation = CreateThread::ordinary(
        thread,
        draft,
        execution(),
        SyndicTimestamp::from_unix_millis(1),
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

fn current(storage: SyndicStorage, store: &HomeStore, thread: SyndicThreadId) -> CandidateCurrent {
    let durable = storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap();
    let session_id = DraftEditorCandidateSessionIdV1::from_bytes([0xC0; 16]);
    let session = match storage
        .draft_editor_candidate_session(store, durable.draft().id(), session_id)
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        DraftEditorCandidateSessionReadOutcomeV1::Absent => {
            let selector = DraftEditorCurrentSelectorV1::new(
                durable.thread().id(),
                durable.thread().revision(),
                durable.draft().id(),
                durable.draft().revision(),
                durable.draft().piece_root(),
            );
            let request = DraftEditorCandidateSessionOpenRequestV1::new(
                selector,
                session_id,
                DraftPieceOperationIdV1::from_bytes([0xC1; 16]),
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
    };
    CandidateCurrent {
        draft: CandidateDraft {
            id: durable.draft().id(),
            revision: durable.draft().revision(),
            root: session.newest_root(),
        },
        session,
    }
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}

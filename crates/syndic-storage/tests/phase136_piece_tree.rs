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
    AssetId, ExecutionBinding, ImageLabelOrdinal, PathFlavor, RootId, RuntimeId, RuntimeMode,
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
    DraftPieceMarkerDirectionV1, DraftPieceMarkerEffectChargesV1, DraftPieceMarkerEffectV1,
    DraftPieceMarkerInsertionV1, DraftPieceMarkerScopeV1, DraftPieceMarkerV1,
    DraftPieceOperationIdV1, DraftPieceOperationStatusV1, DraftPieceOperationVerificationV1,
    DraftPiecePrepareErrorV1, DraftPieceRejectedReasonV1, DraftPieceReplacementV1,
    DraftPieceSettlementOutcomeV1, DraftPieceTextDemandV1, DraftPieceV1, PreparedDraftPieceEditV1,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1, canonical_empty_draft_piece_root_v1,
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
    successor_positions: FixturePositions,
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
    if session.newest_history().frontier_revision() == 0 {
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

#[derive(Clone)]
struct CandidateCurrent {
    session: DraftEditorCandidateSessionV1,
    draft: CandidateDraft,
    positions: FixturePositions,
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
fn fresh_process_resumes_only_from_durable_head_fragments_and_records() {
    let (home, store, storage, thread) = fixture("durable-resume", 10);
    let base = current(&storage, &store, thread);
    let staged_transaction = transaction(
        &storage,
        &store,
        &base,
        11,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("recover".to_owned())],
        )],
        point(7),
    );
    begin_and_stage(&storage, &store, &staged_transaction);
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
        current(&storage, &store, thread).draft().piece_root(),
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
        &storage,
        &reopened,
        base.draft().id(),
        staged_session,
        staged_operation,
    );
    assert_eq!(
        current(&storage, &reopened, thread).draft().piece_root(),
        base.draft().piece_root()
    );

    let recovered = transaction(
        &storage,
        &reopened,
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
        exact_status(&storage, &reopened, &recovered),
        DraftPieceOperationStatusV1::Complete(_)
    ));
    committed(execute(
        &reopened,
        storage.settle_draft_piece_edit(
            storage.revision(&reopened).unwrap(),
            recovered.prepared.clone(),
        ),
    ));
    remember_settled_transaction(&storage, &reopened, &recovered);
    let root = current(&storage, &reopened, thread).draft().piece_root();
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
    let base = current(&storage, &store, thread);
    let accepted = transaction(
        &storage,
        &store,
        &base,
        31,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("x".to_owned())],
        )],
        point(1),
    );
    run_transaction(&storage, &store, &accepted, 2);
    let revision = storage.revision(&store).unwrap();
    assert!(matches!(
        exact_status(&storage, &store, &accepted),
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
        &storage,
        &store,
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
        exact_status(&storage, &store, &colliding),
        DraftPieceOperationStatusV1::Collision(_)
    ));
    assert_eq!(storage.revision(&store).unwrap(), before);
}

#[test]
fn settled_operation_replays_after_a_newer_build_replaces_the_draft_head() {
    let (_home, store, storage, thread) = fixture("settlement-head-replacement", 35);
    let base = current(&storage, &store, thread);
    let first = transaction(
        &storage,
        &store,
        &base,
        36,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("first".to_owned())],
        )],
        point(5),
    );
    run_transaction(&storage, &store, &first, 2);
    let first_settlement = exact_status(&storage, &store, &first);
    let next_base = current(&storage, &store, thread);
    let second = transaction(
        &storage,
        &store,
        &next_base,
        37,
        vec![DraftPieceReplacementV1::new(
            point(5),
            point(5),
            vec![DraftPieceV1::Text(" second".to_owned())],
        )],
        point(12),
    );
    begin_and_stage(&storage, &store, &second);
    assert!(matches!(
        exact_status(&storage, &store, &second),
        DraftPieceOperationStatusV1::Open(_)
    ));
    assert_eq!(exact_status(&storage, &store, &first), first_settlement);
}

#[cfg(feature = "test-faults")]
#[test]
fn settlement_replay_requires_its_exact_terminal_build_after_newer_builds() {
    let (_home, store, storage, thread) = fixture("terminal-build-deletion", 38);
    let base = current(&storage, &store, thread);
    let first = transaction(
        &storage,
        &store,
        &base,
        39,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("settled".to_owned())],
        )],
        point(7),
    );
    run_transaction(&storage, &store, &first, 2);
    let next_base = current(&storage, &store, thread);
    let second = transaction(
        &storage,
        &store,
        &next_base,
        40,
        vec![DraftPieceReplacementV1::new(
            point(7),
            point(7),
            vec![DraftPieceV1::Text(" head".to_owned())],
        )],
        point(12),
    );
    begin_and_stage(&storage, &store, &second);
    committed(execute(
        &store,
        delete_draft_piece_terminal_build(
            &store,
            &storage,
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
    let base = current(&storage, &store, thread);
    let rejected = transaction(
        &storage,
        &store,
        &base,
        51,
        vec![DraftPieceReplacementV1::new(point(0), point(0), Vec::new())],
        point(0),
    );
    begin_and_stage(&storage, &store, &rejected);
    committed(execute(
        &store,
        storage.reject_draft_piece_edit(
            storage.revision(&store).unwrap(),
            rejected.prepared.clone(),
            syndic_storage::DraftPieceRejectedReasonV1::TreeLimit,
        ),
    ));
    let DraftPieceOperationStatusV1::Settled(settlement) =
        exact_status(&storage, &store, &rejected)
    else {
        panic!("missing rejected settlement")
    };
    assert!(matches!(
        settlement.outcome(),
        DraftPieceSettlementOutcomeV1::Rejected(_)
    ));
    assert_eq!(
        current(&storage, &store, thread).draft().piece_root(),
        base.draft().piece_root()
    );

    let after_rejection = current(&storage, &store, thread);
    let failed = transaction(
        &storage,
        &store,
        &after_rejection,
        52,
        vec![DraftPieceReplacementV1::new(point(0), point(0), Vec::new())],
        point(0),
    );
    begin_and_stage(&storage, &store, &failed);
    committed(execute(
        &store,
        storage.error_draft_piece_edit(
            storage.revision(&store).unwrap(),
            failed.prepared.clone(),
            syndic_storage::DraftPieceErrorReasonV1::ResourceLimit,
        ),
    ));
    let DraftPieceOperationStatusV1::Settled(settlement) = exact_status(&storage, &store, &failed)
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
                &storage,
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
fn one_64k_text_piece_resumes_at_durable_byte_offsets() {
    let (_home, store, storage, thread) = fixture("intra-text-frontier", 97);
    let base = current(&storage, &store, thread);
    let seed = transaction(
        &storage,
        &store,
        &base,
        98,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("a".to_owned()); 129],
        )],
        point(129),
    );
    run_transaction(&storage, &store, &seed, 2);
    let base = current(&storage, &store, thread);
    assert!(base.draft().piece_root().summary().height() >= 2);
    let text = "x".repeat(65_536);
    let edit = transaction(
        &storage,
        &store,
        &base,
        99,
        vec![DraftPieceReplacementV1::new(
            point(129),
            point(129),
            vec![DraftPieceV1::Text(text.clone())],
        )],
        point(65_665),
    );
    begin_and_stage(&storage, &store, &edit);
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
    remember_settled_transaction(&storage, &store, &edit);
    let root = current(&storage, &store, thread).draft().piece_root();
    assert_eq!(root.summary().logical_utf8_bytes(), 65_665);
    assert_eq!(
        storage
            .draft_piece_text_demand(&store, root, DraftPieceTextDemandV1::Forward(65_649), 16,)
            .unwrap()
            .bytes(),
        &text.as_bytes()[65_520..]
    );
}

#[test]
fn staged_record_diagnostic_includes_marker_order_record_and_root() {
    let (_home, store, storage, thread) = fixture("staged-marker-order-diagnostic", 100);
    let seed = transaction(
        &storage,
        &store,
        &current(&storage, &store, thread),
        101,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("x".to_owned())],
        )],
        point(1),
    );
    run_transaction(&storage, &store, &seed, 101);
    let base = current(&storage, &store, thread);
    let marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([0xA5; 16]),
        1,
        ImageLabelOrdinal::new(1).unwrap(),
        AssetId::sha256_v1([0x5A; 32], std::num::NonZeroU64::new(1).unwrap()),
    );
    let edit = transaction(
        &storage,
        &store,
        &base,
        102,
        vec![
            DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Marker(marker)])
                .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        0,
                        marker,
                        DraftPieceMarkerEffectChargesV1::for_marker(marker),
                    ),
                )),
        ],
        DraftCompositePositionV1::new(0, DraftCompositeGapWitnessV1::BeforeAll),
    );
    begin_and_stage(&storage, &store, &edit);
    let mut staged_counts = Vec::new();
    for _ in 0..8 {
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
        staged_counts.push(advance.staged_record_count());
        committed(execute(
            &store,
            storage.advance_draft_piece_edit(storage.revision(&store).unwrap(), advance),
        ));
    }
    assert_eq!(staged_counts, [1, 1, 1, 7, 1]);
}

#[cfg(feature = "test-faults")]
#[test]
fn candidate_read_detects_selector_drift_after_exact_traversal() {
    let (_home, store, storage, thread) = fixture("current-drift", 140);
    let base = current(&storage, &store, thread);
    let initial = transaction(
        &storage,
        &store,
        &base,
        141,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("before".to_owned())],
        )],
        point(6),
    );
    run_transaction(&storage, &store, &initial, 30);
    let base = current(&storage, &store, thread);
    let successor = transaction(
        &storage,
        &store,
        &base,
        142,
        vec![DraftPieceReplacementV1::new(
            point(6),
            point(6),
            vec![DraftPieceV1::Text("after".to_owned())],
        )],
        point(11),
    );
    begin_and_stage(&storage, &store, &successor);
    advance_until_complete(&storage, &store, &successor);
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
    remember_settled_transaction(&storage, &store, &successor);
    assert_eq!(
        storage
            .draft_piece_text_demand(
                &store,
                current(&storage, &store, thread).draft().piece_root(),
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
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let base = current(&storage, &store, thread);
    let transaction = transaction(
        &storage,
        &store,
        &base,
        152,
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("custody".to_owned())],
        )],
        point(7),
    );
    begin_and_stage(&storage, &store, &transaction);
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
    advance_until_complete(&storage, &store, &transaction);

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

fn transaction(
    storage: &SyndicStorage,
    store: &HomeStore,
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
        current.session.newest_history(),
        operation,
        current.positions.caret,
        current.positions.selection,
        caret,
        caret,
        replacements.len() as u64,
        chain,
    );
    let prepared = storage
        .prepare_draft_piece_edit(store, header, &current.session)
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
        successor_positions: FixturePositions {
            caret,
            selection: caret,
        },
    }
}

fn begin_and_stage(storage: &SyndicStorage, store: &HomeStore, transaction: &Transaction) {
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

fn advance_until_complete(storage: &SyndicStorage, store: &HomeStore, transaction: &Transaction) {
    advance_until_complete_for(
        storage,
        store,
        transaction.prepared.header().draft_id(),
        transaction.session,
        transaction.operation,
    );
}

fn advance_until_complete_for(
    storage: &SyndicStorage,
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
    storage: &SyndicStorage,
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
    remember_settled_transaction(storage, store, transaction);
}

fn remember_settled_transaction(
    storage: &SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) {
    match exact_status(storage, store, transaction) {
        DraftPieceOperationStatusV1::Settled(settlement) => match settlement.closure() {
            syndic_storage::DraftPieceSettlementClosureV1::Committed(adoption) => {
                remember_fixture_positions(
                    adoption.adopted_session(),
                    transaction.successor_positions,
                );
            }
            syndic_storage::DraftPieceSettlementClosureV1::Noncommit(_) => {}
        },
        other => panic!("settled transaction has unexpected status: {other:?}"),
    }
}

fn exact_status(
    storage: &SyndicStorage,
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

fn current(storage: &SyndicStorage, store: &HomeStore, thread: SyndicThreadId) -> CandidateCurrent {
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
                durable.draft().history(),
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
        positions: fixture_positions(&session),
        session,
    }
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}

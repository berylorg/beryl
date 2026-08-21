use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    ExecutionBinding, ImageLabelOrdinal, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicDraftMarkerId, SyndicThreadId,
};
use syndic_storage::{
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftCompositeSearchKeyV1,
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftEditorCandidateSessionV1,
    DraftEditorCurrentSelectorV1, DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1,
    DraftPieceMalformedRangeRequestV1, DraftPieceMarkerAtV1, DraftPieceMarkerDemandV1,
    DraftPieceMarkerDirectionV1, DraftPieceMarkerEdgeProofRequestV1, DraftPieceMarkerEdgeProofV1,
    DraftPieceMarkerScopeV1, DraftPieceMarkerV1, DraftPieceOperationIdV1,
    DraftPieceRangeSourceErrorV1, DraftPieceReplacementV1, DraftPieceTextDemandV1,
    DraftPieceTextEdgeFactV1, DraftPieceV1, PreparedDraftPieceEditV1, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1,
};

#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{
    DraftPieceDescendantCorruption, DraftPieceDescendantTarget, DraftPieceImmutableDeletion,
    arm_draft_piece_candidate_read_fault, arm_draft_piece_current_read_fault,
    delete_draft_piece_immutable_record, inject_draft_piece_descendant_corruption,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase140-{name}-{}-{}",
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

#[derive(Clone)]
struct CandidateCurrent {
    durable: syndic_storage::SyndicCurrentDraft,
    session: DraftEditorCandidateSessionV1,
    draft: CandidateDraft,
    positions: FixturePositions,
}

#[derive(Clone, Copy)]
struct CandidateDraft {
    id: SyndicDraftId,
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

    const fn piece_root(&self) -> syndic_storage::DraftPieceRootReferenceV1 {
        self.root
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn newline_and_logical_line_corruption_fail_closed_independently() {
    for (name, corruption) in [
        (
            "newline-corrupt",
            DraftPieceDescendantCorruption::NewlineAggregate,
        ),
        (
            "line-corrupt",
            DraftPieceDescendantCorruption::LogicalLineAggregate,
        ),
    ] {
        let (_home, store, storage, thread) = fixture(name, 60);
        let initial = current(storage, &store, thread);
        let text_pieces = (0..130_u16)
            .map(|_| DraftPieceV1::Text("x\n".to_owned()))
            .collect();
        let transaction = transaction(
            storage,
            &store,
            &initial,
            61,
            62,
            vec![DraftPieceReplacementV1::new(
                point(0),
                point(0),
                text_pieces,
            )],
            point(260),
        );
        run_transaction(storage, &store, &transaction, 2);
        let root = current(storage, &store, thread).draft().piece_root();
        committed(execute(
            &store,
            inject_draft_piece_descendant_corruption(
                &store,
                storage,
                root,
                DraftPieceDescendantTarget::Sequence,
                corruption,
            ),
        ));
        assert!(matches!(
            storage.draft_piece_text_demand(
                &store,
                root,
                DraftPieceTextDemandV1::Forward(0),
                65_536,
            ),
            Err(DraftPieceRangeSourceErrorV1::Invariant)
        ));
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn missing_root_and_non_root_records_are_absent_through_all_selectors() {
    for (case, deletion) in [
        DraftPieceImmutableDeletion::Root,
        DraftPieceImmutableDeletion::SequenceDescendant,
    ]
    .into_iter()
    .enumerate()
    {
        let (_home, store, storage, thread) = fixture("missing-immutable", 70 + case as u8);
        let initial = current(storage, &store, thread);
        let pieces = (0..130)
            .map(|_| DraftPieceV1::Text("x".to_owned()))
            .collect();
        let transaction = transaction(
            storage,
            &store,
            &initial,
            72 + case as u8,
            74 + case as u8,
            vec![DraftPieceReplacementV1::new(point(0), point(0), pieces)],
            point(130),
        );
        run_transaction(storage, &store, &transaction, 2);
        let populated = current(storage, &store, thread);
        let root = populated.draft().piece_root();
        let request = DraftEditorCandidateSessionOpenRequestV1::new(
            selector(&populated),
            DraftEditorCandidateSessionIdV1::from_bytes([80 + case as u8; 16]),
            DraftPieceOperationIdV1::from_bytes([82 + case as u8; 16]),
        );
        let prepared = storage
            .prepare_open_draft_editor_candidate_session(&store, request)
            .unwrap();
        let outcome = execute(
            &store,
            storage.open_draft_editor_candidate_session(
                storage.revision(&store).unwrap(),
                prepared.clone(),
            ),
        );
        let DraftEditorCandidateSessionOpenOutcomeV1::Opened(_head) = storage
            .reconcile_draft_editor_candidate_session_open(&store, &prepared, outcome)
            .unwrap()
        else {
            panic!("fixture session did not open")
        };
        let binding = DraftEditorCandidateActivationBindingV1::from_head(&populated.session);
        committed(execute(
            &store,
            delete_draft_piece_immutable_record(&store, storage, root, deletion),
        ));
        assert!(matches!(
            storage.draft_piece_text_demand(
                &store,
                root,
                DraftPieceTextDemandV1::Forward(0),
                65_536,
            ),
            Err(DraftPieceRangeSourceErrorV1::Absent)
        ));
        assert!(
            storage
                .current_draft_piece_text_demand(
                    &store,
                    thread,
                    DraftPieceTextDemandV1::Forward(0),
                    65_536,
                )
                .unwrap()
                .unwrap()
                .value()
                .bytes()
                .is_empty()
        );
        let candidate = storage.candidate_draft_piece_text_demand(
            &store,
            binding,
            DraftPieceTextDemandV1::Forward(0),
            65_536,
        );
        assert!(match deletion {
            DraftPieceImmutableDeletion::Root | DraftPieceImmutableDeletion::RootNode => {
                matches!(candidate, Err(DraftPieceRangeSourceErrorV1::Invariant))
            }
            DraftPieceImmutableDeletion::SequenceDescendant => {
                matches!(candidate, Err(DraftPieceRangeSourceErrorV1::Absent))
            }
            DraftPieceImmutableDeletion::Settlement => unreachable!(),
        });
    }
}

#[cfg(feature = "test-faults")]
#[test]
fn stale_session_candidate_and_disposed_open_are_typed() {
    let (_home, store, storage, thread) = fixture("stale", 50);
    let current = current(storage, &store, thread);
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        selector(&current),
        DraftEditorCandidateSessionIdV1::from_bytes([95; 16]),
        DraftPieceOperationIdV1::from_bytes([96; 16]),
    );
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage.open_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    let DraftEditorCandidateSessionOpenOutcomeV1::Opened(head) = storage
        .reconcile_draft_editor_candidate_session_open(&store, &prepared, outcome)
        .unwrap()
    else {
        panic!("session did not open")
    };
    let stale_session = DraftEditorCandidateActivationBindingV1::new(
        head.draft_id(),
        head.session_id(),
        head.session_generation() + 1,
        head.newest_candidate_generation(),
        head.newest_root(),
        head.newest_history(),
        head.logical_extent(),
    );
    assert!(matches!(
        storage.candidate_draft_piece_text_demand(
            &store,
            stale_session,
            DraftPieceTextDemandV1::Forward(0),
            4,
        ),
        Err(DraftPieceRangeSourceErrorV1::StaleSession)
    ));
    let stale_candidate = DraftEditorCandidateActivationBindingV1::new(
        head.draft_id(),
        head.session_id(),
        head.session_generation(),
        head.newest_candidate_generation() + 1,
        head.newest_root(),
        head.newest_history(),
        head.logical_extent(),
    );
    assert!(matches!(
        storage.candidate_draft_piece_text_demand(
            &store,
            stale_candidate,
            DraftPieceTextDemandV1::Forward(0),
            4,
        ),
        Err(DraftPieceRangeSourceErrorV1::StaleCandidate)
    ));
    committed(execute(
        &store,
        storage.test_dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            head.draft_id(),
            head.session_id(),
        ),
    ));
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, head.draft_id(), head.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
    ));
    let replay = storage
        .prepare_open_draft_editor_candidate_session(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage
            .open_draft_editor_candidate_session(storage.revision(&store).unwrap(), replay.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_session_open(&store, &replay, outcome)
            .unwrap(),
        DraftEditorCandidateSessionOpenOutcomeV1::StaleDisposed(_)
    ));
    assert!(matches!(
        storage.candidate_draft_piece_text_demand(
            &store,
            DraftEditorCandidateActivationBindingV1::from_head(&head),
            DraftPieceTextDemandV1::Forward(0),
            4,
        ),
        Err(DraftPieceRangeSourceErrorV1::Disposed(_))
    ));
}

fn marker(seed: u8, order: u64) -> DraftPieceMarkerV1 {
    let mut id = [seed; 16];
    id[0] = seed;
    id[1..9].copy_from_slice(&order.to_be_bytes());
    DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes(id),
        order,
        ImageLabelOrdinal::new(order + 1).unwrap(),
    )
}

fn transaction(
    storage: SyndicStorage,
    store: &HomeStore,
    current: &CandidateCurrent,
    _session_seed: u8,
    operation_seed: u8,
    replacements: Vec<DraftPieceReplacementV1>,
    caret: DraftCompositePositionV1,
) -> Transaction {
    let session = current.session.session_id();
    let operation = DraftPieceOperationIdV1::from_bytes([operation_seed; 16]);
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

fn run_transaction(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
    _timestamp: u64,
) {
    stage_and_build(storage, store, transaction);
    committed(execute(
        store,
        storage.settle_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
    match storage
        .draft_piece_operation_status_page(store, &transaction.prepared, 1, &transaction.fragments)
        .unwrap()
    {
        syndic_storage::DraftPieceOperationVerificationV1::Status(
            syndic_storage::DraftPieceOperationStatusV1::Settled(settlement),
        ) => match settlement.closure() {
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

fn stage_and_build(storage: SyndicStorage, store: &HomeStore, transaction: &Transaction) {
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
    let mut previous = None;
    for step in 0..4096 {
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                store,
                transaction.prepared.header().draft_id(),
                transaction.session,
                transaction.operation,
            )
            .unwrap_or_else(|error| panic!("advance {step} after {previous:?} failed: {error:?}"))
        else {
            break;
        };
        previous = Some(advance.frontier());
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
}

fn execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([171; 16]),
        RootId::from_bytes([172; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\syndic-phase140",
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
    (home, store, storage, thread)
}

fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn committed(outcome: CommandOutcome) {
    assert!(matches!(
        outcome,
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn current(storage: SyndicStorage, store: &HomeStore, thread: SyndicThreadId) -> CandidateCurrent {
    let durable = storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap();
    let session_id = DraftEditorCandidateSessionIdV1::from_bytes([0xD0; 16]);
    let session = match storage
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
                DraftPieceOperationIdV1::from_bytes([0xD1; 16]),
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
            root: session.newest_root(),
        },
        durable,
        positions: fixture_positions(&session),
        session,
    }
}

fn selector(current: &CandidateCurrent) -> DraftEditorCurrentSelectorV1 {
    DraftEditorCurrentSelectorV1::new(
        current.durable.thread().id(),
        current.durable.thread().revision(),
        current.durable.draft().id(),
        current.durable.draft().revision(),
        current.durable.draft().piece_root(),
        current.durable.draft().history(),
    )
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}

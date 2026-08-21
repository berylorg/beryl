pub(super) use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) use beryl_home_store::test_faults::{FaultController, FaultPoint};
pub(super) use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
pub(super) use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId, ThreadRevision,
};
pub(super) use syndic_storage::{
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1,
    DraftEditHistoryAppendErrorV1, DraftEditHistoryPolicyV1,
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftEditorCandidateSessionV1,
    DraftEditorCurrentSelectorV1, DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1,
    DraftPieceErrorReasonV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerEdgeProofRequestV1, DraftPieceMarkerScopeV1, DraftPieceOperationIdV1,
    DraftPieceOperationStatusV1, DraftPieceOperationVerificationV1, DraftPiecePrepareErrorV1,
    DraftPieceRangeSourceErrorV1, DraftPieceReconciledCommandV1, DraftPieceRejectedReasonV1,
    DraftPieceReplacementV1, DraftPieceSettlementClosureV1, DraftPieceSettlementOutcomeV1,
    DraftPieceSettlementProofV1, DraftPieceTextDemandV1, DraftPieceTransactionOutcomeV1,
    DraftPieceV1, PreparedDraftPieceEditV1, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    canonical_draft_piece_fragment_chain_v1, canonical_empty_draft_edit_history_v1,
    canonical_empty_draft_piece_fragment_chain_v1, canonical_empty_draft_piece_root_v1,
    test_faults::{
        DraftEditHistoryRecordDeletion, DraftPieceImmutableDeletion,
        alternative_ordinary_draft_edit_history, delete_draft_edit_history_frontier,
        delete_draft_edit_history_record, delete_draft_piece_immutable_record,
        draft_edit_history_accounting_corruption, draft_edit_history_availability_corruption,
        draft_edit_history_first_transition_gap, draft_edit_history_no_head_gap,
        draft_edit_history_overflow_errors, draft_edit_history_root_exists,
        draft_edit_history_stored_charge_components, draft_edit_history_transition_exists,
        draft_edit_history_wrong_head_root, inject_draft_edit_history_frontier_digest_corruption,
        inject_draft_piece_settlement_closure_corruption,
        occupy_canonical_empty_draft_edit_history, publish_draft_edit_history_pair,
        replace_draft_edit_history_frontier, replace_draft_edit_history_frontier_and_session,
        replace_draft_edit_history_transition, syndic_v5_family_names,
    },
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

pub(super) struct TestHome(pub(super) PathBuf);

impl TestHome {
    pub(super) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase146-{name}-{}-{}",
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
pub(super) struct Transaction {
    pub(super) prepared: PreparedDraftPieceEditV1,
    pub(super) fragments: Vec<DraftPieceBuildFragmentV1>,
}

pub(super) fn open(home: &TestHome) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap()
}

pub(super) fn fixture(
    name: &str,
    seed: u8,
    budget: u64,
) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(storage, &store, seed, budget);
    (home, store, storage, thread)
}

pub(super) fn fault_fixture(
    name: &str,
    seed: u8,
    budget: u64,
) -> (
    TestHome,
    HomeStore,
    SyndicStorage,
    FaultController,
    SyndicThreadId,
) {
    let home = TestHome::new(name);
    let faults = FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = create_thread(storage, &store, seed, budget);
    (home, store, storage, faults, thread)
}

pub(super) fn create_request(seed: u8, budget: u64) -> CreateThread {
    CreateThread::ordinary(
        SyndicThreadId::from_bytes([seed; 16]),
        SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
        ExecutionBinding::new(
            RuntimeId::from_bytes([171; 16]),
            RootId::from_bytes([172; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                "C:\\phase146",
            )
            .unwrap(),
        ),
        SyndicTimestamp::from_unix_millis(1),
        DraftEditHistoryPolicyV1::new(budget, 1).unwrap(),
    )
}

pub(super) fn create_thread(
    storage: SyndicStorage,
    store: &HomeStore,
    seed: u8,
    budget: u64,
) -> SyndicThreadId {
    let request = create_request(seed, budget);
    let thread = request.thread_id();
    committed(execute(
        store,
        storage.create_thread(storage.revision(store).unwrap(), request),
    ));
    thread
}

pub(super) fn current(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
) -> syndic_storage::SyndicCurrentDraft {
    storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
}

pub(super) fn selector(
    current: &syndic_storage::SyndicCurrentDraft,
) -> DraftEditorCurrentSelectorV1 {
    DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    )
}

pub(super) fn open_request(
    current: &syndic_storage::SyndicCurrentDraft,
    session: u8,
    operation: u8,
) -> DraftEditorCandidateSessionOpenRequestV1 {
    DraftEditorCandidateSessionOpenRequestV1::new(
        selector(current),
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
    )
}

pub(super) fn open_session(
    storage: SyndicStorage,
    store: &HomeStore,
    current: &syndic_storage::SyndicCurrentDraft,
    session: u8,
    operation: u8,
) -> DraftEditorCandidateSessionV1 {
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(
            store,
            open_request(current, session, operation),
        )
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
        other => panic!("session did not open: {other:?}"),
    }
}

pub(super) fn transaction(
    storage: SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    text: &str,
    successor: DraftCompositePositionV1,
) -> Transaction {
    transaction_with_positions(
        storage,
        store,
        session,
        operation,
        text,
        point(0),
        point(0),
        successor,
        successor,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn transaction_with_positions(
    storage: SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    text: &str,
    predecessor_caret: DraftCompositePositionV1,
    predecessor_selection: DraftCompositePositionV1,
    successor_caret: DraftCompositePositionV1,
    successor_selection: DraftCompositePositionV1,
) -> Transaction {
    let replacements = vec![DraftPieceReplacementV1::new(
        point(0),
        point(0),
        vec![DraftPieceV1::Text(text.to_owned())],
    )];
    let chain = canonical_draft_piece_fragment_chain_v1(&replacements);
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        predecessor_caret,
        predecessor_selection,
        successor_caret,
        successor_selection,
        replacements.len() as u64,
        chain,
    );
    let prepared = storage
        .prepare_draft_piece_edit(store, header, session)
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
        prepared,
        fragments,
    }
}

pub(super) fn build(storage: SyndicStorage, store: &HomeStore, transaction: &Transaction) {
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
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            transaction.prepared.header().draft_id(),
            transaction.prepared.header().session_id(),
            transaction.prepared.header().operation_id(),
        )
        .unwrap()
    {
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
}

pub(super) fn settled(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> syndic_storage::DraftPieceSettlementV1 {
    match storage
        .draft_piece_operation_status_page(store, &transaction.prepared, 1, &transaction.fragments)
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Settled(value)) => {
            value
        }
        other => panic!("operation is not settled: {other:?}"),
    }
}

pub(super) fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

pub(super) fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "command was not committed: {outcome:?}"
    );
}

pub(super) fn not_committed(outcome: CommandOutcome) {
    assert!(matches!(outcome, CommandOutcome::NotCommitted { .. }));
}

pub(super) fn replay_succeeded(outcome: CommandOutcome) {
    assert!(matches!(
        outcome,
        CommandOutcome::NotCommitted { .. }
            | CommandOutcome::Committed {
                later_failure: None,
                ..
            }
    ));
}

pub(super) fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}

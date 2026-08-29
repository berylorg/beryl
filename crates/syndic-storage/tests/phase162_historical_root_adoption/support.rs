use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    DomainRevision, ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicThreadId,
};
use syndic_storage::{
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftEditHistoryPolicyV1,
    DraftEditorCandidateActivationBindingV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
    DraftEditorCandidateSessionV1, DraftEditorCurrentSelectorV1, DraftHistoricalRootDirectionV1,
    DraftHistoricalRootSelectionIntentV1, DraftHistoricalRootSelectionV1,
    DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1, DraftPieceOperationIdV1,
    DraftPieceReplacementV1, DraftPieceV1, PreparedDraftHistoricalRootAdoptionV1,
    PreparedDraftPieceEditV1, SyndicCurrentDraft, SyndicPointReadLimit, SyndicStorage,
    SyndicTimestamp, canonical_draft_piece_fragment_chain_v1,
    canonical_empty_draft_piece_fragment_chain_v1,
};

#[cfg(feature = "test-faults")]
use beryl_home_store::test_faults::FaultController;

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

pub struct TestHome(pub PathBuf);

impl TestHome {
    pub fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase162-{name}-{}-{}",
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
pub struct Transaction {
    pub prepared: PreparedDraftPieceEditV1,
    pub fragments: Vec<DraftPieceBuildFragmentV1>,
}

pub fn open(home: &TestHome) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap()
}

pub fn fixture(name: &str, seed: u8) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    fixture_with_history_budget(name, seed, 16_384)
}

pub fn fixture_with_history_budget(
    name: &str,
    seed: u8,
    history_budget: u64,
) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store = open(&home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let request = CreateThread::ordinary(
        thread,
        SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
        ExecutionBinding::new(
            RuntimeId::from_bytes([171; 16]),
            RootId::from_bytes([172; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                "C:\\phase162",
            )
            .unwrap(),
        ),
        SyndicTimestamp::from_unix_millis(1),
        DraftEditHistoryPolicyV1::new(history_budget, 1).unwrap(),
    );
    committed(execute(
        &store,
        storage.create_thread(storage.revision(&store).unwrap(), request),
    ));
    (home, store, storage, thread)
}

#[cfg(feature = "test-faults")]
pub fn fault_fixture(
    name: &str,
    seed: u8,
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
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    let request = CreateThread::ordinary(
        thread,
        SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
        ExecutionBinding::new(
            RuntimeId::from_bytes([171; 16]),
            RootId::from_bytes([172; 16]),
            RuntimeNativePath::from_admitted(
                RuntimeMode::host(),
                PathFlavor::Windows,
                "C:\\phase162",
            )
            .unwrap(),
        ),
        SyndicTimestamp::from_unix_millis(1),
        DraftEditHistoryPolicyV1::new(16_384, 1).unwrap(),
    );
    committed(execute(
        &store,
        storage.create_thread(storage.revision(&store).unwrap(), request),
    ));
    (home, store, storage, faults, thread)
}

pub fn reopen(home: &TestHome, store: HomeStore) -> (HomeStore, SyndicStorage) {
    drop(store);
    let mut store = open(home);
    let storage = SyndicStorage::register(&mut store).unwrap();
    (store, storage)
}

pub fn current(
    storage: &SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
) -> SyndicCurrentDraft {
    storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
}

pub fn open_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    current: &SyndicCurrentDraft,
    session: u8,
    operation: u8,
) -> DraftEditorCandidateSessionV1 {
    let selector = DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    );
    let prepared = storage
        .prepare_open_draft_editor_candidate_session(
            store,
            DraftEditorCandidateSessionOpenRequestV1::new(
                selector,
                DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
                operation_id(operation),
            ),
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
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(session)
        | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(session) => session,
        value => panic!("unexpected session outcome: {value:?}"),
    }
}

pub fn transaction(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    text: &str,
    before: DraftCompositePositionV1,
    after: DraftCompositePositionV1,
) -> Transaction {
    let replacements = vec![DraftPieceReplacementV1::new(
        point(0),
        point(0),
        vec![DraftPieceV1::Text(text.to_owned())],
    )];
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        operation_id(operation),
        before,
        before,
        after,
        after,
        replacements.len() as u64,
        canonical_draft_piece_fragment_chain_v1(&replacements),
    );
    let prepared = storage
        .prepare_draft_piece_edit(store, header, session)
        .unwrap();
    let mut chain = canonical_empty_draft_piece_fragment_chain_v1();
    let fragments = replacements
        .into_iter()
        .enumerate()
        .map(|(index, replacement)| {
            let fragment = storage
                .prepare_draft_piece_fragment(&prepared, index as u64 + 1, chain, replacement)
                .unwrap();
            chain = fragment.chain_digest();
            fragment
        })
        .collect();
    Transaction {
        prepared,
        fragments,
    }
}

pub fn settle(
    storage: &SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
) -> syndic_storage::DraftPieceSettlementV1 {
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
            syndic_storage::DraftPieceOperationStatusV1::Settled(value),
        ) => value,
        value => panic!("transaction not settled: {value:?}"),
    }
}

pub fn operation_id(value: u8) -> DraftPieceOperationIdV1 {
    DraftPieceOperationIdV1::from_bytes([value; 16])
}

pub fn historical_selection_intent(
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    direction: DraftHistoricalRootDirectionV1,
) -> DraftHistoricalRootSelectionIntentV1 {
    DraftHistoricalRootSelectionIntentV1::new(
        DraftEditorCandidateActivationBindingV1::from_head(session),
        operation_id(operation),
        direction,
    )
}

pub fn prepare_historical_selection(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    direction: DraftHistoricalRootDirectionV1,
) -> PreparedDraftHistoricalRootAdoptionV1 {
    match storage
        .prepare_draft_historical_root_selection(
            store,
            historical_selection_intent(session, operation, direction),
        )
        .unwrap()
    {
        DraftHistoricalRootSelectionV1::Prepared(prepared) => prepared,
        DraftHistoricalRootSelectionV1::Unavailable => {
            panic!("historical direction unexpectedly unavailable")
        }
    }
}

pub fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}

pub fn execute(store: &HomeStore, contribution: MutationContribution) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

pub fn committed(outcome: CommandOutcome) {
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

pub fn revision(storage: &SyndicStorage, store: &HomeStore) -> DomainRevision {
    storage.revision(store).unwrap()
}

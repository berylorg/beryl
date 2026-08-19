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
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1,
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionOpenOutcomeV1,
    DraftEditorCandidateSessionOpenRequestV1, DraftEditorCandidateSessionV1,
    DraftEditorCurrentSelectorV1, DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1,
    DraftPieceMarkerV1, DraftPieceOperationIdV1, DraftPieceReplacementV1, DraftPieceV1,
    PreparedDraftPieceEditV1, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    canonical_draft_piece_fragment_chain_v1, canonical_empty_draft_piece_fragment_chain_v1,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

pub struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-app-phase141-{name}-{}-{}",
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
    session: DraftEditorCandidateSessionV1,
    operation: DraftPieceOperationIdV1,
    prepared: PreparedDraftPieceEditV1,
    fragments: Vec<DraftPieceBuildFragmentV1>,
}

pub fn fixture(name: &str, seed: u8) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
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
            ),
        ),
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
    SyndicThreadId,
    beryl_home_store::test_faults::FaultController,
) {
    let home = TestHome::new(name);
    let faults = beryl_home_store::test_faults::FaultController::new();
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
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
            ),
        ),
    ));
    (home, store, storage, thread, faults)
}

pub fn populate(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    session_seed: u8,
) -> (DraftPieceMarkerV1, DraftPieceMarkerV1) {
    let current = current(storage, store, thread);
    let left = marker(session_seed.wrapping_add(1), 1);
    let right = marker(session_seed.wrapping_add(2), 2);
    let transaction = transaction(
        storage,
        store,
        &current,
        session_seed,
        session_seed.wrapping_add(10),
        vec![DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![
                DraftPieceV1::Text("\u{03b1}\n".to_owned()),
                DraftPieceV1::Marker(left),
                DraftPieceV1::Marker(right),
                DraftPieceV1::Text("\u{03b2}\n".to_owned()),
            ],
        )],
        point(0),
    );
    run_transaction(storage, store, &transaction, 2);
    (left, right)
}

pub fn transaction(
    storage: SyndicStorage,
    store: &HomeStore,
    current: &syndic_storage::SyndicCurrentDraft,
    session_seed: u8,
    operation_seed: u8,
    replacements: Vec<DraftPieceReplacementV1>,
    caret: DraftCompositePositionV1,
) -> Transaction {
    let session = open_session(storage, store, current, session_seed, operation_seed);
    transaction_for_session(
        storage,
        session,
        operation_seed.wrapping_add(1),
        replacements,
        caret,
    )
}

pub fn transaction_for_session(
    storage: SyndicStorage,
    session: DraftEditorCandidateSessionV1,
    operation_seed: u8,
    replacements: Vec<DraftPieceReplacementV1>,
    caret: DraftCompositePositionV1,
) -> Transaction {
    let operation = DraftPieceOperationIdV1::from_bytes([operation_seed; 16]);
    let chain = canonical_draft_piece_fragment_chain_v1(&replacements);
    let header = DraftPieceEditHeaderV1::new(
        session.draft_id(),
        session.session_id(),
        session.newest_candidate_generation(),
        session.newest_root(),
        operation,
        caret,
        caret,
        replacements.len() as u64,
        chain,
    );
    let prepared = storage.prepare_draft_piece_edit(header, &session).unwrap();
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

pub fn run_transaction(
    storage: SyndicStorage,
    store: &HomeStore,
    transaction: &Transaction,
    _timestamp: u64,
) {
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
    for _ in 0..4096 {
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                store,
                transaction.prepared.header().draft_id(),
                transaction.session.session_id(),
                transaction.operation,
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
        storage.settle_draft_piece_edit(
            storage.revision(store).unwrap(),
            transaction.prepared.clone(),
        ),
    ));
}

fn open_session(
    storage: SyndicStorage,
    store: &HomeStore,
    current: &syndic_storage::SyndicCurrentDraft,
    session_seed: u8,
    operation_seed: u8,
) -> DraftEditorCandidateSessionV1 {
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        DraftEditorCurrentSelectorV1::new(
            current.thread().id(),
            current.thread().revision(),
            current.draft().id(),
            current.draft().revision(),
            current.draft().piece_root(),
        ),
        DraftEditorCandidateSessionIdV1::from_bytes([session_seed; 16]),
        DraftPieceOperationIdV1::from_bytes([operation_seed; 16]),
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

pub fn current(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
) -> syndic_storage::SyndicCurrentDraft {
    storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
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
            &outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "command did not commit cleanly: {outcome:?}"
    );
}

fn marker(seed: u8, order: u64) -> DraftPieceMarkerV1 {
    let mut id = [seed; 16];
    id[1..9].copy_from_slice(&order.to_be_bytes());
    DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes(id),
        order,
        ImageLabelOrdinal::new(order + 1).unwrap(),
    )
}

fn execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([171; 16]),
        RootId::from_bytes([172; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\beryl-app-phase141",
        )
        .unwrap(),
    )
}

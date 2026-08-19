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
    DraftEditorCandidateSessionIdV1, DraftPieceBuildFragmentV1, DraftPieceEditHeaderV1,
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
    session: DraftEditorCandidateSessionIdV1,
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
    current: &syndic_storage::SyndicCurrentDraft,
    session_seed: u8,
    operation_seed: u8,
    replacements: Vec<DraftPieceReplacementV1>,
    caret: DraftCompositePositionV1,
) -> Transaction {
    let session = DraftEditorCandidateSessionIdV1::from_bytes([session_seed; 16]);
    let operation = DraftPieceOperationIdV1::from_bytes([operation_seed; 16]);
    let chain = canonical_draft_piece_fragment_chain_v1(&replacements);
    let header = DraftPieceEditHeaderV1::new(
        current.draft().id(),
        session,
        current.draft().revision(),
        current.draft().piece_root(),
        operation,
        caret,
        caret,
        replacements.len() as u64,
        chain,
    );
    let prepared = storage.prepare_draft_piece_edit(header).unwrap();
    let mut preceding = canonical_empty_draft_piece_fragment_chain_v1();
    let fragments = replacements
        .into_iter()
        .enumerate()
        .map(|(ordinal, replacement)| {
            let fragment = storage
                .prepare_draft_piece_fragment(&prepared, ordinal as u64, preceding, replacement)
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
    timestamp: u64,
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
                transaction.session,
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
            SyndicTimestamp::from_unix_millis(timestamp),
        ),
    ));
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
    assert!(matches!(
        outcome,
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
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

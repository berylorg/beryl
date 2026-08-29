use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    MutationContribution,
};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use syndic_storage::{
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1,
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionOpenOutcomeV1,
    DraftEditorCandidateSessionOpenRequestV1, DraftEditorCandidateSessionV1,
    DraftEditorCurrentSelectorV1, DraftMutationBeginV1, DraftMutationOperationIdV1,
    DraftMutationStagingHeadV1, DraftMutationStagingIdentityV1, DraftMutationStagingLaneV1,
    DraftMutationStagingPageInputV1, DraftMutationStagingPageItemV1, DraftPieceOperationIdV1,
    DraftPieceReplacementV1, DraftPieceV1, PreparedDraftMutationStagingBatchV1, SyndicCurrentDraft,
    SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

pub struct TestHome(pub PathBuf);

impl TestHome {
    pub fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase152-{name}-{}-{}",
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

pub struct ReceivingFixture {
    pub home: TestHome,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub session: DraftEditorCandidateSessionV1,
    pub identity: DraftMutationStagingIdentityV1,
    pub head: DraftMutationStagingHeadV1,
}

pub fn receiving_fixture(name: &str, seed: u8) -> ReceivingFixture {
    let home = TestHome::new(name);
    let store = HomeStore::open(HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT)).unwrap();
    receiving_fixture_in(home, store, seed)
}

#[cfg(feature = "test-faults")]
pub fn receiving_fault_fixture(
    name: &str,
    seed: u8,
    faults: beryl_home_store::test_faults::FaultController,
) -> ReceivingFixture {
    let home = TestHome::new(name);
    let store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    receiving_fixture_in(home, store, seed)
}

fn receiving_fixture_in(home: TestHome, mut store: HomeStore, seed: u8) -> ReceivingFixture {
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
                ExecutionBinding::new(
                    RuntimeId::from_bytes([seed.wrapping_add(2); 16]),
                    RootId::from_bytes([seed.wrapping_add(3); 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase152",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(1_048_576, 1).unwrap(),
            ),
        ),
    ));
    let current = current(&storage, &store, thread);
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        selector(&current),
        DraftEditorCandidateSessionIdV1::from_bytes([seed.wrapping_add(4); 16]),
        DraftPieceOperationIdV1::from_bytes([seed.wrapping_add(5); 16]),
    );
    let open = storage
        .prepare_open_draft_editor_candidate_session(&store, request)
        .unwrap();
    let outcome = execute(
        &store,
        storage
            .open_draft_editor_candidate_session(storage.revision(&store).unwrap(), open.clone()),
    );
    let session = match storage
        .reconcile_draft_editor_candidate_session_open(&store, &open, outcome)
        .unwrap()
    {
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(head)
        | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(head) => head,
        other => panic!("candidate session did not open: {other:?}"),
    };
    let identity = DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([seed.wrapping_add(6); 16]),
    );
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, &session), &session)
        .unwrap();
    let session = begin.target_session().unwrap().clone();
    committed(execute(
        &store,
        storage.draft_mutation_staging_command(storage.revision(&store).unwrap(), begin),
    ));
    let head = storage
        .draft_mutation_staging_head(&store, identity)
        .unwrap()
        .unwrap();
    ReceivingFixture {
        home,
        store,
        storage,
        session,
        identity,
        head,
    }
}

pub fn source_inputs(
    page_count: usize,
    items_per_page: usize,
) -> Box<[DraftMutationStagingPageInputV1]> {
    let mut cursor = 0u64;
    let mut inputs = Vec::with_capacity(page_count);
    for _ in 0..page_count {
        let items = (0..items_per_page)
            .map(|offset| {
                DraftMutationStagingPageItemV1::SourcePosition(point(cursor + offset as u64))
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let successor = cursor + items_per_page as u64;
        inputs.push(DraftMutationStagingPageInputV1::new(
            DraftMutationStagingLaneV1::Source,
            cursor,
            successor,
            items_per_page as u16,
            65_536,
            items,
        ));
        cursor = successor;
    }
    inputs.into_boxed_slice()
}

pub fn proposal_inputs(texts: &[&str]) -> Box<[DraftMutationStagingPageInputV1]> {
    texts
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let replacement = if index == 0 {
                DraftPieceReplacementV1::new(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text((*text).to_owned())],
                )
            } else {
                DraftPieceReplacementV1::continuation(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text((*text).to_owned())],
                )
            };
            DraftMutationStagingPageInputV1::new(
                DraftMutationStagingLaneV1::Proposal,
                index as u64,
                index as u64 + 1,
                1,
                65_536,
                Box::new([DraftMutationStagingPageItemV1::Proposal(replacement)]),
            )
        })
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

pub fn prepare(
    fixture: &ReceivingFixture,
    inputs: Box<[DraftMutationStagingPageInputV1]>,
) -> PreparedDraftMutationStagingBatchV1 {
    fixture
        .storage
        .prepare_draft_mutation_staging_page_batch(&fixture.head, &fixture.session, inputs)
        .unwrap()
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
        "unexpected command outcome: {outcome:?}"
    );
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

fn selector(current: &SyndicCurrentDraft) -> DraftEditorCurrentSelectorV1 {
    DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    )
}

pub fn begin_input(
    identity: DraftMutationStagingIdentityV1,
    session: &DraftEditorCandidateSessionV1,
) -> DraftMutationBeginV1 {
    DraftMutationBeginV1::new(
        identity,
        session.session_generation(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        session.logical_extent(),
        point(0),
        point(0),
        point(0),
        point(0),
        point(0),
        0,
        0,
    )
}

pub fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
}

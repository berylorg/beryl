use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(feature = "test-faults")]
use beryl_home_store::{
    CommandError, HomeHealthState,
    test_faults::{FaultController, FaultPoint},
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
    DraftEditorCandidateSessionOpenRequestV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftEditorCandidateSessionV1, DraftEditorCurrentSelectorV1, DraftLogicalExtentV1,
    DraftMutationBeginV1, DraftMutationFinishInputV1, DraftMutationOperationIdV1,
    DraftMutationStagingHeadV1, DraftMutationStagingIdentityV1, DraftMutationStagingLaneV1,
    DraftMutationStagingPageInputV1, DraftMutationStagingPageItemV1, DraftMutationStagingStatusV1,
    DraftPieceDurableBuildWindowLimitsV1, DraftPieceMarkerAtV1, DraftPieceMarkerEffectChargesV1,
    DraftPieceMarkerEffectV1, DraftPieceMarkerInsertionV1, DraftPieceMarkerRemovalProofV1,
    DraftPieceMarkerV1, DraftPieceOperationIdV1, DraftPieceOperationStatusV1,
    DraftPieceOperationVerificationV1, DraftPiecePrepareErrorV1, DraftPieceRejectedReasonV1,
    DraftPieceReplacementV1, DraftPieceV1, PreparedDraftMutationStagingBatchV1,
    PreparedDraftPieceEditV1, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    canonical_empty_draft_piece_fragment_chain_v1, draft_piece_fragment_chain_link_v1,
};
#[cfg(feature = "test-faults")]
use syndic_storage::test_faults::{
    DraftPieceBuildCorruption, inject_draft_piece_build_corruption,
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(1);

struct TestHome(PathBuf);

impl TestHome {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "beryl-syndic-phase154-{name}-{}-{}",
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
fn stage_replacement(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacement: DraftPieceReplacementV1,
    final_extent: DraftLogicalExtentV1,
) -> (
    PreparedDraftPieceEditV1,
    DraftMutationStagingIdentityV1,
    syndic_storage::DraftPieceBuildFragmentV1,
) {
    let fragment_replacement = replacement.clone();
    let identity = DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([operation; 16]),
    );
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, session), session)
        .unwrap();
    let mut active = begin.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), begin),
    ));
    let chain = draft_piece_fragment_chain_link_v1(
        canonical_empty_draft_piece_fragment_chain_v1(),
        1,
        &replacement,
    );
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let page = prepare_one_page(
        *storage,
        &head,
        &active,
        DraftMutationStagingPageItemV1::Proposal(replacement),
    );
    active = page.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
    ));
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &head,
            &active,
            DraftMutationFinishInputV1::new(
                head.source(),
                head.proposal(),
                final_extent,
                point(0),
                point(0),
                point(0),
                chain,
            ),
        )
        .unwrap();
    active = finish.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), finish),
    ));
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&head, &active)
        .unwrap();
    let prepared = transfer.prepared_edit().clone();
    committed(execute(
        store,
        storage
            .transfer_draft_mutation_staging_to_builder(storage.revision(store).unwrap(), transfer),
    ));
    let DraftMutationStagingStatusV1::Building { build, .. } = storage
        .draft_mutation_staging_status(store, identity)
        .unwrap()
    else {
        panic!("transferred effect lost builder custody");
    };
    let window = storage
        .prepare_next_durable_draft_piece_window(
            store,
            identity,
            build,
            DraftPieceDurableBuildWindowLimitsV1::maximum(),
        )
        .unwrap()
        .unwrap();
    committed(execute(
        store,
        storage.stage_next_durable_draft_piece_window(storage.revision(store).unwrap(), window),
    ));
    let fragment = storage
        .prepare_draft_piece_fragment(
            &prepared,
            1,
            canonical_empty_draft_piece_fragment_chain_v1(),
            fragment_replacement,
        )
        .unwrap();
    (prepared, identity, fragment)
}

fn complete_staged(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    replacement: DraftPieceReplacementV1,
    final_extent: DraftLogicalExtentV1,
) -> DraftEditorCandidateSessionV1 {
    let (prepared, identity, _) = stage_replacement(
        storage,
        store,
        session,
        operation,
        replacement,
        final_extent,
    );
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        )
        .unwrap_or_else(|error| panic!("operation {operation} failed to advance: {error:?}"))
    {
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    active_session(storage, store, session.draft_id(), session.session_id())
}

fn advance_error(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
) -> DraftPiecePrepareErrorV1 {
    loop {
        match storage.prepare_draft_piece_build_advance(
            store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
        ) {
            Ok(Some(advance)) => committed(execute(
                store,
                storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
            )),
            Ok(None) => panic!("invalid effect unexpectedly completed"),
            Err(error) => return error,
        }
    }
}

fn open_build(
    storage: &SyndicStorage,
    store: &HomeStore,
    prepared: &PreparedDraftPieceEditV1,
    fragment: &syndic_storage::DraftPieceBuildFragmentV1,
) -> syndic_storage::DraftPieceBuildRecordV1 {
    match storage
        .draft_piece_operation_status_page(store, prepared, 1, std::slice::from_ref(fragment))
        .unwrap()
    {
        DraftPieceOperationVerificationV1::Status(DraftPieceOperationStatusV1::Open(build)) => {
            build
        }
        other => panic!("operation was not an open authenticated build: {other:?}"),
    }
}

fn prepare_one_page(
    storage: SyndicStorage,
    head: &DraftMutationStagingHeadV1,
    session: &DraftEditorCandidateSessionV1,
    item: DraftMutationStagingPageItemV1,
) -> PreparedDraftMutationStagingBatchV1 {
    let lane = match item {
        DraftMutationStagingPageItemV1::SourcePosition(_) => DraftMutationStagingLaneV1::Source,
        DraftMutationStagingPageItemV1::Proposal(_) => DraftMutationStagingLaneV1::Proposal,
    };
    let frontier = match lane {
        DraftMutationStagingLaneV1::Source => head.source(),
        DraftMutationStagingLaneV1::Proposal => head.proposal(),
    };
    storage
        .prepare_draft_mutation_staging_page_batch(
            head,
            session,
            Box::new([DraftMutationStagingPageInputV1::new(
                lane,
                frontier.next_cursor(),
                frontier.next_cursor() + 1,
                1,
                65_536,
                Box::new([item]),
            )]),
        )
        .unwrap()
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
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase154",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    (home, store, storage, thread)
}

#[cfg(feature = "test-faults")]
fn fixture_with_faults(
    name: &str,
    seed: u8,
    faults: FaultController,
) -> (TestHome, HomeStore, SyndicStorage, SyndicThreadId) {
    let home = TestHome::new(name);
    let mut store = HomeStore::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults,
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
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\syndic-phase154-faults",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    (home, store, storage, thread)
}

fn open_session(
    storage: SyndicStorage,
    store: &HomeStore,
    current: &syndic_storage::SyndicCurrentDraft,
    session: u8,
    operation: u8,
) -> DraftEditorCandidateSessionV1 {
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        selector(current),
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
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
        other => panic!("session did not open: {other:?}"),
    }
}

fn active_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    draft_id: SyndicDraftId,
    session_id: DraftEditorCandidateSessionIdV1,
) -> DraftEditorCandidateSessionV1 {
    match storage
        .draft_editor_candidate_session(store, draft_id, session_id)
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("candidate session was not active: {other:?}"),
    }
}

fn current(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
) -> syndic_storage::SyndicCurrentDraft {
    storage
        .current_draft(store, thread, SyndicPointReadLimit::new(65_536).unwrap())
        .unwrap()
        .unwrap()
}

fn selector(current: &syndic_storage::SyndicCurrentDraft) -> DraftEditorCurrentSelectorV1 {
    DraftEditorCurrentSelectorV1::new(
        current.thread().id(),
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().piece_root(),
        current.draft().history(),
    )
}

fn begin_input(
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

fn marker(seed: u8, order: u64, label: u64) -> DraftPieceMarkerV1 {
    DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([seed; 16]),
        order,
        ImageLabelOrdinal::new(label).unwrap(),
        beryl_model::AssetId::sha256_v1(
            [seed.wrapping_add(1); 32],
            std::num::NonZeroU64::new(u64::from(seed) + 1).unwrap(),
        ),
    )
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
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
    ), "unexpected command outcome: {outcome:?}");
}

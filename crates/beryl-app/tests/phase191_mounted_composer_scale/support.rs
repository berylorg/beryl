#[path = "../phase177_main_window_composer_slot/support.rs"]
pub mod fixture;

use std::num::NonZeroU64;

use beryl_app::composer_host::ComposerHostBinding;
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeStore, MutationContribution, SidecarByteLimit,
    SidecarNamespace,
};
use beryl_model::{
    AssetId, ExecutionBinding, ImageLabelOrdinal, PathFlavor, RootId, RuntimeId, RuntimeMode,
    RuntimeNativePath, SyndicDraftId, SyndicDraftMarkerId, SyndicThreadId, WindowId,
};
use beryl_state::{
    AssetMediaType, BerylState, PublishAssetMetadata, RememberedTarget, ReplaceWindowClaim,
    SessionState, WindowClaimSelection,
};
use syndic_storage::{
    CreateThread, DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftEditHistoryPolicyV1,
    DraftEditorCandidatePublicationEvidenceV1, DraftEditorCandidatePublicationOutcomeV1,
    DraftEditorCandidatePublicationRequestV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionOpenOutcomeV1, DraftEditorCandidateSessionOpenRequestV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftEditorCandidateSessionV1,
    DraftEditorCurrentSelectorV1, DraftLogicalExtentV1, DraftMutationBeginV1,
    DraftMutationFinishInputV1, DraftMutationOperationIdV1, DraftMutationStagingHeadV1,
    DraftMutationStagingIdentityV1, DraftMutationStagingLaneV1, DraftMutationStagingPageInputV1,
    DraftMutationStagingPageItemV1, DraftMutationStagingStatusV1,
    DraftPieceDurableBuildWindowLimitsV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerScopeV1, DraftPieceOperationIdV1, DraftPieceReplacementV1,
    DraftPieceTextDemandV1, DraftPieceV1, DraftRootHistoryPairV1,
    PreparedDraftMutationStagingBatchV1, SyndicPointReadLimit, SyndicStorage, SyndicTimestamp,
    canonical_empty_draft_piece_fragment_chain_v1, draft_piece_fragment_chain_link_v1,
};

pub const LARGE_CHUNK_BYTES: usize = 32_768;
pub const LARGE_CHUNK_COUNT: usize = 96;
pub const LARGE_DRAFT_BYTES: u64 = (LARGE_CHUNK_BYTES * LARGE_CHUNK_COUNT) as u64;
pub const SAME_ANCHOR_MARKERS: usize = 257;
pub const MARKER_ID_BASE: u128 = 0x1890_0000;

pub fn create_third_target(
    fixture: &fixture::Fixture,
    seed: u8,
    selected_claim: WindowClaimSelection,
) -> (SyndicThreadId, WindowClaimSelection, WindowClaimSelection) {
    let runtime_id = RuntimeId::from_bytes([seed; 16]);
    let root_id = RootId::from_bytes([seed.wrapping_add(1); 16]);
    let third_thread = SyndicThreadId::from_bytes([seed.wrapping_add(7); 16]);
    committed(execute(
        &fixture.store,
        fixture.storage.create_thread(
            fixture.storage.revision(&fixture.store).unwrap(),
            CreateThread::ordinary(
                third_thread,
                SyndicDraftId::from_bytes([seed.wrapping_add(8); 16]),
                ExecutionBinding::new(
                    runtime_id,
                    root_id,
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        r"C:\Work\Beryl\third",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(4),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));

    let state = BerylState::reacquire(&fixture.store).unwrap();
    let session = state.session();
    let third_claim = replace_claim(
        &fixture.store,
        session,
        fixture.window_id,
        selected_claim,
        runtime_id,
        root_id,
        third_thread,
    );
    let selected_claim = replace_claim(
        &fixture.store,
        session,
        fixture.window_id,
        third_claim,
        runtime_id,
        root_id,
        fixture.selected_thread,
    );
    (third_thread, third_claim, selected_claim)
}

pub fn seed_large_published_draft(fixture: &fixture::Fixture, thread: beryl_model::SyndicThreadId) {
    let current = fixture
        .storage
        .current_draft(
            &fixture.store,
            thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut session = open_session(fixture.storage, &fixture.store, &current);
    for chunk in 0..LARGE_CHUNK_COUNT {
        let offset = (chunk * LARGE_CHUNK_BYTES) as u64;
        session = append_chunk(
            fixture.storage,
            &fixture.store,
            &session,
            (chunk + 1) as u8,
            offset,
            large_chunk(chunk),
        );
    }
    let request = DraftEditorCandidatePublicationRequestV1::new(
        selector(&current),
        session.session_id(),
        DraftPieceOperationIdV1::from_bytes([199; 16]),
        session.newest_candidate_generation(),
        DraftRootHistoryPairV1::new(session.newest_root(), session.newest_history()),
        DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        SyndicTimestamp::from_unix_millis(1_000),
    );
    let capture = fixture
        .storage
        .capture_draft_editor_candidate_publication_source(
            &fixture.store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                request.selector(),
                syndic_storage::DraftEditorCandidateActivationBindingV1::new(
                    request.selector().draft_id(),
                    request.session_id(),
                    session.session_generation(),
                    request.candidate_generation(),
                    request.candidate().root(),
                    request.candidate().history(),
                    request.candidate().root().summary().logical_extent(),
                ),
                request.operation_id(),
                request.published_at(),
            ),
        )
        .unwrap();
    let prepared = fixture
        .storage
        .prepare_draft_editor_candidate_publication(&fixture.store, capture, request.evidence())
        .unwrap();
    let outcome = execute(
        &fixture.store,
        fixture.storage.publish_draft_editor_candidate(
            fixture.storage.revision(&fixture.store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        fixture
            .storage
            .reconcile_draft_editor_candidate_publication(&fixture.store, &prepared, outcome,)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
}

pub fn assert_tail_byte(
    storage: SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    expected: u8,
) {
    let page = storage
        .draft_piece_text_demand(
            store,
            root,
            DraftPieceTextDemandV1::Forward(LARGE_DRAFT_BYTES - 1),
            4,
        )
        .unwrap();
    assert_eq!(page.start(), LARGE_DRAFT_BYTES - 1);
    assert_eq!(page.bytes(), &[expected]);
}

pub fn assert_candidate_operation_reconciled(
    storage: SyndicStorage,
    store: &HomeStore,
    binding: ComposerHostBinding,
) {
    let candidate = binding.candidate();
    let session = storage
        .draft_editor_candidate_session(store, candidate.draft_id(), candidate.session_id())
        .unwrap();
    let DraftEditorCandidateSessionReadOutcomeV1::Active(session) = session else {
        panic!("mounted candidate operation did not reconcile to an active session: {session:?}");
    };
    assert_eq!(
        session.newest_candidate_generation(),
        candidate.candidate_generation()
    );
    assert_eq!(session.newest_root(), candidate.root());
    assert_eq!(session.newest_history(), candidate.history());
    assert_eq!(session.logical_extent(), candidate.logical_extent());
}

pub fn expected_byte(offset: u64) -> u8 {
    b'a' + ((offset / LARGE_CHUNK_BYTES as u64 + offset) % 26) as u8
}

pub fn marker_object_id(index: usize) -> u128 {
    MARKER_ID_BASE + (SAME_ANCHOR_MARKERS - index) as u128
}

pub fn publish_image_asset(fixture: &fixture::Fixture, bytes: &[u8]) -> AssetId {
    let sidecar = fixture
        .store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1_024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let assets = fixture.assets();
    let expected = assets.revision(&fixture.store).unwrap();
    let contribution = assets
        .publish_metadata(
            expected,
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
                None,
                expected.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    committed(fixture.store.execute(command));
    asset
}

pub fn assert_same_anchor_marker_order(
    storage: SyndicStorage,
    store: &HomeStore,
    root: syndic_storage::DraftPieceRootReferenceV1,
    asset: AssetId,
) {
    let mut cursor = None;
    let mut seen = 0_usize;
    for _ in 0..16 {
        let page = storage
            .draft_piece_marker_demand(
                store,
                root,
                DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::ExactAnchor(LARGE_DRAFT_BYTES),
                    DraftPieceMarkerDirectionV1::Forward,
                    cursor,
                    31,
                    65_536,
                ),
            )
            .unwrap();
        for at in page.markers() {
            let marker = at.marker();
            assert_eq!(at.anchor(), LARGE_DRAFT_BYTES);
            assert_eq!(marker.order_key(), (seen + 1) as u64);
            assert_eq!(
                marker.marker_id(),
                SyndicDraftMarkerId::from_bytes(marker_object_id(seen).to_be_bytes())
            );
            assert_eq!(marker.label(), ImageLabelOrdinal::new(1).unwrap());
            assert_eq!(marker.asset_id(), asset);
            seen += 1;
        }
        cursor = page.continuation();
        if cursor.is_none() {
            assert!(page.requested_side_complete());
            break;
        }
    }
    assert_eq!(seen, SAME_ANCHOR_MARKERS);
    assert_eq!(cursor, None);
}

fn large_chunk(chunk: usize) -> String {
    let mut bytes = vec![0_u8; LARGE_CHUNK_BYTES];
    for (offset, byte) in bytes.iter_mut().enumerate() {
        *byte = expected_byte((chunk * LARGE_CHUNK_BYTES + offset) as u64);
    }
    String::from_utf8(bytes).unwrap()
}

fn open_session(
    storage: SyndicStorage,
    store: &HomeStore,
    current: &syndic_storage::SyndicCurrentDraft,
) -> DraftEditorCandidateSessionV1 {
    let request = DraftEditorCandidateSessionOpenRequestV1::new(
        selector(current),
        DraftEditorCandidateSessionIdV1::from_bytes([197; 16]),
        DraftPieceOperationIdV1::from_bytes([198; 16]),
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
        DraftEditorCandidateSessionOpenOutcomeV1::Opened(session)
        | DraftEditorCandidateSessionOpenOutcomeV1::ExactReplay(session) => session,
        other => panic!("large-draft editor session did not open: {other:?}"),
    }
}

fn append_chunk(
    storage: SyndicStorage,
    store: &HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    offset: u64,
    text: String,
) -> DraftEditorCandidateSessionV1 {
    let identity = DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes([operation; 16]),
    );
    let begin = storage
        .prepare_draft_mutation_staging_begin(begin_input(identity, session, offset), session)
        .unwrap();
    let mut active = begin.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), begin),
    ));
    let replacement =
        DraftPieceReplacementV1::new(point(offset), point(offset), vec![DraftPieceV1::Text(text)]);
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let page = prepare_page(storage, &head, &active, replacement.clone());
    active = page.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
    ));
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let chain = draft_piece_fragment_chain_link_v1(
        canonical_empty_draft_piece_fragment_chain_v1(),
        1,
        &replacement,
    );
    let final_offset = offset + LARGE_CHUNK_BYTES as u64;
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &head,
            &active,
            DraftMutationFinishInputV1::new(
                head.source(),
                head.proposal(),
                DraftLogicalExtentV1::new(final_offset, 1),
                point(final_offset),
                point(final_offset),
                point(final_offset),
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
        panic!("large-draft staging transfer lost builder custody");
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
    let mut settled = false;
    for _ in 0..16_384 {
        let Some(advance) = storage
            .prepare_draft_piece_build_advance(
                store,
                identity.draft_id(),
                identity.session_id(),
                identity.operation_id().as_piece_operation(),
            )
            .unwrap()
        else {
            settled = true;
            break;
        };
        committed(execute(
            store,
            storage.advance_draft_piece_edit(storage.revision(store).unwrap(), advance),
        ));
    }
    assert!(
        settled,
        "large-draft builder exceeded its finite advance bound"
    );
    committed(execute(
        store,
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
    match storage
        .draft_editor_candidate_session(store, session.draft_id(), session.session_id())
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("large-draft editor session was not active: {other:?}"),
    }
}

fn prepare_page(
    storage: SyndicStorage,
    head: &DraftMutationStagingHeadV1,
    session: &DraftEditorCandidateSessionV1,
    replacement: DraftPieceReplacementV1,
) -> PreparedDraftMutationStagingBatchV1 {
    storage
        .prepare_draft_mutation_staging_page_batch(
            head,
            session,
            Box::new([DraftMutationStagingPageInputV1::new(
                DraftMutationStagingLaneV1::Proposal,
                head.proposal().next_cursor(),
                head.proposal().next_cursor() + 1,
                1,
                65_536,
                Box::new([DraftMutationStagingPageItemV1::Proposal(replacement)]),
            )]),
        )
        .unwrap()
}

fn begin_input(
    identity: DraftMutationStagingIdentityV1,
    session: &DraftEditorCandidateSessionV1,
    offset: u64,
) -> DraftMutationBeginV1 {
    DraftMutationBeginV1::new(
        identity,
        session.session_generation(),
        session.newest_candidate_generation(),
        session.newest_root(),
        session.newest_history(),
        session.logical_extent(),
        point(offset),
        point(offset),
        point(offset),
        point(offset),
        point(offset),
        0,
        0,
    )
}

fn point(offset: u64) -> DraftCompositePositionV1 {
    DraftCompositePositionV1::new(offset, DraftCompositeGapWitnessV1::Unambiguous)
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

fn replace_claim(
    store: &HomeStore,
    session: SessionState,
    window_id: WindowId,
    expected: WindowClaimSelection,
    runtime_id: RuntimeId,
    root_id: RootId,
    thread_id: SyndicThreadId,
) -> WindowClaimSelection {
    let initial = session.minimal_bootstrap(store).unwrap().unwrap();
    let record = initial
        .windows()
        .iter()
        .find(|record| record.window_id() == window_id)
        .unwrap();
    committed(execute(
        store,
        session.replace_claim(
            session.revision(store).unwrap(),
            ReplaceWindowClaim::new(
                initial.header().revision(),
                window_id,
                record.revision(),
                Some(expected),
                RememberedTarget::new(runtime_id, root_id),
                thread_id,
            ),
        ),
    ));
    session
        .minimal_bootstrap(store)
        .unwrap()
        .unwrap()
        .windows()
        .iter()
        .find(|record| record.window_id() == window_id)
        .unwrap()
        .selected_thread()
        .unwrap()
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

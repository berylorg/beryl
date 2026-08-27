#[path = "../phase177_main_window_composer_slot/support.rs"]
pub mod fixture;

use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationRequest, ComposerHostInitialDemand, ComposerHostRequestId,
    ComposerHostRequestPurpose,
};
use beryl_home_store::{CommandOutcome, HomeCommand, MutationContribution};
use gpui::{
    SharedString, StreamingLayoutBinding, StreamingLayoutLimits, StreamingLayoutPosition, TextRun,
    black, font, px,
};
use gpui_scrollbar::ScrollbarStyle;
use gpui_text_input::{
    ClipboardLimits, ExactGeometryLimits, MutationLimits, ObjectResidencyLimits,
    PresentationGeneration, RangeSettlementCoordinator, RangeTextInputConfig, RangeTextInputLimits,
    ResidencyLimits, SegmentationLimits, StreamingGeometryStyle, StreamingOversizePresentation,
    TextInputAtomClipboardPolicy, TextInputEnterKey, TextInputRichPastePolicy, TextInputTheme,
};
use syndic_storage::{
    DraftCompositeGapWitnessV1, DraftCompositePositionV1,
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

const ACTIVATION_CHUNK_BYTES: usize = 768;
const ACTIVATION_CHUNK_COUNT: usize = 3;
const ACTIVATION_PAGE_BYTES: usize = 256;
pub const ACTIVATION_DRAFT_BYTES: u64 = (ACTIVATION_CHUNK_BYTES * ACTIVATION_CHUNK_COUNT) as u64;

pub fn activation(
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
    presentation: u64,
    end: u64,
) -> ComposerHostActivationRequest {
    let mut demands = Vec::new();
    let page_count = if end == 0 { 1 } else { 8 };
    for page in 0..page_count {
        let start = page * ACTIVATION_PAGE_BYTES as u64;
        demands.push(ComposerHostInitialDemand::Text {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(page * 2 + 1).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceTextDemandV1::Forward(start),
            max_bytes: ACTIVATION_PAGE_BYTES,
        });
        demands.push(ComposerHostInitialDemand::Markers {
            request_id: ComposerHostRequestId::new(NonZeroU64::new(page * 2 + 2).unwrap()),
            purpose: ComposerHostRequestPurpose::Geometry,
            demand: DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::Range {
                    start,
                    end: (start + ACTIVATION_PAGE_BYTES as u64).min(end),
                },
                DraftPieceMarkerDirectionV1::Forward,
                None,
                48,
                65_536,
            ),
        });
    }
    ComposerHostActivationRequest::new(
        thread,
        syndic_storage::DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        fixture::operation_id(operation),
        NonZeroU64::new(presentation).unwrap(),
        None,
        demands.into_boxed_slice(),
    )
}

pub fn widget_config(
    binding: gpui_text_input::RangeBinding,
    presentation: NonZeroU64,
) -> RangeTextInputConfig {
    let layout = StreamingLayoutBinding {
        input_id: 18_500,
        segment_policy_id: 18_501,
        start_position: StreamingLayoutPosition::at(0),
        wrap_width: px(320.),
        font_size: px(12.),
        line_height: px(16.),
        limits: StreamingLayoutLimits {
            segment_bytes: ACTIVATION_PAGE_BYTES,
            runs: 8,
            decorations: 8,
            glyphs: 4096,
            wraps: 256,
            maps: 4097,
            fragments: 4,
            retained_items: 32_768,
            retained_bytes: 2 * 1024 * 1024,
        },
    };
    let run = TextRun {
        len: 0,
        font: font(".SystemUIFont"),
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    RangeTextInputConfig {
        binding,
        presentation_generation: PresentationGeneration::new(presentation.get()),
        enter_key: TextInputEnterKey::Propagate,
        atom_clipboard_policy: TextInputAtomClipboardPolicy::Propagate,
        rich_paste_policy: TextInputRichPastePolicy::Propagate,
        layout,
        style: StreamingGeometryStyle::new(
            run,
            StreamingOversizePresentation::new(
                SharedString::new_static(""),
                vec![],
                px(12.),
                px(16.),
                px(12.),
                None,
            ),
        ),
        geometry_limits: ExactGeometryLimits::new(
            ACTIVATION_PAGE_BYTES as u64,
            16,
            2 * 1024 * 1024,
            32_768,
        )
        .unwrap(),
        residency_limits: ResidencyLimits::new(9, 384 * 1024, 6, 384 * 1024).unwrap(),
        object_residency_limits: ObjectResidencyLimits::new(9, 48, 65_536, 65_536, 6, 48, 65_536)
            .unwrap(),
        mutation_limits: MutationLimits::new(64, 65_536).unwrap(),
        clipboard_limits: ClipboardLimits::new(64 * 1024, ACTIVATION_PAGE_BYTES as u64).unwrap(),
        segmentation_limits: SegmentationLimits::new(ACTIVATION_PAGE_BYTES as u64, 4096).unwrap(),
        limits: RangeTextInputLimits::new(
            8 * 1024 * 1024,
            131_072,
            64,
            px(64.),
            ACTIVATION_PAGE_BYTES as u64,
            ACTIVATION_PAGE_BYTES as u64,
            px(16.),
        )
        .unwrap(),
        settlement_coordinator: RangeSettlementCoordinator::new(4).unwrap(),
        viewport_extent: px(640.),
        overscan: px(32.),
        placeholder: SharedString::new_static("Message"),
        theme: TextInputTheme::default(),
        scrollbar_style: ScrollbarStyle::default(),
    }
}

pub fn drive(cx: &mut gpui::VisualTestContext, rounds: usize) {
    for _ in 0..rounds {
        cx.run_until_parked();
        cx.update(|window, app| window.draw(app).clear());
    }
}

pub fn seed_activation_published_draft(
    fixture: &fixture::Fixture,
    thread: beryl_model::SyndicThreadId,
) {
    assert_eq!(
        seed_activation_published_draft_chunks(fixture, thread, ACTIVATION_CHUNK_COUNT),
        ACTIVATION_DRAFT_BYTES
    );
}

pub fn seed_activation_published_draft_chunks(
    fixture: &fixture::Fixture,
    thread: beryl_model::SyndicThreadId,
    chunk_count: usize,
) -> u64 {
    assert!(chunk_count > 0 && chunk_count < 199);
    let current = fixture
        .storage
        .current_draft(
            &fixture.store,
            thread,
            SyndicPointReadLimit::new(65_536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut session = open_activation_session(fixture.storage, &fixture.store, &current);
    for chunk in 0..chunk_count {
        let offset = (chunk * ACTIVATION_CHUNK_BYTES) as u64;
        session = append_activation_chunk(
            fixture.storage,
            &fixture.store,
            &session,
            (chunk + 1) as u8,
            offset,
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
            .reconcile_draft_editor_candidate_publication(&fixture.store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    (ACTIVATION_CHUNK_BYTES * chunk_count) as u64
}

fn open_activation_session(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
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
        other => panic!("activation editor session did not open: {other:?}"),
    }
}

fn append_activation_chunk(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    offset: u64,
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
    let mut text = vec![b'x'; ACTIVATION_CHUNK_BYTES];
    for line_end in (63..ACTIVATION_CHUNK_BYTES).step_by(64) {
        text[line_end] = b'\n';
    }
    let replacement = DraftPieceReplacementV1::new(
        point(offset),
        point(offset),
        vec![DraftPieceV1::Text(String::from_utf8(text).unwrap())],
    );
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
    let final_offset = offset + ACTIVATION_CHUNK_BYTES as u64;
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
        panic!("activation staging transfer lost builder custody");
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
        "activation builder exceeded its finite advance bound"
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
        other => panic!("activation editor session was not active: {other:?}"),
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

fn execute(
    store: &beryl_home_store::HomeStore,
    contribution: MutationContribution,
) -> CommandOutcome {
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

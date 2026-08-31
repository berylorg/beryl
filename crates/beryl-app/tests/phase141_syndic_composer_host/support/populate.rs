use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostBinding,
    ComposerHostError, ComposerHostImageMarkerMetadata, ComposerHostMutationOutcome,
    SyndicComposerHost,
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::SyndicThreadId;
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, LogicalExtent, MutationBeginRequest,
    MutationCommitRequest, MutationCursor, MutationFinishInput, MutationIdentity, MutationKey,
    MutationKind, MutationLane, MutationPage, MutationPageItem, MutationPageKey,
    MutationPageRequest, MutationPositions, MutationProposal, MutationStreamFinish, OperationId,
    SourcePosition, SourceRange, SourceRevision,
};
use syndic_storage::{
    DraftCompositeGapWitnessV1, DraftCompositePositionV1, DraftEditorCandidateSessionIdV1,
    DraftEditorCandidateSessionReadOutcomeV1, DraftMarkerAdmissionOperationIdV1,
    DraftMarkerAdmissionOwnerV1, DraftMutationBeginV1, DraftMutationFinishInputV1,
    DraftMutationOperationIdV1, DraftMutationStagingIdentityV1, DraftMutationStagingLaneV1,
    DraftMutationStagingPageInputV1, DraftMutationStagingPageItemV1, DraftMutationStagingStatusV1,
    DraftPieceDurableBuildWindowLimitsV1, DraftPieceMarkerEffectChargesV1,
    DraftPieceMarkerEffectV1, DraftPieceMarkerInsertionV1, DraftPieceMarkerV1,
    DraftPieceOperationIdV1, DraftPieceReplacementV1, DraftPieceV1, SyndicStorage,
    canonical_empty_draft_piece_fragment_chain_v1, draft_piece_fragment_chain_link_v1,
};

use super::{committed, execute, marker, point};

pub fn populate(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    session_seed: u8,
) -> (DraftPieceMarkerV1, DraftPieceMarkerV1) {
    let left = marker(session_seed.wrapping_add(1), 1, 2);
    let right = marker(session_seed.wrapping_add(2), 2, 1);
    let mut host = SyndicComposerHost::new(storage.clone());
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .test_activate(
            store,
            ComposerHostActivationRequest::new(
                thread,
                DraftEditorCandidateSessionIdV1::from_bytes([session_seed; 16]),
                DraftPieceOperationIdV1::from_bytes([session_seed.wrapping_add(10); 16]),
                NonZeroU64::MIN,
                None,
                Box::new([]),
            ),
            &CommandCancellation::new(),
        )
        .unwrap()
    else {
        panic!("fixture candidate session did not activate");
    };
    let zero = position(0);
    let binding = commit_page(
        &mut host,
        store,
        binding,
        u64::from(session_seed) + 1,
        SourceRange::new(zero, zero).unwrap(),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "\u{03b1}\n\u{03b2}\n".into(),
        }],
        Box::new([]),
        LogicalExtent::new(6, 3),
        MutationPositions::collapsed(position(6)),
    );
    let session = active_session(&storage, store, binding);
    let session = complete_admitted_marker_insertion(
        &storage,
        store,
        &session,
        right,
        session_seed.wrapping_add(20),
        point(3),
    );
    complete_admitted_marker_insertion(
        &storage,
        store,
        &session,
        left,
        session_seed.wrapping_add(21),
        DraftCompositePositionV1::new(3, DraftCompositeGapWitnessV1::BeforeAll),
    );
    (left, right)
}

fn active_session(
    storage: &SyndicStorage,
    store: &HomeStore,
    binding: ComposerHostBinding,
) -> syndic_storage::DraftEditorCandidateSessionV1 {
    match storage
        .draft_editor_candidate_session(
            store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("fixture candidate session was not active: {other:?}"),
    }
}

fn complete_admitted_marker_insertion(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    marker: DraftPieceMarkerV1,
    operation: u8,
    position: DraftCompositePositionV1,
) -> syndic_storage::DraftEditorCandidateSessionV1 {
    let replacement =
        DraftPieceReplacementV1::new(position, position, vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    3,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            ));
    let (admission, identity, active) =
        begin_admitted_marker_insertion(storage, store, session, marker, operation);
    let active = stage_admitted_marker_insertion(storage, store, identity, &active, &replacement);
    let active = finish_admitted_marker_insertion(
        storage,
        store,
        identity,
        &active,
        session.logical_extent(),
        &replacement,
    );
    let prepared = transfer_admitted_marker_insertion(storage, store, identity, &active);
    settle_admitted_marker_insertion(storage, store, identity, prepared);
    storage
        .release_settled_draft_marker_writer(store, admission)
        .unwrap();
    match storage
        .draft_editor_candidate_session(store, session.draft_id(), session.session_id())
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("fixture candidate session was not active after settlement: {other:?}"),
    }
}

fn begin_admitted_marker_insertion(
    storage: &SyndicStorage,
    store: &HomeStore,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
    marker: DraftPieceMarkerV1,
    operation: u8,
) -> (
    DraftMarkerAdmissionOwnerV1,
    DraftMutationStagingIdentityV1,
    syndic_storage::DraftEditorCandidateSessionV1,
) {
    let admission = DraftMarkerAdmissionOwnerV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes([operation; 16]),
    );
    let readiness = storage
        .seed_draft_marker_writer_ready_target_for_test(store, session, admission, marker)
        .unwrap();
    let identity = DraftMutationStagingIdentityV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMutationOperationIdV1::from_bytes(*admission.operation_id().as_bytes()),
    );
    let begin = storage
        .prepare_draft_mutation_staging_marker_begin(
            mutation_begin(identity, session),
            session,
            readiness,
        )
        .unwrap();
    let active = begin.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), begin),
    ));
    (admission, identity, active)
}

fn stage_admitted_marker_insertion(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
    active: &syndic_storage::DraftEditorCandidateSessionV1,
    replacement: &DraftPieceReplacementV1,
) -> syndic_storage::DraftEditorCandidateSessionV1 {
    let head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let page = storage
        .prepare_draft_mutation_staging_page_batch(
            &head,
            active,
            Box::new([DraftMutationStagingPageInputV1::new(
                DraftMutationStagingLaneV1::Proposal,
                head.proposal().next_cursor(),
                head.proposal().next_cursor() + 1,
                1,
                65_536,
                Box::new([DraftMutationStagingPageItemV1::Proposal(
                    replacement.clone(),
                )]),
            )]),
        )
        .unwrap();
    let active = page.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_page_batch(storage.revision(store).unwrap(), page),
    ));
    active
}

fn finish_admitted_marker_insertion(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
    active: &syndic_storage::DraftEditorCandidateSessionV1,
    extent: syndic_storage::DraftLogicalExtentV1,
    replacement: &DraftPieceReplacementV1,
) -> syndic_storage::DraftEditorCandidateSessionV1 {
    let receiving = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let chain = draft_piece_fragment_chain_link_v1(
        canonical_empty_draft_piece_fragment_chain_v1(),
        1,
        replacement,
    );
    let finish = storage
        .prepare_draft_mutation_staging_finish(
            &receiving,
            active,
            DraftMutationFinishInputV1::new(
                receiving.source(),
                receiving.proposal(),
                extent,
                point(0),
                point(0),
                point(0),
                chain,
            ),
        )
        .unwrap();
    let active = finish.target_session().unwrap().clone();
    committed(execute(
        store,
        storage.draft_mutation_staging_command(storage.revision(store).unwrap(), finish),
    ));
    active
}

fn transfer_admitted_marker_insertion(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
    active: &syndic_storage::DraftEditorCandidateSessionV1,
) -> syndic_storage::PreparedDraftPieceEditV1 {
    let prepared_head = storage
        .draft_mutation_staging_head(store, identity)
        .unwrap()
        .unwrap();
    let transfer = storage
        .prepare_draft_mutation_staging_transfer(&prepared_head, active)
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
        panic!("admitted marker staging did not transfer to building")
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
    prepared
}

fn settle_admitted_marker_insertion(
    storage: &SyndicStorage,
    store: &HomeStore,
    identity: DraftMutationStagingIdentityV1,
    prepared: syndic_storage::PreparedDraftPieceEditV1,
) {
    while let Some(advance) = storage
        .prepare_draft_piece_build_advance(
            store,
            identity.draft_id(),
            identity.session_id(),
            identity.operation_id().as_piece_operation(),
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
        storage.settle_draft_piece_edit(storage.revision(store).unwrap(), prepared),
    ));
}

fn mutation_begin(
    identity: DraftMutationStagingIdentityV1,
    session: &syndic_storage::DraftEditorCandidateSessionV1,
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

fn commit_page(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    replacement: SourceRange,
    items: Vec<MutationPageItem>,
    marker_metadata: Box<[ComposerHostImageMarkerMetadata]>,
    extent: LogicalExtent,
    intended: MutationPositions,
) -> ComposerHostBinding {
    let key = MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
    );
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(replacement.start()),
                replacement,
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        items,
    )
    .unwrap();
    let proposal_finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    host.stage_mutation_page(store, MutationPageRequest::new(page), marker_metadata)
        .unwrap();
    host.finish_mutation_input(
        store,
        MutationFinishInput::new(key, empty_finish(), proposal_finish, extent, intended),
    )
    .unwrap();
    for _ in 0..16 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => continue,
            other => panic!("fixture mutation {operation} did not commit: {other:?}"),
        }
    }
    panic!("fixture mutation remained pending")
}

fn empty_finish() -> MutationStreamFinish {
    MutationStreamFinish {
        next_cursor: MutationCursor::new(0),
        next_ordinal: 0,
        cumulative_identity: MutationIdentity::ROOT,
        totals: Default::default(),
    }
}

fn position(offset: u64) -> SourcePosition {
    SourcePosition::new(ByteOffset::new(offset), InlineObjectGap::NoObjects)
}

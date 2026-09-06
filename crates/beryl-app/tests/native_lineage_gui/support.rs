use beryl_app::composer_host::{
    ComposerHostBinding, ComposerHostError, ComposerHostImageMarkerMetadata,
    ComposerHostMutationOutcome, SyndicComposerHost,
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftMarkerId};
use gpui_text_input::{
    ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor, InlineObjectOrder,
    LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKind, MutationLane, MutationPage,
    MutationPageItem, MutationPageKey, MutationPageRequest, MutationPositions, MutationProposal,
    MutationStreamFinish, MutationTotals, ObjectChange, SourcePosition, SourceRange,
    SuccessorObject,
};
use syndic_storage::{
    DraftEditorCandidateSessionReadOutcomeV1, DraftMarkerAdmissionOperationIdV1,
    DraftMarkerAdmissionOwnerV1, DraftPieceMarkerV1, SyndicStorage,
};

pub fn insert_published_marker_with_readiness(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    storage: &SyndicStorage,
    binding: ComposerHostBinding,
    operation: u64,
    asset: AssetId,
) -> ComposerHostBinding {
    let object = InlineObjectId::new(0x1001);
    let order = InlineObjectOrder::new(1);
    let point = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects);
    let key = gpui_text_input::MutationKey::new(
        gpui_text_input::BindingId::new(binding.host_generation().get()),
        gpui_text_input::SourceRevision::new(binding.candidate().candidate_generation()),
        gpui_text_input::OperationId::new(operation),
    );
    let begin = MutationBeginRequest::new(
        MutationProposal::new(
            key,
            MutationKind::Edit,
            MutationPositions::collapsed(point),
            SourceRange::new(point, point).unwrap(),
            0,
        ),
        MutationCursor::new(0),
        MutationCursor::new(0),
    );
    let session = match storage
        .draft_editor_candidate_session(
            store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(session) => session,
        other => panic!("fixture candidate session was not active: {other:?}"),
    };
    let mut operation_bytes = [0; 16];
    operation_bytes[8..].copy_from_slice(&operation.to_be_bytes());
    let owner = DraftMarkerAdmissionOwnerV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes(operation_bytes),
    );
    let marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes(object.get().to_be_bytes()),
        1,
        ImageLabelOrdinal::new(1).unwrap(),
        asset,
    );
    let readiness = storage
        .seed_draft_marker_writer_ready_target_for_test(store, &session, owner, marker)
        .unwrap();
    host.test_begin_marker_mutation(store, binding, begin, readiness)
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
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(object, ByteOffset::new(0), order, 17, 5),
        })],
    )
    .unwrap();
    let finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    host.stage_mutation_page(
        store,
        MutationPageRequest::new(page),
        Box::from([ComposerHostImageMarkerMetadata::new(
            object,
            ImageLabelOrdinal::new(1).unwrap(),
            asset,
        )]),
    )
    .unwrap();
    let after = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(object, order)),
    );
    host.finish_mutation_input(
        store,
        MutationFinishInput::new(
            key,
            MutationStreamFinish {
                next_cursor: MutationCursor::new(0),
                next_ordinal: 0,
                cumulative_identity: MutationIdentity::ROOT,
                totals: MutationTotals::default(),
            },
            finish,
            LogicalExtent::new(0, 1),
            MutationPositions::collapsed(after),
        ),
    )
    .unwrap();
    for _ in 0..16 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => {
                storage
                    .release_settled_draft_marker_writer(store, owner)
                    .unwrap();
                return binding;
            }
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("marker mutation did not commit: {other:?}"),
        }
    }
    panic!("marker mutation remained pending")
}

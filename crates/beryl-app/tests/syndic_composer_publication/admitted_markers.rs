use beryl_app::composer_host::{
    ComposerHostBinding, ComposerHostError, ComposerHostImageMarkerMetadata,
    ComposerHostMutationEvidenceOutcome, ComposerHostMutationEvidenceRequest,
    ComposerHostMutationOutcome, SyndicComposerHost,
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::AssetId;
use beryl_state::AssetState;
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationLimits,
    MutationPage, MutationPageAcceptance, MutationPageItem, MutationPageKey, MutationPageRequest,
    MutationPositions, MutationProducerIdentity, MutationProposal, ObjectChange, OperationId,
    RangeEditCoordinator, SourcePosition, SourceRange, SourceRevision, SuccessorObject,
};

pub fn insert_published_marker(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: &AssetState,
    binding: ComposerHostBinding,
    operation: u64,
    asset: AssetId,
) -> (ComposerHostBinding, SourcePosition, SourcePosition) {
    let object = InlineObjectId::new(0x1001);
    let neighbor = InlineObjectNeighbor::new(object, InlineObjectOrder::new(1));
    let before = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::before(neighbor));
    let after = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::after(neighbor));
    let binding = insert_at(
        host,
        store,
        assets,
        binding,
        operation,
        SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects),
        object,
        1,
        asset,
    );
    (binding, before, after)
}

pub fn insert_two_markers(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: &AssetState,
    binding: ComposerHostBinding,
    operation: u64,
    marker_assets: [AssetId; 2],
) -> ComposerHostBinding {
    let (binding, _, after) =
        insert_published_marker(host, store, assets, binding, operation, marker_assets[0]);
    insert_at(
        host,
        store,
        assets,
        binding,
        operation + 1,
        after,
        InlineObjectId::new(0x1002),
        2,
        marker_assets[1],
    )
}

pub fn insert_later_marker(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: &AssetState,
    binding: ComposerHostBinding,
    operation: u64,
) -> ComposerHostBinding {
    let point = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(
            InlineObjectId::new(0x1002),
            InlineObjectOrder::new(2),
        )),
    );
    let asset = super::publication::publish_image_asset(store, assets.clone(), b"later-marker");
    insert_at(
        host,
        store,
        assets,
        binding,
        operation,
        point,
        InlineObjectId::new(0x1003),
        3,
        asset,
    )
}

fn insert_at(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: &AssetState,
    binding: ComposerHostBinding,
    operation: u64,
    point: SourcePosition,
    object: InlineObjectId,
    order: u128,
    asset: AssetId,
) -> ComposerHostBinding {
    let key = MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
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
    )
    .with_replayable_producer(MutationProducerIdentity::new(operation + 1_000));
    let mut editor = RangeEditCoordinator::new(
        binding.range_binding(),
        MutationLimits::new(16, 65_536)
            .unwrap()
            .with_object_limits(16, 65_536, 65_536)
            .unwrap(),
    );
    editor.begin(begin).unwrap();
    let pass = editor.request_evidence(key).unwrap();
    assert!(
        matches!(drive(host, store, assets, binding, ComposerHostMutationEvidenceRequest::Begin { begin, pass }), ComposerHostMutationEvidenceOutcome::Started(actual) if actual == pass)
    );
    let frontier = editor.stream_finish(key, MutationLane::Proposal).unwrap();
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            frontier.next_cursor,
            frontier.next_ordinal,
            frontier.cumulative_identity,
        ),
        MutationCursor::new(frontier.next_cursor.get() + 1),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(
                object,
                point.byte_offset,
                InlineObjectOrder::new(order),
                17,
                5,
            ),
        })],
    )
    .unwrap();
    let acknowledgement = editor.submit_evidence_page(pass, page.clone()).unwrap();
    assert!(
        matches!(drive(host, store, assets, binding, ComposerHostMutationEvidenceRequest::Page { pass, page: page.clone(), metadata: Box::new([ComposerHostImageMarkerMetadata::new(object, asset)]) }), ComposerHostMutationEvidenceOutcome::PageAccepted(actual) if actual == pass)
    );
    editor.acknowledge_evidence_page(acknowledgement).unwrap();
    let after = SourcePosition::new(
        point.byte_offset,
        InlineObjectGap::after(InlineObjectNeighbor::new(
            object,
            InlineObjectOrder::new(order),
        )),
    );
    let finish = MutationFinishInput::new(
        key,
        editor.stream_finish(key, MutationLane::Source).unwrap(),
        editor.stream_finish(key, MutationLane::Proposal).unwrap(),
        binding.range_binding().extent(),
        MutationPositions::collapsed(after),
    );
    editor.finish_evidence(pass, finish).unwrap();
    assert!(
        matches!(drive(host, store, assets, binding, ComposerHostMutationEvidenceRequest::Finish { pass, finish }), ComposerHostMutationEvidenceOutcome::Began(actual) if actual == key)
    );
    editor.accept_preflight(key, None).unwrap();
    let staging = editor.mutation_restart(key).unwrap();
    editor.acknowledge_restart(staging).unwrap();
    assert!(matches!(
        editor.accept_pass_page(staging, page.clone()),
        Ok(MutationPageAcceptance::Accepted { .. })
    ));
    host.stage_mutation_page(
        store,
        MutationPageRequest::new(page).with_pass(staging),
        Box::new([ComposerHostImageMarkerMetadata::new(object, asset)]),
    )
    .unwrap();
    editor.finish_pass_input(staging, finish).unwrap();
    host.finish_mutation_input(store, finish).unwrap();
    for _ in 0..64 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("publication marker did not commit: {other:?}"),
        }
    }
    panic!("publication marker exceeded its bounded work budget")
}

fn drive(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    assets: &AssetState,
    binding: ComposerHostBinding,
    request: ComposerHostMutationEvidenceRequest,
) -> ComposerHostMutationEvidenceOutcome {
    let cancellation = CommandCancellation::new();
    let mut request = Some(request);
    for _ in 0..64 {
        match host
            .dispatch_mutation_evidence(
                store,
                binding,
                assets,
                request.take().unwrap(),
                &cancellation,
            )
            .unwrap()
        {
            ComposerHostMutationEvidenceOutcome::Pending(key) => {
                request = Some(ComposerHostMutationEvidenceRequest::Advance(key))
            }
            outcome => return outcome,
        }
    }
    panic!("publication marker evidence exceeded its bounded work budget")
}

use std::num::{NonZeroU64, NonZeroUsize};

use beryl_app::{
    composer_host::{
        ComposerHostBinding, ComposerHostError, ComposerHostImageMarkerMetadata,
        ComposerHostMarkerSealAuthority, ComposerHostMutationOutcome, SyndicComposerHost,
    },
    composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits},
};
use beryl_home_store::{
    CommandCancellation, CommandOutcome, HomeCommand, HomeStore, SidecarByteLimit, SidecarNamespace,
};
use beryl_model::{AssetId, AssetReferenceSetId, ImageLabelOrdinal, SyndicDraftMarkerId};
use beryl_state::{
    AssetMediaType, AssetReferenceSetStagingAuthority, AssetState, PublishAssetMetadata,
};
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
    DraftMarkerAdmissionOwnerV1, DraftMarkerSealOperationIdV1, DraftPieceMarkerV1, SyndicStorage,
};

pub fn service(
    store: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
    flights: usize,
    page: usize,
) -> DraftMarkerSealService {
    DraftMarkerSealService::new(
        store,
        store.health().generation().unwrap(),
        storage,
        assets,
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(flights).unwrap(),
            NonZeroUsize::new(page).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

pub fn authority(seed: u8) -> ComposerHostMarkerSealAuthority {
    ComposerHostMarkerSealAuthority::new(
        DraftMarkerSealOperationIdV1::from_bytes([seed; 16]),
        AssetReferenceSetStagingAuthority::new(
            AssetReferenceSetId::from_bytes([seed.wrapping_add(1); 16]),
            [seed.wrapping_add(2); 32],
        ),
    )
}

pub fn insert_two_markers(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    assets: [AssetId; 2],
) -> ComposerHostBinding {
    let point = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects);
    let first = InlineObjectId::new(0x1001);
    let first_after = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(first, InlineObjectOrder::new(1))),
    );
    let binding = insert_marker_at(
        host, store, None, binding, operation, point, first, 1, assets[0],
    );
    insert_marker_at(
        host,
        store,
        None,
        binding,
        operation + 1,
        first_after,
        InlineObjectId::new(0x1002),
        2,
        assets[1],
    )
}

pub fn insert_published_marker(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    asset: AssetId,
) -> (ComposerHostBinding, SourcePosition, SourcePosition) {
    let object = InlineObjectId::new(0x1001);
    let neighbor = InlineObjectNeighbor::new(object, InlineObjectOrder::new(1));
    let before = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::before(neighbor));
    let after = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::after(neighbor));
    (
        insert_marker_at(
            host,
            store,
            None,
            binding,
            operation,
            SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects),
            object,
            1,
            asset,
        ),
        before,
        after,
    )
}

pub fn insert_published_marker_with_readiness(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    storage: &SyndicStorage,
    binding: ComposerHostBinding,
    operation: u64,
    asset: AssetId,
) -> (ComposerHostBinding, SourcePosition, SourcePosition) {
    let object = InlineObjectId::new(0x1001);
    let neighbor = InlineObjectNeighbor::new(object, InlineObjectOrder::new(1));
    let before = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::before(neighbor));
    let after = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::after(neighbor));
    (
        insert_marker_at(
            host,
            store,
            Some(storage),
            binding,
            operation,
            SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects),
            object,
            1,
            asset,
        ),
        before,
        after,
    )
}

pub fn insert_text_after_published_marker(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
) -> ComposerHostBinding {
    let neighbor =
        InlineObjectNeighbor::new(InlineObjectId::new(0x1001), InlineObjectOrder::new(1));
    let point = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::after(neighbor));
    let key = gpui_text_input::MutationKey::new(
        gpui_text_input::BindingId::new(binding.host_generation().get()),
        gpui_text_input::SourceRevision::new(binding.candidate().candidate_generation()),
        gpui_text_input::OperationId::new(operation),
    );
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(point),
                SourceRange::new(point, point).unwrap(),
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
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: "x".into(),
        }],
    )
    .unwrap();
    let proposal_finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    host.stage_mutation_page(store, MutationPageRequest::new(page), Box::new([]))
        .unwrap();
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
            proposal_finish,
            LogicalExtent::new(1, 1),
            MutationPositions::collapsed(SourcePosition::new(
                ByteOffset::new(1),
                InlineObjectGap::NoObjects,
            )),
        ),
    )
    .unwrap();
    for _ in 0..16 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("text mutation did not commit: {other:?}"),
        }
    }
    panic!("text mutation remained pending")
}

pub fn insert_later_marker(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
) -> ComposerHostBinding {
    let second = InlineObjectId::new(0x1002);
    let after_second = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(second, InlineObjectOrder::new(2))),
    );
    insert_marker_at(
        host,
        store,
        None,
        binding,
        operation,
        after_second,
        InlineObjectId::new(0x1003),
        3,
        metadata_asset(InlineObjectId::new(0x1003)),
    )
}

fn insert_marker_at(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    readiness_storage: Option<&SyndicStorage>,
    binding: ComposerHostBinding,
    operation: u64,
    point: SourcePosition,
    object: InlineObjectId,
    order: u128,
    asset: AssetId,
) -> ComposerHostBinding {
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
    let readiness_owner = if let Some(storage) = readiness_storage {
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
            u64::try_from(order).unwrap(),
            ImageLabelOrdinal::new(u64::try_from(order).unwrap()).unwrap(),
            asset,
        );
        let readiness = storage
            .seed_draft_marker_writer_ready_target_for_test(store, &session, owner, marker)
            .unwrap();
        host.test_begin_marker_mutation(store, binding, begin, readiness)
            .unwrap();
        Some(owner)
    } else {
        host.begin_mutation(store, binding, begin).unwrap();
        None
    };

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
            object: SuccessorObject::new(
                object,
                ByteOffset::new(0),
                InlineObjectOrder::new(order),
                17,
                5,
            ),
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
        Box::from([metadata(object, u64::try_from(order).unwrap(), asset)]),
    )
    .unwrap();
    let after = SourcePosition::new(
        ByteOffset::new(0),
        InlineObjectGap::after(InlineObjectNeighbor::new(
            object,
            InlineObjectOrder::new(order),
        )),
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
                if let (Some(storage), Some(owner)) = (readiness_storage, readiness_owner) {
                    storage
                        .release_settled_draft_marker_writer(store, owner)
                        .unwrap();
                }
                return binding;
            }
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("marker mutation did not commit: {other:?}"),
        }
    }
    panic!("marker mutation remained pending")
}

pub fn publish_image_asset(store: &HomeStore, assets: AssetState, bytes: &[u8]) -> AssetId {
    let sidecar = store
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
    let expected = assets.revision(store).unwrap();
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
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    asset
}

fn metadata_asset(id: InlineObjectId) -> AssetId {
    let mut digest = [0; 32];
    digest[..16].copy_from_slice(&id.get().to_be_bytes());
    digest[16..].copy_from_slice(&id.get().to_be_bytes());
    AssetId::sha256_v1(digest, NonZeroU64::MIN)
}

fn metadata(id: InlineObjectId, label: u64, asset: AssetId) -> ComposerHostImageMarkerMetadata {
    ComposerHostImageMarkerMetadata::new(id, ImageLabelOrdinal::new(label).unwrap(), asset)
}

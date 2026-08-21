use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostBinding,
    ComposerHostError, ComposerHostImageMarkerMetadata, ComposerHostMutationOutcome,
    SyndicComposerHost,
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::SyndicThreadId;
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationPage,
    MutationPageItem, MutationPageKey, MutationPageRequest, MutationPositions, MutationProposal,
    MutationStreamFinish, ObjectChange, OperationId, SourcePosition, SourceRange, SourceRevision,
    SuccessorObject,
};
use syndic_storage::{
    DraftEditorCandidateSessionIdV1, DraftPieceMarkerV1, DraftPieceOperationIdV1, SyndicStorage,
};

use super::marker;

pub fn populate(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    session_seed: u8,
) -> (DraftPieceMarkerV1, DraftPieceMarkerV1) {
    let left = marker(session_seed.wrapping_add(1), 1);
    let right = marker(session_seed.wrapping_add(2), 2);
    let mut host = SyndicComposerHost::new(storage);
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .activate(
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
    let left_id = InlineObjectId::new(u128::from_be_bytes(*left.marker_id().as_bytes()));
    let right_id = InlineObjectId::new(u128::from_be_bytes(*right.marker_id().as_bytes()));
    let left_order = InlineObjectOrder::new(left.order_key().into());
    let right_order = InlineObjectOrder::new(right.order_key().into());
    let after_right = SourcePosition::new(
        ByteOffset::new(3),
        InlineObjectGap::after(InlineObjectNeighbor::new(right_id, right_order)),
    );
    let binding = commit_page(
        &mut host,
        store,
        binding,
        u64::from(session_seed) + 2,
        SourceRange::new(position(3), position(3)).unwrap(),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(right_id, ByteOffset::new(3), right_order, 17, 5),
        })],
        vec![ComposerHostImageMarkerMetadata::new(
            right_id,
            right.label(),
        )]
        .into_boxed_slice(),
        LogicalExtent::new(6, 3),
        MutationPositions::collapsed(after_right),
    );
    let before_right = SourcePosition::new(
        ByteOffset::new(3),
        InlineObjectGap::before(InlineObjectNeighbor::new(right_id, right_order)),
    );
    commit_page(
        &mut host,
        store,
        binding,
        u64::from(session_seed) + 3,
        SourceRange::new(before_right, before_right).unwrap(),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(left_id, ByteOffset::new(3), left_order, 17, 5),
        })],
        vec![ComposerHostImageMarkerMetadata::new(left_id, left.label())].into_boxed_slice(),
        LogicalExtent::new(6, 3),
        MutationPositions::collapsed(after_right),
    );
    (left, right)
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

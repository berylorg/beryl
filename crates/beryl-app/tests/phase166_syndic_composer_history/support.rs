use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostBinding,
    ComposerHostError, ComposerHostImageMarkerMetadata, ComposerHostMutationOutcome,
    SyndicComposerHost,
};
use beryl_home_store::{CommandCancellation, HomeCommand, HomeStore};
use beryl_model::{AssetId, ImageLabelOrdinal};
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationPage,
    MutationPageItem, MutationPageKey, MutationPageRequest, MutationPositions, MutationProposal,
    MutationStreamFinish, MutationTotals, ObjectChange, ObjectTarget, OperationId,
    RangeHistoryIntent, RangeHistoryOutcome, RangeSourceSelection, SourcePosition, SourceRange,
    SourceRevision, SuccessorObject,
};
use syndic_storage::{
    DraftEditorCandidateSessionIdV1, DraftHistoricalRootSelectionIntentV1,
    DraftHistoricalRootSelectionV1, DraftPieceOperationIdV1, DraftPieceTextDemandV1, SyndicStorage,
};

pub fn activated(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
) -> (SyndicComposerHost, ComposerHostBinding) {
    let mut host = SyndicComposerHost::new(storage);
    let binding = reactivate(&mut host, store, thread, session, operation);
    (host, binding)
}

pub fn reactivate(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
) -> ComposerHostBinding {
    let request = ComposerHostActivationRequest::new(
        thread,
        DraftEditorCandidateSessionIdV1::from_bytes([session; 16]),
        DraftPieceOperationIdV1::from_bytes([operation; 16]),
        NonZeroU64::MIN,
        None,
        Box::new([]),
    );
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .activate(store, request, &CommandCancellation::new())
        .unwrap()
    else {
        panic!("activation did not yield a composer binding")
    };
    binding
}

#[allow(clippy::too_many_arguments)]
pub fn commit_text(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    start: u64,
    end: u64,
    text: &str,
    caret: u64,
    lines: u64,
) -> ComposerHostBinding {
    let key = mutation_key(binding, operation);
    let proposal = MutationProposal::new(
        key,
        MutationKind::Edit,
        MutationPositions::collapsed(position(start)),
        SourceRange::new(position(start), position(end)).unwrap(),
        0,
    );
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0)),
    )
    .unwrap();
    let proposal_finish = if text.is_empty() {
        empty_finish()
    } else {
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
                text: text.into(),
            }],
        )
        .unwrap();
        let finish = MutationStreamFinish {
            next_cursor: page.next_cursor(),
            next_ordinal: 1,
            cumulative_identity: page.cumulative_identity(),
            totals: page.totals(),
        };
        host.stage_mutation_page(store, MutationPageRequest::new(page), Box::new([]))
            .unwrap();
        finish
    };
    host.finish_mutation_input(
        store,
        MutationFinishInput::new(
            key,
            empty_finish(),
            proposal_finish,
            LogicalExtent::new(caret, lines),
            MutationPositions::collapsed(position(caret)),
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
            other => panic!("mutation did not commit: {other:?}"),
        }
    }
    panic!("mutation remained pending")
}

pub fn begin_text(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    offset: u64,
) -> Result<(), ComposerHostError> {
    let key = mutation_key(binding, operation);
    let caret = position(offset);
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(caret),
                SourceRange::new(caret, caret).unwrap(),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
}

pub fn insert_marker(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    after: bool,
) -> (ComposerHostBinding, SourcePosition, SourcePosition) {
    let id = InlineObjectId::new(0x8001_0203_0405_0607_0809_0a0b_0c0d_0eff);
    let order = InlineObjectOrder::new(1);
    let neighbor = InlineObjectNeighbor::new(id, order);
    let before = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::before(neighbor));
    let after_position = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::after(neighbor));
    let intended = if after { after_position } else { before };
    let binding = commit_items(
        host,
        store,
        binding,
        operation,
        SourceRange::new(position(0), position(0)).unwrap(),
        position(0),
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(id, ByteOffset::new(0), order, 17, 5),
        })],
        MutationPositions::collapsed(intended),
        vec![ComposerHostImageMarkerMetadata::new(
            id,
            ImageLabelOrdinal::new(1).unwrap(),
            asset_id_for_object(id),
        )],
    );
    (binding, before, after_position)
}

fn asset_id_for_object(id: InlineObjectId) -> AssetId {
    let bytes = id.get().to_be_bytes();
    let mut digest = [0; 32];
    digest[..16].copy_from_slice(&bytes);
    digest[16..].copy_from_slice(&bytes);
    AssetId::sha256_v1(digest, NonZeroU64::MIN)
}

pub fn remove_marker(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    before: SourcePosition,
    after: SourcePosition,
    current: SourcePosition,
) -> ComposerHostBinding {
    let neighbor = match after.gap {
        InlineObjectGap::After(neighbor) => neighbor,
        _ => unreachable!(),
    };
    let target = ObjectTarget::new(
        SourceRange::new(before, after).unwrap(),
        neighbor.id(),
        neighbor.order(),
    )
    .unwrap();
    commit_items(
        host,
        store,
        binding,
        operation,
        target.range(),
        current,
        vec![MutationPageItem::Object(ObjectChange::Remove { target })],
        MutationPositions::collapsed(position(0)),
        Vec::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_items(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    replacement: SourceRange,
    current: SourcePosition,
    items: Vec<MutationPageItem>,
    intended: MutationPositions,
    marker_metadata: Vec<ComposerHostImageMarkerMetadata>,
) -> ComposerHostBinding {
    let key = mutation_key(binding, operation);
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(current),
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
    host.stage_mutation_page(
        store,
        MutationPageRequest::new(page),
        marker_metadata.into_boxed_slice(),
    )
    .unwrap();
    host.finish_mutation_input(
        store,
        MutationFinishInput::new(
            key,
            empty_finish(),
            proposal_finish,
            LogicalExtent::new(0, 1),
            intended,
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
            other => panic!("object mutation did not commit: {other:?}"),
        }
    }
    panic!("object mutation remained pending")
}

pub fn history_intent(
    binding: ComposerHostBinding,
    operation: u64,
    kind: MutationKind,
    caret: SourcePosition,
) -> RangeHistoryIntent {
    RangeHistoryIntent::new(
        mutation_key(binding, operation),
        binding.range_binding(),
        kind,
        binding.range_history_frontier(),
        caret,
        RangeSourceSelection::caret(caret),
    )
}

pub fn select_history(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    kind: MutationKind,
) -> ComposerHostBinding {
    let caret = position(binding.logical_extent().logical_utf8_bytes());
    let intent = history_intent(binding, operation, kind, caret);
    host.begin_history_selection(store, binding, intent)
        .unwrap();
    let RangeHistoryOutcome::Committed(commit) = host
        .execute_history_selection(store, intent.key(), &CommandCancellation::new())
        .unwrap()
    else {
        panic!("history selection did not commit")
    };
    assert_eq!(commit.binding(), host.binding().unwrap().range_binding());
    host.binding().unwrap()
}

pub fn candidate_text(
    storage: SyndicStorage,
    store: &HomeStore,
    binding: ComposerHostBinding,
) -> Vec<u8> {
    let mut offset = 0;
    let mut bytes = Vec::new();
    loop {
        let page = storage
            .candidate_draft_piece_text_demand(
                store,
                binding.candidate(),
                DraftPieceTextDemandV1::Forward(offset),
                65_536,
            )
            .unwrap()
            .value()
            .clone();
        bytes.extend_from_slice(page.bytes());
        match page.following() {
            syndic_storage::DraftPieceTextEdgeFactV1::Continuation(next) => offset = next,
            syndic_storage::DraftPieceTextEdgeFactV1::DocumentEnd => return bytes,
            _ => panic!("forward page returned a preceding edge"),
        }
    }
}

pub fn direct_adopt(
    store: &HomeStore,
    storage: SyndicStorage,
    intent: DraftHistoricalRootSelectionIntentV1,
) {
    let DraftHistoricalRootSelectionV1::Prepared(prepared) = storage
        .prepare_draft_historical_root_selection(store, intent)
        .unwrap()
    else {
        panic!("history unexpectedly unavailable")
    };
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.adopt_draft_historical_root(storage.revision(store).unwrap(), prepared))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        beryl_home_store::CommandOutcome::Committed { .. }
    ));
}

pub fn operation_id(operation: u64) -> DraftPieceOperationIdV1 {
    let mut bytes = [0; 16];
    bytes[8..].copy_from_slice(&operation.to_be_bytes());
    DraftPieceOperationIdV1::from_bytes(bytes)
}

fn mutation_key(binding: ComposerHostBinding, operation: u64) -> MutationKey {
    MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
    )
}

pub fn position(offset: u64) -> SourcePosition {
    SourcePosition::new(ByteOffset::new(offset), InlineObjectGap::NoObjects)
}

fn empty_finish() -> MutationStreamFinish {
    MutationStreamFinish {
        next_cursor: MutationCursor::new(0),
        next_ordinal: 0,
        cumulative_identity: MutationIdentity::ROOT,
        totals: MutationTotals::default(),
    }
}

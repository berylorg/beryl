#[path = "phase141_syndic_composer_host/support.rs"]
mod support;

use std::num::NonZeroU64;

use beryl_app::composer_host::{
    ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostBinding,
    ComposerHostError, ComposerHostImageMarkerMetadata, ComposerHostMutationOutcome,
    ComposerHostMutationStatus, SyndicComposerHost,
};
use beryl_home_store::CommandCancellation;
use beryl_model::{AssetId, ImageLabelOrdinal, SyndicDraftMarkerId};
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationPage,
    MutationPageAcceptance, MutationPageItem, MutationPageKey, MutationPageRequest,
    MutationPositions, MutationProposal, MutationStreamFinish, MutationTotals, ObjectChange,
    ObjectTarget, OperationId, SourcePosition, SourceRange, SourceRevision, SuccessorObject,
};
use syndic_storage::{
    DraftEditorCandidateSessionIdV1, DraftEditorCandidateSessionReadOutcomeV1,
    DraftMutationOperationIdV1, DraftMutationStagingIdentityV1, DraftMutationStagingStatusV1,
    DraftPieceMarkerAtV1, DraftPieceMarkerV1, DraftPieceOperationIdV1, DraftPieceReplacementV1,
    DraftPieceTextDemandV1, DraftPieceV1, SyndicStorage,
};

use support::{current, fixture, point, reopen, run_transaction, transaction_for_session};

#[path = "phase143_syndic_composer_mutations/empty_custody.rs"]
mod empty_custody;
#[path = "phase143_syndic_composer_mutations/marker_conflict.rs"]
mod marker_conflict;
#[path = "phase143_syndic_composer_mutations/metadata_intake.rs"]
mod metadata_intake;
#[path = "phase143_syndic_composer_mutations/outcomes.rs"]
mod outcomes;
#[path = "phase143_syndic_composer_mutations/protocol_pages.rs"]
mod protocol_pages;

fn commit_items(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    replacement: SourceRange,
    items: Vec<MutationPageItem>,
    intended: MutationPositions,
    marker_metadata: Vec<ComposerHostImageMarkerMetadata>,
    byte_len: u64,
    line_count: u64,
) -> ComposerHostBinding {
    let key = mutation_key(binding, operation);
    let proposal = MutationProposal::new(
        key,
        MutationKind::Edit,
        MutationPositions::collapsed(replacement.start()),
        replacement,
        0,
    );
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(proposal, MutationCursor::new(0), MutationCursor::new(0)),
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
            LogicalExtent::new(byte_len, line_count),
            intended,
        ),
    )
    .unwrap();
    commit(host, store, key)
}

fn assert_marker(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    marker_id: SyndicDraftMarkerId,
    anchor: u64,
    order: u64,
    label: ImageLabelOrdinal,
    asset_id: AssetId,
) {
    let marker = DraftPieceMarkerV1::new(marker_id, order, label, asset_id);
    assert!(
        storage
            .validate_draft_marker_location(
                store,
                binding.root(),
                DraftPieceMarkerAtV1::new(anchor, marker),
            )
            .unwrap()
    );
}

fn asset_id_for_object(id: InlineObjectId) -> AssetId {
    let bytes = id.get().to_be_bytes();
    let mut digest = [0; 32];
    digest[..16].copy_from_slice(&bytes);
    digest[16..].copy_from_slice(&bytes);
    AssetId::sha256_v1(digest, NonZeroU64::MIN)
}

fn commit_text(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    start: u64,
    end: u64,
    text: &str,
    caret: u64,
    lines: u64,
) -> ComposerHostBinding {
    let (key, finish) = stage_text(
        host, store, binding, operation, start, end, text, caret, lines,
    );
    host.finish_mutation_input(store, finish).unwrap();
    commit(host, store, key)
}

fn stage_text(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    operation: u64,
    start: u64,
    end: u64,
    text: &str,
    caret: u64,
    lines: u64,
) -> (MutationKey, MutationFinishInput) {
    let key = mutation_key(binding, operation);
    let predecessor = MutationPositions::collapsed(source_position(start));
    let proposal = MutationProposal::new(
        key,
        MutationKind::Edit,
        predecessor,
        range(source_position(start), source_position(end)),
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
    (
        key,
        finish_input(key, empty_finish(), proposal_finish, caret, lines),
    )
}

fn finish_input(
    key: MutationKey,
    source: MutationStreamFinish,
    proposal: MutationStreamFinish,
    caret: u64,
    lines: u64,
) -> MutationFinishInput {
    MutationFinishInput::new(
        key,
        source,
        proposal,
        LogicalExtent::new(caret, lines),
        MutationPositions::collapsed(source_position(caret)),
    )
}

fn commit(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    key: MutationKey,
) -> ComposerHostBinding {
    for _ in 0..16 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => continue,
            other => panic!("mutation did not commit: {other:?}"),
        }
    }
    panic!("mutation remained pending after sixteen bounded drives")
}

fn activated(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
) -> (SyndicComposerHost, ComposerHostBinding) {
    let mut host = SyndicComposerHost::new(storage);
    let binding = reactivate(&mut host, storage, store, thread, session, operation);
    (host, binding)
}

fn reactivate(
    host: &mut SyndicComposerHost,
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: beryl_model::SyndicThreadId,
    session: u8,
    operation: u8,
) -> ComposerHostBinding {
    if host.binding().is_some() {
        host.dispose_composer_service(store).unwrap();
        *host = SyndicComposerHost::new(storage);
    }
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
        panic!("activation did not yield a composer binding");
    };
    binding
}

fn mutation_key(binding: ComposerHostBinding, operation: u64) -> MutationKey {
    MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
    )
}

fn staging_identity(
    binding: ComposerHostBinding,
    operation: u64,
) -> DraftMutationStagingIdentityV1 {
    let mut bytes = [0; 16];
    bytes[8..].copy_from_slice(&operation.to_be_bytes());
    DraftMutationStagingIdentityV1::new(
        binding.candidate().draft_id(),
        binding.candidate().session_id(),
        DraftMutationOperationIdV1::from_bytes(bytes),
    )
}

fn source_position(offset: u64) -> SourcePosition {
    SourcePosition::new(ByteOffset::new(offset), InlineObjectGap::NoObjects)
}

fn range(start: SourcePosition, end: SourcePosition) -> SourceRange {
    SourceRange::new(start, end).unwrap()
}

fn empty_finish() -> MutationStreamFinish {
    MutationStreamFinish {
        next_cursor: MutationCursor::new(0),
        next_ordinal: 0,
        cumulative_identity: MutationIdentity::ROOT,
        totals: MutationTotals::default(),
    }
}

fn candidate_text(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
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

fn add_totals(left: MutationTotals, right: MutationTotals) -> MutationTotals {
    MutationTotals {
        pages: left.pages + right.pages,
        items: left.items + right.items,
        retained_bytes: left.retained_bytes + right.retained_bytes,
        inserted_bytes: left.inserted_bytes + right.inserted_bytes,
        inserted_line_breaks: left.inserted_line_breaks + right.inserted_line_breaks,
        objects: left.objects + right.objects,
        object_bytes: left.object_bytes + right.object_bytes,
        presentation_bytes: left.presentation_bytes + right.presentation_bytes,
    }
}

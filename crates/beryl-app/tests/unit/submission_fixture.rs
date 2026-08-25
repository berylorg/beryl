use std::num::NonZeroUsize;

use crate::{
    composer_host::{
        ComposerHostActivationOutcome, ComposerHostActivationRequest, ComposerHostBinding,
        ComposerHostImageMarkerMetadata, ComposerHostMarkerSealAuthority,
        ComposerHostMutationOutcome, ComposerHostSubmissionAdvance, ComposerHostSubmissionRequest,
        SyndicComposerHost,
    },
    composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits},
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{
    AssetId, AssetReferenceSetId, ImageLabelOrdinal, SyndicDraftId, SyndicDraftMarkerId,
    SyndicItemId, SyndicThreadId,
};
use beryl_state::{AssetReferenceSetStagingAuthority, AssetState};
use gpui_text_input::{
    BindingId, ByteOffset, InlineObjectGap, InlineObjectId, InlineObjectNeighbor,
    InlineObjectOrder, LogicalExtent, MutationBeginRequest, MutationCommitRequest,
    MutationCursor, MutationFinishInput, MutationIdentity, MutationKey, MutationKind,
    MutationLane, MutationPage, MutationPageItem, MutationPageKey, MutationPageRequest,
    MutationPositions, MutationProposal, MutationStreamFinish, MutationTotals, SourcePosition,
    SourceRange, SourceRevision, SuccessorObject,
};
use syndic_storage::{
    DraftComposerMaterializationOperationIdV1, DraftEditorCandidateSessionIdV1,
    DraftMarkerSealOperationIdV1, DraftPieceOperationIdV1, FirstAcceptanceKind, SyndicStorage,
    SyndicTimestamp,
};

pub enum Atom<'a> {
    Text(&'a str),
    Image(ImageLabelOrdinal, AssetId),
}

pub fn submit_atoms(
    store: &HomeStore,
    storage: SyndicStorage,
    assets: AssetState,
    thread: SyndicThreadId,
    next_draft: SyndicDraftId,
    item: SyndicItemId,
    atoms: &[Atom<'_>],
    seed: u8,
    admitted_at: SyndicTimestamp,
) -> (FirstAcceptanceKind, SyndicDraftId) {
    let mut host = SyndicComposerHost::new(storage);
    let request = ComposerHostActivationRequest::new(
        thread,
        DraftEditorCandidateSessionIdV1::from_bytes([seed; 16]),
        DraftPieceOperationIdV1::from_bytes([seed.wrapping_add(1); 16]),
        std::num::NonZeroU64::MIN,
        None,
        Box::new([]),
    );
    let ComposerHostActivationOutcome::Activated { binding, .. } = host
        .test_activate(store, request, &CommandCancellation::new())
        .unwrap()
    else {
        panic!("submission fixture activation did not produce a binding")
    };
    let binding = commit_atoms(&mut host, store, binding, atoms);
    let source_draft = binding.candidate().draft_id();
    let seals = DraftMarkerSealService::new(
        store,
        store.health().generation().unwrap(),
        storage,
        assets,
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::MIN,
            NonZeroUsize::MIN,
        )
        .unwrap(),
    )
    .unwrap();
    let marker_authority = atoms.iter().any(|atom| matches!(atom, Atom::Image(..))).then(|| {
        ComposerHostMarkerSealAuthority::new(
            DraftMarkerSealOperationIdV1::from_bytes([seed.wrapping_add(2); 16]),
            AssetReferenceSetStagingAuthority::new(
                AssetReferenceSetId::from_bytes([seed.wrapping_add(3); 16]),
                [seed.wrapping_add(4); 32],
            ),
        )
    });
    let ticket = host
        .begin_submission(ComposerHostSubmissionRequest::new(
            next_draft,
            item,
            DraftComposerMaterializationOperationIdV1::from_bytes([seed.wrapping_add(5); 16]),
            DraftPieceOperationIdV1::from_bytes([seed.wrapping_add(6); 16]),
            admitted_at,
            submission_admission_requirement(),
        ))
        .unwrap();
    for _ in 0..256 {
        match host
            .advance_submission(
                store,
                ticket,
                assets,
                &seals,
                DraftPieceOperationIdV1::from_bytes([seed.wrapping_add(7); 16]),
                marker_authority,
                admitted_at,
                &CommandCancellation::new(),
            )
            .unwrap()
        {
            ComposerHostSubmissionAdvance::Progress(_)
            | ComposerHostSubmissionAdvance::ReconciliationPending => {}
            ComposerHostSubmissionAdvance::ExactSuccess(kind) => return (kind, source_draft),
            outcome => panic!("submission fixture did not commit exactly: {outcome:?}"),
        }
    }
    panic!("submission fixture did not converge")
}

fn submission_admission_requirement() -> beryl_home_store::TurnStartAdmissionRequirement {
    crate::cas_projection::ProjectionServiceConfig::try_new(
        1,
        4,
        beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
    )
    .unwrap()
    .turn_start_admission_requirement()
}

fn commit_atoms(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: ComposerHostBinding,
    atoms: &[Atom<'_>],
) -> ComposerHostBinding {
    let key = MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        gpui_text_input::OperationId::new(1),
    );
    let origin = SourcePosition::new(ByteOffset::new(0), InlineObjectGap::NoObjects);
    host.begin_mutation(
        store,
        binding,
        MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(origin),
                SourceRange::new(origin, origin).unwrap(),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        ),
    )
    .unwrap();
    let mut page_items = Vec::with_capacity(atoms.len());
    let mut metadata = Vec::new();
    let mut text_bytes = 0_u64;
    let mut lines = 1_u64;
    let mut last_neighbor = None;
    for atom in atoms {
        match atom {
            Atom::Text(text) => {
                page_items.push(MutationPageItem::Utf8 {
                    inserted_offset: text_bytes,
                    text: (*text).into(),
                });
                text_bytes = text_bytes.checked_add(text.len() as u64).unwrap();
                lines = lines
                    .checked_add(text.bytes().filter(|byte| *byte == b'\n').count() as u64)
                    .unwrap();
            }
            Atom::Image(label, asset) => {
                let ordinal = u64::try_from(metadata.len() + 1).unwrap();
                let marker = marker_id(binding.candidate().draft_id(), ordinal);
                let object = InlineObjectId::new(u128::from_be_bytes(*marker.as_bytes()));
                let order = InlineObjectOrder::new(u128::from(ordinal));
                page_items.push(MutationPageItem::Object(
                    gpui_text_input::ObjectChange::Insert {
                        object: SuccessorObject::new(
                            object,
                            ByteOffset::new(text_bytes),
                            order,
                            17,
                            5,
                        ),
                    },
                ));
                metadata.push(ComposerHostImageMarkerMetadata::new(object, *label, *asset));
                last_neighbor = Some(InlineObjectNeighbor::new(object, order));
            }
        }
    }
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        page_items,
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
        metadata.into_boxed_slice(),
    )
    .unwrap();
    let caret = SourcePosition::new(
        ByteOffset::new(text_bytes),
        last_neighbor.map_or(InlineObjectGap::NoObjects, InlineObjectGap::after),
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
            LogicalExtent::new(text_bytes, lines),
            MutationPositions::collapsed(caret),
        ),
    )
    .unwrap();
    for _ in 0..64 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(crate::composer_host::ComposerHostError::MutationWorkPending) => {}
            outcome => panic!("submission fixture mutation did not commit: {outcome:?}"),
        }
    }
    panic!("submission fixture mutation did not converge")
}

fn marker_id(draft: SyndicDraftId, ordinal: u64) -> SyndicDraftMarkerId {
    let mut bytes = *draft.as_bytes();
    bytes[8..].copy_from_slice(&ordinal.to_be_bytes());
    SyndicDraftMarkerId::from_bytes(bytes)
}

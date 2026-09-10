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
    InlineObjectOrder, LogicalExtent, MutationBeginRequest, MutationCommitRequest, MutationCursor,
    MutationFinishInput, MutationIdentity, MutationKey, MutationKind, MutationLane, MutationPage,
    MutationPageItem, MutationPageKey, MutationPageRequest, MutationPositions, MutationProposal,
    MutationStreamFinish, MutationTotals, SourcePosition, SourceRange, SourceRevision,
    SuccessorObject,
};
use syndic_storage::{
    DraftComposerMaterializationOperationIdV1, DraftEditorCandidateSessionIdV1,
    DraftMarkerSealOperationIdV1, DraftPieceOperationIdV1, FirstAcceptanceKind, SyndicStorage,
    SyndicTimestamp,
};

#[path = "submission_fixture/marker_readiness.rs"]
mod marker_readiness;

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
    let mut host = SyndicComposerHost::new(storage.clone());
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
    let binding = commit_atoms(&mut host, store, &assets, binding, atoms);
    let source_draft = binding.candidate().draft_id();
    let seals = DraftMarkerSealService::new(
        store,
        store.health().generation().unwrap(),
        storage,
        assets.clone(),
        DraftMarkerSealServiceLimits::new(NonZeroUsize::MIN, NonZeroUsize::MIN).unwrap(),
    )
    .unwrap();
    let marker_authority = atoms
        .iter()
        .any(|atom| matches!(atom, Atom::Image(..)))
        .then(|| {
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
                assets.clone(),
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
    assets: &AssetState,
    mut binding: ComposerHostBinding,
    atoms: &[Atom<'_>],
) -> ComposerHostBinding {
    let mut text_bytes = 0_u64;
    let mut lines = 1_u64;
    let mut last_neighbor = None;
    for (atom_index, atom) in atoms.iter().enumerate() {
        let key = MutationKey::new(
            BindingId::new(binding.host_generation().get()),
            SourceRevision::new(binding.candidate().candidate_generation()),
            gpui_text_input::OperationId::new(u64::try_from(atom_index + 1).unwrap()),
        );
        let origin = SourcePosition::new(
            ByteOffset::new(text_bytes),
            last_neighbor.map_or(InlineObjectGap::NoObjects, InlineObjectGap::after),
        );
        let begin = MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                MutationPositions::collapsed(origin),
                SourceRange::new(origin, origin).unwrap(),
                0,
            ),
            MutationCursor::new(0),
            MutationCursor::new(0),
        );
        let mut page_items: Vec<(MutationPageItem, Box<[ComposerHostImageMarkerMetadata]>)> =
            Vec::new();
        let mut markers = Vec::new();
        let initial_text_bytes = text_bytes;
        match atom {
            Atom::Text(text) => {
                let mut remaining = *text;
                while !remaining.is_empty() {
                    let mut end = remaining.len().min(16_384);
                    while !remaining.is_char_boundary(end) {
                        end -= 1;
                    }
                    let (chunk, rest) = remaining.split_at(end);
                    page_items.push((
                        MutationPageItem::Utf8 {
                            inserted_offset: text_bytes - initial_text_bytes,
                            text: chunk.into(),
                        },
                        Box::new([]),
                    ));
                    text_bytes += chunk.len() as u64;
                    lines += chunk.bytes().filter(|byte| *byte == b'\n').count() as u64;
                    last_neighbor = None;
                    remaining = rest;
                }
            }
            Atom::Image(label, asset) => {
                let ordinal = u64::try_from(atom_index + 1).unwrap();
                let marker = marker_id(binding.candidate().draft_id(), ordinal);
                let object = InlineObjectId::new(u128::from_be_bytes(*marker.as_bytes()));
                let order = InlineObjectOrder::new(u128::from(ordinal));
                page_items.push((
                    MutationPageItem::Object(gpui_text_input::ObjectChange::Insert {
                        object: SuccessorObject::new(
                            object,
                            ByteOffset::new(text_bytes),
                            order,
                            17,
                            5,
                        ),
                    }),
                    Box::new([ComposerHostImageMarkerMetadata::new(object, *asset)]),
                ));
                markers.push(syndic_storage::DraftPieceMarkerV1::new(
                    marker, ordinal, *label, *asset,
                ));
                last_neighbor = Some(InlineObjectNeighbor::new(object, order));
            }
        }
        if markers.is_empty() {
            host.begin_mutation(store, binding, begin).unwrap();
        } else {
            let storage = SyndicStorage::reacquire(store).unwrap();
            let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = storage
                .draft_editor_candidate_session(
                    store,
                    binding.candidate().draft_id(),
                    binding.candidate().session_id(),
                )
                .unwrap()
            else {
                panic!("submission fixture candidate is inactive")
            };
            let mut operation = [0; 16];
            operation[8..].copy_from_slice(&begin.proposal().key().operation().get().to_be_bytes());
            let owner = syndic_storage::DraftMarkerAdmissionOwnerV1::new(
                session.draft_id(),
                session.session_id(),
                syndic_storage::DraftMarkerAdmissionOperationIdV1::from_bytes(operation),
            );
            let marker = markers[0];
            let previous = atoms[..atom_index].iter().enumerate().find_map(|(index, atom)| {
                matches!(atom, Atom::Image(label, asset) if *label == marker.label() && *asset == marker.asset_id())
                    .then(|| marker_id(session.draft_id(), u64::try_from(index + 1).unwrap()))
            });
            let readiness =
                marker_readiness::ready(&storage, store, assets, &session, owner, marker, previous);
            host.test_begin_marker_mutation(store, binding, begin, readiness)
                .unwrap();
        }
        let mut finish = MutationStreamFinish {
            next_cursor: MutationCursor::new(0),
            next_ordinal: 0,
            cumulative_identity: MutationIdentity::ROOT,
            totals: MutationTotals::default(),
        };
        for (item, metadata) in page_items {
            let next = finish.next_ordinal + 1;
            let page = MutationPage::new(
                MutationPageKey::new(
                    key,
                    MutationLane::Proposal,
                    finish.next_cursor,
                    finish.next_ordinal,
                    finish.cumulative_identity,
                ),
                MutationCursor::new(next),
                vec![item],
            )
            .unwrap();
            let totals = page.totals();
            finish = MutationStreamFinish {
                next_cursor: page.next_cursor(),
                next_ordinal: next,
                cumulative_identity: page.cumulative_identity(),
                totals: MutationTotals {
                    pages: finish.totals.pages + totals.pages,
                    items: finish.totals.items + totals.items,
                    retained_bytes: finish.totals.retained_bytes + totals.retained_bytes,
                    inserted_bytes: finish.totals.inserted_bytes + totals.inserted_bytes,
                    inserted_line_breaks: finish.totals.inserted_line_breaks
                        + totals.inserted_line_breaks,
                    objects: finish.totals.objects + totals.objects,
                    object_bytes: finish.totals.object_bytes + totals.object_bytes,
                    presentation_bytes: finish.totals.presentation_bytes
                        + totals.presentation_bytes,
                },
            };
            host.stage_mutation_page(store, MutationPageRequest::new(page), metadata)
                .unwrap();
        }
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
        let mut committed = None;
        for _ in 0..64 {
            match host.execute_mutation(
                store,
                MutationCommitRequest::new(key, MutationIdentity::ROOT),
                &CommandCancellation::new(),
            ) {
                Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => {
                    committed = Some(binding);
                    break;
                }
                Err(crate::composer_host::ComposerHostError::MutationWorkPending) => {}
                outcome => panic!("submission fixture mutation did not commit: {outcome:?}"),
            }
        }
        binding = committed.expect("submission fixture mutation did not converge");
    }
    binding
}

fn marker_id(draft: SyndicDraftId, ordinal: u64) -> SyndicDraftMarkerId {
    let mut bytes = *draft.as_bytes();
    bytes[8..].copy_from_slice(&ordinal.to_be_bytes());
    SyndicDraftMarkerId::from_bytes(bytes)
}

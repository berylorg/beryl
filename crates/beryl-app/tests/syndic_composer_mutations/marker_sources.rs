use beryl_app::composer_host::SyndicComposerHost;
use beryl_home_store::HomeStore;
use gpui_text_input::{
    MutationCursor, MutationIdentity, MutationKey, MutationLane, MutationPage, MutationPageItem,
    MutationPageKey, MutationPageRequest, MutationStreamFinish, MutationTotals, ObjectChange,
};

pub fn begin_marker_removal(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    binding: beryl_app::composer_host::ComposerHostBinding,
    begin: gpui_text_input::MutationBeginRequest,
) {
    use syndic_storage::{
        DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionOperationIdV1,
        DraftMarkerAdmissionOwnerV1, DraftMarkerLabelAssignmentOutcomeV1,
        DraftMarkerLabelReadinessDispositionV1, DraftMarkerLabelReadinessPageRequestV1,
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1, SyndicStorage,
    };
    let storage = SyndicStorage::reacquire(store).unwrap();
    let mut bytes = [0; 16];
    bytes[8..].copy_from_slice(&begin.proposal().key().operation().get().to_be_bytes());
    let owner = DraftMarkerAdmissionOwnerV1::new(
        binding.candidate().draft_id(),
        binding.candidate().session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes(bytes),
    );
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_page(
            store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes(bytes),
                std::num::NonZeroU64::MIN,
                true,
                DraftMarkerLabelReadinessDispositionV1::Allocate,
                Box::new([]),
                None,
            ),
        )
        .unwrap();
    let command = attempt.take_command().unwrap();
    let receipt = store.compose_proof(command).unwrap();
    let flight = attempt.into_submission_flight(store, receipt).unwrap();
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced {
            later_failure: None,
            ..
        }
    ));
    for step in 1_u64..=64 {
        bytes[..8].copy_from_slice(&step.to_be_bytes());
        let flight = storage
            .prepare_draft_marker_label_assignment(
                store,
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes(bytes),
            )
            .unwrap();
        match storage.submit_draft_marker_label_assignment(store, flight) {
            DraftMarkerLabelAssignmentOutcomeV1::Advanced {
                later_failure: None,
                ..
            } => {}
            DraftMarkerLabelAssignmentOutcomeV1::Ready {
                proof,
                later_failure: None,
                ..
            } => {
                host.test_begin_marker_mutation(store, binding, begin, proof)
                    .unwrap();
                return;
            }
            _ => panic!("marker removal readiness did not advance"),
        }
    }
    panic!("marker removal readiness exceeded its bounded drive budget")
}

pub fn stage_marker_sources(
    host: &mut SyndicComposerHost,
    store: &HomeStore,
    key: MutationKey,
    items: &[MutationPageItem],
) -> MutationStreamFinish {
    let source_items: Vec<_> = items
        .iter()
        .filter(|item| {
            matches!(
                item,
                MutationPageItem::Object(
                    ObjectChange::Remove { .. }
                        | ObjectChange::Replace { .. }
                        | ObjectChange::Move { .. }
                )
            )
        })
        .cloned()
        .collect();
    if source_items.is_empty() {
        return MutationStreamFinish {
            next_cursor: MutationCursor::new(0),
            next_ordinal: 0,
            cumulative_identity: MutationIdentity::ROOT,
            totals: MutationTotals::default(),
        };
    }
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Source,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        source_items,
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
}

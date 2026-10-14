use super::*;
use beryl_app::composer_host::{
    ComposerHostReadTarget, ComposerHostRequestId, ComposerHostRequestKey, ComposerHostRequestKind,
    ComposerHostRequestPurpose, ComposerHostResponse, ComposerHostResponseValue,
};
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1,
    DraftPieceMarkerScopeV1,
};

pub(super) fn read_markers(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    request_id: u64,
    anchor: u64,
    purpose: ComposerHostRequestPurpose,
) -> Result<ComposerHostResponse, ComposerHostError> {
    let key = ComposerHostRequestKey::new(
        binding,
        ComposerHostRequestId::new(NonZeroU64::new(request_id).unwrap()),
        purpose,
    );
    let pending = host.begin_request(
        key,
        ComposerHostRequestKind::Markers {
            target: ComposerHostReadTarget::Candidate,
            demand: DraftPieceMarkerDemandV1::new(
                DraftPieceMarkerScopeV1::ExactAnchor(anchor),
                DraftPieceMarkerDirectionV1::Forward,
                None,
                2,
                65_536,
            ),
        },
    )?;
    let execution = host.execute_pending(store, pending);
    host.complete_request(execution)
}

pub(super) fn assert_predecessor_markers(
    host: &mut SyndicComposerHost,
    storage: &SyndicStorage,
    store: &beryl_home_store::HomeStore,
    binding: ComposerHostBinding,
    request_id: u64,
    anchor: u64,
    expected: [InlineObjectNeighbor; 2],
) {
    let before = host.binding();
    let current = match storage
        .draft_editor_candidate_session(
            store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap()
    {
        DraftEditorCandidateSessionReadOutcomeV1::Active(current) => current,
        other => panic!("expected live owned staging session, got {other:?}"),
    };
    let response = read_markers(
        host,
        store,
        binding,
        request_id,
        anchor,
        ComposerHostRequestPurpose::MutationPredecessor,
    )
    .unwrap();
    let ComposerHostResponseValue::CandidateMarkers(result) = response.value() else {
        panic!("expected typed candidate marker result");
    };
    assert_eq!(
        result.binding(),
        DraftEditorCandidateActivationBindingV1::from_head(&current)
    );
    assert_eq!(
        result.binding().candidate_generation(),
        binding.candidate().candidate_generation()
    );
    assert_eq!(result.binding().root(), binding.root());
    assert_eq!(result.binding().history(), binding.history());
    assert_eq!(
        result.binding().logical_extent(),
        binding.candidate().logical_extent()
    );
    assert_eq!(result.value().markers().len(), 2);
    for (marker, expected) in result.value().markers().iter().zip(expected) {
        assert_eq!(marker.anchor(), anchor);
        assert_eq!(
            marker.marker().marker_id(),
            SyndicDraftMarkerId::from_bytes(expected.id().get().to_be_bytes())
        );
        assert_eq!(
            u128::from(marker.marker().order_key()),
            expected.order().get()
        );
    }
    assert_eq!(host.binding(), before);
    assert_eq!(before, Some(binding));
}

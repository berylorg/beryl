use beryl_home_store::HomeStore;
use beryl_model::SyndicDraftMarkerId;
use beryl_state::AssetState;
use syndic_storage::{
    DraftEditorCandidateSessionV1, DraftMarkerAdmissionCommandIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMarkerLabelAssignmentOutcomeV1, DraftMarkerLabelReadinessDispositionV1,
    DraftMarkerLabelReadinessPageRequestV1, DraftMarkerLabelReadinessPageSubmissionOutcomeV1,
    DraftMarkerLabelReadinessProofV1, DraftMarkerReadinessCandidateSourceV1,
    DraftMarkerReadinessSourceAssociationV1, DraftMarkerReadinessSourceSelectorV1,
    DraftMarkerReadinessWitnessFactoryV1, DraftPieceMarkerV1, SyndicStorage,
};

pub(super) fn ready(
    storage: &SyndicStorage,
    store: &HomeStore,
    assets: &AssetState,
    session: &DraftEditorCandidateSessionV1,
    owner: DraftMarkerAdmissionOwnerV1,
    marker: DraftPieceMarkerV1,
    previous: Option<SyndicDraftMarkerId>,
) -> DraftMarkerLabelReadinessProofV1 {
    let (source, witness) = match previous {
        Some(previous) => (
            DraftMarkerReadinessSourceSelectorV1::Candidate(
                DraftMarkerReadinessCandidateSourceV1::new(
                    session.draft_id(),
                    session.session_id(),
                    session.newest_candidate_generation(),
                    session.newest_root(),
                    previous,
                ),
            ),
            None,
        ),
        None => (
            DraftMarkerReadinessSourceSelectorV1::FreshAsset(marker.asset_id()),
            Some(DraftMarkerReadinessWitnessFactoryV1::fresh(
                assets.draft_marker_fresh_asset_readiness_witness_factory(),
            )),
        ),
    };
    let mut command = *owner.operation_id().as_bytes();
    let mut attempt = storage
        .prepare_draft_marker_label_readiness_page(
            store,
            DraftMarkerLabelReadinessPageRequestV1::new(
                owner,
                DraftMarkerAdmissionCommandIdV1::from_bytes(command),
                std::num::NonZeroU64::MIN,
                true,
                DraftMarkerLabelReadinessDispositionV1::Allocate,
                Box::new([DraftMarkerReadinessSourceAssociationV1::new(
                    marker.marker_id(),
                    source,
                )]),
                witness,
            ),
        )
        .unwrap();
    let receipt = store
        .compose_proof(attempt.take_command().unwrap())
        .unwrap();
    let flight = attempt.into_submission_flight(store, receipt).unwrap();
    assert!(matches!(
        storage.submit_draft_marker_label_readiness_page(store, flight),
        DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced {
            later_failure: None,
            ..
        }
    ));
    command[..8].copy_from_slice(&1_u64.to_be_bytes());
    let flight = storage
        .prepare_draft_marker_label_assignment(
            store,
            owner,
            DraftMarkerAdmissionCommandIdV1::from_bytes(command),
        )
        .unwrap();
    let DraftMarkerLabelAssignmentOutcomeV1::Ready {
        proof,
        later_failure: None,
        ..
    } = storage.submit_draft_marker_label_assignment(store, flight)
    else {
        panic!("submission fixture marker assignment did not become ready")
    };
    let assigned = storage
        .inspect_draft_marker_label_readiness_proof_for_test(store, &proof)
        .unwrap();
    assert_eq!(assigned.len(), 1);
    assert_eq!(assigned[0].target_marker_id(), marker.marker_id());
    assert_eq!(assigned[0].assigned_label(), marker.label());
    proof
}

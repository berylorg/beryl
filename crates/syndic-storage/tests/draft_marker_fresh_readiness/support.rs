use super::*;

pub(super) fn owner(
    session: &DraftEditorCandidateSessionV1,
    seed: u8,
) -> DraftMarkerAdmissionOwnerV1 {
    DraftMarkerAdmissionOwnerV1::new(
        session.draft_id(),
        session.session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes([seed; 16]),
    )
}

pub(super) fn fresh(target: u8, asset: AssetId) -> DraftMarkerReadinessSourceAssociationV1 {
    DraftMarkerReadinessSourceAssociationV1::new(
        SyndicDraftMarkerId::from_bytes([target; 16]),
        DraftMarkerReadinessSourceSelectorV1::FreshAsset(asset),
    )
}

pub(super) fn fresh_factory(fixture: &AcceptedFixture) -> DraftMarkerReadinessWitnessFactoryV1 {
    DraftMarkerReadinessWitnessFactoryV1::fresh(
        fixture
            .state
            .assets()
            .draft_marker_fresh_asset_readiness_witness_factory(),
    )
}

pub(super) fn request(
    operation: DraftMarkerAdmissionOwnerV1,
    command: u8,
    ordinal: u64,
    eof: bool,
    disposition: Disposition,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
    witness: Option<DraftMarkerReadinessWitnessFactoryV1>,
) -> DraftMarkerLabelReadinessPageRequestV1 {
    DraftMarkerLabelReadinessPageRequestV1::new(
        operation,
        DraftMarkerAdmissionCommandIdV1::from_bytes([command; 16]),
        NonZeroU64::new(ordinal).unwrap(),
        eof,
        disposition,
        associations.into_boxed_slice(),
        witness,
    )
}

pub(super) fn ingest(
    fixture: &AcceptedFixture,
    operation: DraftMarkerAdmissionOwnerV1,
    command: u8,
    ordinal: u64,
    eof: bool,
    associations: Vec<DraftMarkerReadinessSourceAssociationV1>,
    factory: impl Fn() -> Option<DraftMarkerReadinessWitnessFactoryV1>,
) {
    for _ in 0..associations.len().max(1) {
        let mut attempt = fixture
            .storage
            .prepare_draft_marker_label_readiness_page(
                &fixture.store,
                request(
                    operation,
                    command,
                    ordinal,
                    eof,
                    Disposition::Allocate,
                    associations.clone(),
                    factory(),
                ),
            )
            .unwrap();
        let receipt = fixture
            .store
            .compose_proof(attempt.take_command().unwrap())
            .unwrap();
        let flight = attempt
            .into_submission_flight(&fixture.store, receipt)
            .unwrap();
        assert!(matches!(
            fixture
                .storage
                .submit_draft_marker_label_readiness_page(&fixture.store, flight),
            DraftMarkerLabelReadinessPageSubmissionOutcomeV1::Advanced { .. }
        ));
    }
}

pub(super) fn assign(
    fixture: &AcceptedFixture,
    operation: DraftMarkerAdmissionOwnerV1,
    seed: u8,
    count: usize,
) -> DraftMarkerLabelReadinessProofV1 {
    for index in 0..count.max(1) {
        let flight = fixture
            .storage
            .prepare_draft_marker_label_assignment(
                &fixture.store,
                operation,
                DraftMarkerAdmissionCommandIdV1::from_bytes([seed + index as u8; 16]),
            )
            .unwrap();
        match fixture
            .storage
            .submit_draft_marker_label_assignment(&fixture.store, flight)
        {
            DraftMarkerLabelAssignmentOutcomeV1::Advanced { .. } if index + 1 < count => {}
            DraftMarkerLabelAssignmentOutcomeV1::Ready { proof, .. }
                if index + 1 == count.max(1) =>
            {
                return proof;
            }
            _ => panic!("assignment did not advance one bounded occurrence"),
        }
    }
    unreachable!()
}

pub(super) fn label(
    fixture: &AcceptedFixture,
    proof: &DraftMarkerLabelReadinessProofV1,
    target: u8,
) -> ImageLabelOrdinal {
    fixture
        .storage
        .inspect_draft_marker_label_readiness_proof_for_test(&fixture.store, proof)
        .unwrap()
        .into_iter()
        .find(|entry| entry.target_marker_id() == SyndicDraftMarkerId::from_bytes([target; 16]))
        .unwrap()
        .assigned_label()
}

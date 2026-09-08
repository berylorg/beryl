use super::*;

pub struct DraftMarkerSourceEndpointForTest {
    build: DraftPieceBuildRecordV1,
    session: DraftEditorCandidateSessionRecordV1,
}

pub fn capture_draft_marker_source_endpoint_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    build: &DraftPieceBuildRecordV1,
) -> DraftMarkerSourceEndpointForTest {
    let session_key =
        DraftEditorCandidateSessionRecordKeyV1::head(build.draft_id(), build.session_id());
    let session = storage
        .point::<DraftEditorCandidateSessionsFamily>(
            store,
            session_key,
            SyndicPointReadLimit::new(DraftEditorCandidateSessionsFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    DraftMarkerSourceEndpointForTest {
        build: build.clone(),
        session,
    }
}

pub fn restore_draft_marker_source_endpoint_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    endpoint: &DraftMarkerSourceEndpointForTest,
    occupied_target_receipt: DraftPieceBuildProgressReceiptReferenceV1,
) {
    assert_eq!(
        occupied_target_receipt.key().transition_ordinal(),
        endpoint.build.progress_receipt().key().transition_ordinal() + 1
    );
    write_endpoint(
        store,
        storage,
        endpoint.build.clone(),
        None,
        Some(occupied_target_receipt.key()),
        endpoint.session.clone(),
    );
}

pub fn inject_coordinated_draft_marker_secondary_for_test(
    store: &HomeStore,
    storage: &SyndicStorage,
    build: &DraftPieceBuildRecordV1,
) {
    let continuation = build.marker_effect_continuation();
    let active = continuation.active().unwrap();
    assert!(matches!(
        active.pending(),
        DraftPieceMarkerPendingV1::Proof {
            purpose: DraftPieceMarkerProofPurposeV1::SourceInsertIdentityAbsent,
            ..
        }
    ));
    let active = active.with_program(
        None,
        Some(DraftPieceMarkerPlanningV1 {
            source_boundary: None,
            previous_start: None,
        }),
        None,
        DraftPieceMarkerPendingV1::Proof {
            purpose: DraftPieceMarkerProofPurposeV1::SourceBounds,
            component: DraftPieceMarkerProofComponentV1::Secondary,
            primary_marker_rank: Some(0),
        },
    );
    let replacement =
        build
            .clone()
            .with_marker_effect_continuation(DraftPieceMarkerEffectContinuationV1::new(
                continuation.source_logical_frontier(),
                continuation.successor_logical_frontier(),
                continuation.scan(),
                Some(active),
            ));
    let bytes = DraftPieceBuildsFamily::encode_value(&replacement).unwrap();
    assert!(DraftPieceBuildsFamily::decode_value(&bytes).is_ok());
    let receipt = storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            build.progress_receipt().key(),
            SyndicPointReadLimit::new(DraftPieceBuildProgressFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    let prior = storage
        .point::<DraftPieceBuildProgressFamily>(
            store,
            receipt.previous().unwrap().key(),
            SyndicPointReadLimit::new(DraftPieceBuildProgressFamily::MAX_VALUE_BYTES).unwrap(),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        prior
            .marker_effect_continuation()
            .active()
            .unwrap()
            .pending(),
        DraftPieceMarkerPendingV1::Proof {
            purpose: DraftPieceMarkerProofPurposeV1::SourceBounds,
            component: DraftPieceMarkerProofComponentV1::Primary,
            ..
        }
    ));
    let key =
        DraftPieceSettlementKeyV1::new(build.draft_id(), build.session_id(), build.operation_id());
    install_coordinated_program(store, storage, key, replacement, receipt, prior);
}

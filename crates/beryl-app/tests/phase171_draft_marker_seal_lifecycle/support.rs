use super::*;

pub(super) fn marker_session(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    seed: u8,
) -> DraftEditorCandidateSessionV1 {
    marker_session_with_marker(storage, store, thread, seed, marker(seed, 1, 1))
}

pub(super) fn marker_session_with_marker(
    storage: SyndicStorage,
    store: &HomeStore,
    thread: SyndicThreadId,
    seed: u8,
    marker: DraftPieceMarkerV1,
) -> DraftEditorCandidateSessionV1 {
    let durable = current(storage, store, thread);
    let mut session = open_session(
        storage,
        store,
        &durable,
        seed.wrapping_add(1),
        seed.wrapping_add(2),
    );
    session = complete_staged(
        &storage,
        store,
        &session,
        seed.wrapping_add(3),
        DraftPieceReplacementV1::new(
            point(0),
            point(0),
            vec![DraftPieceV1::Text("abc".to_owned())],
        ),
        DraftLogicalExtentV1::new(3, 1),
    );
    complete_staged(
        &storage,
        store,
        &session,
        seed.wrapping_add(4),
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
        DraftLogicalExtentV1::new(3, 1),
    )
}

pub(super) fn advance_revision(store: &HomeStore, assets: AssetState, seed: u8) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(assets.begin_reference_set(
            assets.revision(store).unwrap(),
            BeginAssetReferenceSet::new(beryl_state::AssetReferenceSetStagingAuthority::new(
                AssetReferenceSetId::from_bytes([seed; 16]),
                [seed; 32],
            )),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));
}

pub(super) fn publish_asset(
    store: &HomeStore,
    state: &BerylState,
    bytes: &[u8],
) -> beryl_model::AssetId {
    let sidecar = store
        .admit_sidecar(
            beryl_home_store::SidecarNamespace::new("images").unwrap(),
            bytes,
            beryl_home_store::SidecarByteLimit::new(std::num::NonZeroU64::new(1024).unwrap()),
        )
        .unwrap();
    let asset = beryl_model::AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        std::num::NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    let expected = state.assets().revision(store).unwrap();
    let contribution = state
        .assets()
        .publish_metadata(
            expected,
            sidecar,
            beryl_state::PublishAssetMetadata::new(
                asset,
                beryl_state::AssetMediaType::new("image/png").unwrap(),
                None,
                expected.checked_next().unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    contribution.add_to(&mut command).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));
    asset
}

pub(super) fn new_service(
    store: &HomeStore,
    storage: SyndicStorage,
    assets: beryl_state::AssetState,
    flights: usize,
) -> DraftMarkerSealService {
    DraftMarkerSealService::new(
        store,
        store.health().generation().unwrap(),
        storage,
        assets,
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(flights).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

pub(super) fn request(
    session: &DraftEditorCandidateSessionV1,
    operation: u8,
    set: u8,
) -> DraftMarkerSealFlightRequest {
    DraftMarkerSealFlightRequest::new(
        DraftEditorCandidateActivationBindingV1::from_head(session),
        DraftMarkerSealOperationIdV1::from_bytes([operation; 16]),
        beryl_state::AssetReferenceSetStagingAuthority::new(
            AssetReferenceSetId::from_bytes([set; 16]),
            [set; 32],
        ),
    )
}

pub(super) fn admitted(
    service: &DraftMarkerSealService,
    store: &HomeStore,
    request: DraftMarkerSealFlightRequest,
) -> DraftMarkerSealFlight {
    match service
        .admit(store, request, &CommandCancellation::new())
        .unwrap()
    {
        DraftMarkerSealAdmission::Admitted(flight) => flight,
        other => panic!("request was not admitted: {other:?}"),
    }
}

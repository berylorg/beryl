use super::*;

pub(super) fn marker_session(
    storage: syndic_storage::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    thread: SyndicThreadId,
    seed: u8,
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
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let marker = marker(seed, 1, 1);
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
        DraftLogicalExtentV1::new(1, 1),
    )
}

pub(super) fn published_marker_session(
    storage: syndic_storage::SyndicStorage,
    store: &beryl_home_store::HomeStore,
    state: &BerylState,
    thread: SyndicThreadId,
    seed: u8,
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
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".to_owned())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([seed; 16]),
        1,
        ImageLabelOrdinal::new(1).unwrap(),
        publish_asset(store, state, b"phase171-published-marker"),
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
        DraftLogicalExtentV1::new(1, 1),
    )
}

pub(super) fn new_service(
    store: &beryl_home_store::HomeStore,
    storage: syndic_storage::SyndicStorage,
    assets: beryl_state::AssetState,
    flights: usize,
    page: usize,
) -> DraftMarkerSealService {
    DraftMarkerSealService::new(
        store,
        store.health().generation().unwrap(),
        storage,
        assets,
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(flights).unwrap(),
            NonZeroUsize::new(page).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}

pub(super) fn request(
    session: &syndic_storage::DraftEditorCandidateSessionV1,
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
    store: &beryl_home_store::HomeStore,
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

pub(super) fn admit_asset_sidecar(
    store: &beryl_home_store::HomeStore,
    bytes: &[u8],
) -> (AdmittedSidecar, AssetId) {
    let sidecar = store
        .admit_sidecar(
            SidecarNamespace::new("images").unwrap(),
            bytes,
            SidecarByteLimit::new(NonZeroU64::new(1_024).unwrap()),
        )
        .unwrap();
    let asset = AssetId::sha256_v1(
        sidecar.address().digest().as_bytes(),
        NonZeroU64::new(sidecar.address().length()).unwrap(),
    );
    (sidecar, asset)
}

pub(super) fn publish_asset(
    store: &beryl_home_store::HomeStore,
    state: &BerylState,
    bytes: &[u8],
) -> AssetId {
    let (sidecar, asset) = admit_asset_sidecar(store, bytes);
    publish_admitted_asset(store, state, sidecar, asset);
    asset
}

pub(super) fn publish_admitted_asset(
    store: &beryl_home_store::HomeStore,
    state: &BerylState,
    sidecar: AdmittedSidecar,
    asset: AssetId,
) {
    let expected = state.assets().revision(store).unwrap();
    let contribution = state
        .assets()
        .publish_metadata(
            expected,
            sidecar,
            PublishAssetMetadata::new(
                asset,
                AssetMediaType::new("image/png").unwrap(),
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
}

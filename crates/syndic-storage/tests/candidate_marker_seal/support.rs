include!("../durable_builder/support.rs");

#[path = "../support/composer_asset_fixture.rs"]
mod asset_fixture;

use beryl_home_store::{HomeCandidateRecoveryAccess, ReconciliationResolution};
use beryl_model::AssetReferenceSetId;
use beryl_state::{
    AppendAssetReferencePage, AssetReferencePageEntry, AssetReferenceSetCompletion,
    AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, BerylState, SealAssetReferenceSet,
};
use syndic_storage::{
    DRAFT_MARKER_SEAL_PAGE_MAX_MARKERS, DraftMarkerSealErrorV1, DraftMarkerSealFailureReasonV1,
    DraftMarkerSealOperationIdV1, DraftMarkerSealRequestV1, DraftMarkerSealStatusV1,
    inject_draft_marker_seal_natural_identity_collision_for_test,
    inject_draft_marker_seal_record_corruption_for_test,
};

fn marker_fixture(
    name: &str,
    faults: FaultController,
) -> (
    TestHome,
    HomeStore,
    SyndicStorage,
    BerylState,
    DraftEditorCandidateSessionV1,
) {
    let home = TestHome::new(name);
    let mut opening = beryl_home_store::HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(&home.0, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut opening).unwrap();
    let state = BerylState::register(&mut opening).unwrap();
    let store = opening
        .prepare_publication(
            SyndicStorage::required_domains()
                .unwrap()
                .merge(BerylState::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let thread = SyndicThreadId::from_bytes([160; 16]);
    committed(execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([161; 16]),
                ExecutionBinding::new(
                    RuntimeId::from_bytes([171; 16]),
                    RootId::from_bytes([172; 16]),
                    RuntimeNativePath::from_admitted(
                        RuntimeMode::host(),
                        PathFlavor::Windows,
                        "C:\\candidate-marker",
                    )
                    .unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(1),
                syndic_storage::DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let selected = current(&storage, &store, thread);
    let opened = open_session(&storage, &store, &selected, 162, 163);
    let mut session = complete_staged(
        &storage,
        &store,
        &opened,
        164,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("abc".into())]),
        DraftLogicalExtentV1::new(3, 1),
    );
    let asset = asset_fixture::publish_asset(&store, &state.assets());
    for (operation, marker_id, label) in [(165, 166, 9), (167, 168, 3)] {
        let marker = DraftPieceMarkerV1::new(
            SyndicDraftMarkerId::from_bytes([marker_id; 16]),
            label,
            ImageLabelOrdinal::new(label).unwrap(),
            asset,
        );
        let before_all = if session.newest_root().marker_commitment().marker_count() == 0 {
            point(1)
        } else {
            DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll)
        };
        session = complete_staged(
            &storage,
            &store,
            &session,
            operation,
            DraftPieceReplacementV1::new(
                before_all,
                before_all,
                vec![DraftPieceV1::Marker(marker)],
            )
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
            session.logical_extent(),
        );
    }
    (home, store, storage, state, session)
}

fn candidate_execute(
    access: &HomeCandidateRecoveryAccess<'_>,
    contribution: MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(access.home_revision().unwrap());
    command.add(contribution).unwrap();
    access.execute(command)
}

fn settle_candidate(access: &HomeCandidateRecoveryAccess<'_>, outcome: CommandOutcome) -> bool {
    match outcome {
        CommandOutcome::NotCommitted { .. } => false,
        CommandOutcome::Committed { .. } => true,
        CommandOutcome::Indeterminate { reconciliation, .. } => {
            let handle = reconciliation.install_and_handle();
            match access.reconcile(&handle).unwrap() {
                ReconciliationResolution::ExactOld => false,
                ReconciliationResolution::ExactNew { .. } => true,
                other => panic!("unexpected seal command reconciliation: {other:?}"),
            }
        }
    }
}

fn fail_home(store: &HomeStore, faults: &FaultController) {
    if store.health().state() == HomeHealthState::Healthy {
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
    }
}

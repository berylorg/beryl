include!("../../syndic-storage/tests/phase154_durable_builder/support.rs");

use std::num::{NonZeroU64, NonZeroUsize};

use beryl_app::composer_marker_seal::{
    DraftMarkerSealAdmission, DraftMarkerSealCommandStage, DraftMarkerSealDriveOutcome,
    DraftMarkerSealFlight, DraftMarkerSealFlightRequest, DraftMarkerSealReleaseIntent,
    DraftMarkerSealReleaseOutcome, DraftMarkerSealService, DraftMarkerSealServiceError,
    DraftMarkerSealServiceLimits,
};
use beryl_home_store::{AdmittedSidecar, CommandCancellation, SidecarByteLimit, SidecarNamespace};
use beryl_model::{AssetId, AssetReferenceSetId};
use beryl_state::{
    ASSET_REFERENCE_PAGE_MAX_ENTRIES, AppendAssetReferencePage, AssetMediaType, AssetOwner,
    AssetReferencePageEntry, AssetReferenceSetStagingAuthority, BeginAssetReferenceSet, BerylState,
    PublishAssetMetadata,
};
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftMarkerSealFailureReasonV1,
    DraftMarkerSealOperationIdV1, DraftMarkerSealRequestV1, DraftMarkerSealStatusV1,
};

#[path = "phase171_draft_marker_seal_service/support.rs"]
mod app_support;

use app_support::*;

#[test]
fn changed_nonempty_streams_multiple_atomic_pages_and_changed_empty_has_no_asset_set() {
    let (_home, mut store, storage, thread) = fixture("app-phase170-complete", 10);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 11, 12);
    session = complete_staged(
        &storage,
        &store,
        &session,
        13,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("abc".into())]),
        DraftLogicalExtentV1::new(3, 1),
    );
    let asset = publish_asset(&store, &state, b"phase170-shared-asset");
    let before_all = DraftCompositePositionV1::new(1, DraftCompositeGapWitnessV1::BeforeAll);
    for (seed, order, label, position) in [(14, 8, 20, point(1)), (15, 7, 21, before_all)] {
        let marker = DraftPieceMarkerV1::new(
            SyndicDraftMarkerId::from_bytes([seed; 16]),
            order,
            ImageLabelOrdinal::new(label).unwrap(),
            asset,
        );
        session = complete_staged(
            &storage,
            &store,
            &session,
            seed + 20,
            DraftPieceReplacementV1::new(position, position, vec![DraftPieceV1::Marker(marker)])
                .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                    DraftPieceMarkerInsertionV1::new(
                        1,
                        marker,
                        DraftPieceMarkerEffectChargesV1::for_marker(marker),
                    ),
                )),
            DraftLogicalExtentV1::new(3, 1),
        );
    }
    let service = new_service(&store, storage, state.assets(), 2, 1);
    let request = request(&session, 40, 41);
    let flight = admitted(&service, &store, request);
    let mut progress = 0;
    let completed = loop {
        match service.drive(&store, flight).unwrap() {
            DraftMarkerSealDriveOutcome::Progress => progress += 1,
            outcome @ DraftMarkerSealDriveOutcome::ChangedNonempty { .. } => break outcome,
            other => panic!("unexpected changed-nonempty outcome: {other:?}"),
        }
    };
    let DraftMarkerSealDriveOutcome::ChangedNonempty { syndic, assets } = completed else {
        unreachable!()
    };
    assert!(progress >= 3);
    assert_eq!(syndic.sequential().marker_count(), 2);
    assert_eq!(assets.sequential(), syndic.sequential());
    assert_eq!(assets.ordered_assets(), syndic.ordered_assets());
    assert_eq!(service.diagnostics().current_flights(), 0);

    let empty_session = open_session(storage, &store, &durable, 42, 43);
    let empty_set = AssetReferenceSetId::from_bytes([44; 16]);
    let empty_flight = admitted(
        &service,
        &store,
        DraftMarkerSealFlightRequest::new(
            DraftEditorCandidateActivationBindingV1::from_head(&empty_session),
            DraftMarkerSealOperationIdV1::from_bytes([45; 16]),
            AssetReferenceSetStagingAuthority::new(empty_set, [44; 32]),
        ),
    );
    let empty = loop {
        match service.drive(&store, empty_flight).unwrap() {
            DraftMarkerSealDriveOutcome::Progress => {}
            outcome @ DraftMarkerSealDriveOutcome::ChangedToEmpty { .. } => break outcome,
            other => panic!("unexpected changed-empty outcome: {other:?}"),
        }
    };
    let DraftMarkerSealDriveOutcome::ChangedToEmpty { syndic } = empty else {
        unreachable!()
    };
    assert_eq!(syndic.sequential().marker_count(), 0);
    let begin =
        BeginAssetReferenceSet::new(AssetReferenceSetStagingAuthority::new(empty_set, [44; 32]));
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            state
                .assets()
                .begin_reference_set(state.assets().revision(&store).unwrap(), begin),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));
}

#[test]
fn failed_atomic_page_keeps_both_frontiers_and_replays_after_asset_metadata_arrives() {
    let (_home, mut store, storage, thread) = fixture("app-phase170-replay", 50);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 51, 52);
    session = complete_staged(
        &storage,
        &store,
        &session,
        53,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".repeat(65))]),
        DraftLogicalExtentV1::new(65, 1),
    );
    let (sidecar, asset) = admit_asset_sidecar(&store, b"phase170-delayed-metadata");
    let marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([54; 16]),
        9,
        ImageLabelOrdinal::new(30).unwrap(),
        asset,
    );
    session = complete_staged(
        &storage,
        &store,
        &session,
        55,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
        DraftLogicalExtentV1::new(1, 1),
    );
    let service = new_service(&store, storage, state.assets(), 1, 1);
    let request = request(&session, 56, 57);
    let key = syndic_storage::DraftMarkerSealRequestV1::new(
        session.newest_root(),
        request.operation_id(),
    )
    .key();
    let flight = admitted(&service, &store, request);
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::NotCommitted(DraftMarkerSealCommandStage::Page)
    );
    assert!(matches!(
        storage.draft_marker_seal_status(&store, key).unwrap(),
        DraftMarkerSealStatusV1::Open {
            completed_marker_count: 0
        }
    ));
    publish_admitted_asset(&store, &state, sidecar, asset);
    let completed = loop {
        match service.drive(&store, flight).unwrap() {
            DraftMarkerSealDriveOutcome::Progress => {}
            outcome @ DraftMarkerSealDriveOutcome::ChangedNonempty { .. } => break outcome,
            other => panic!("unexpected replay outcome: {other:?}"),
        }
    };
    let DraftMarkerSealDriveOutcome::ChangedNonempty { syndic, assets } = completed else {
        unreachable!()
    };
    assert_eq!(syndic.sequential().marker_count(), 1);
    assert_eq!(assets.sequential(), syndic.sequential());
    assert_eq!(service.diagnostics().current_flights(), 0);
}

#[test]
fn indeterminate_atomic_advance_uses_exact_home_reconciliation() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase170-indeterminate", 60, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 61, 62);
    let service = new_service(&store, storage, state.assets(), 1, 1);
    let flight = admitted(&service, &store, request(&session, 63, 64));
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::ChangedToEmpty { .. }
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
}

#[test]
fn shared_cap_coalesces_exact_identity_and_rejects_conflict_without_queueing() {
    let (_home, mut store, storage, thread) = fixture("app-phase170-cap", 70);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 71, 72);
    let service = new_service(&store, storage, state.assets(), 1, 1);
    let other_host = service.clone();
    let first_request = request(&session, 73, 74);
    let first = admitted(&service, &store, first_request);
    assert_eq!(
        other_host
            .admit(&store, first_request, &CommandCancellation::new())
            .unwrap(),
        DraftMarkerSealAdmission::Coalesced(first)
    );
    let conflict = DraftMarkerSealFlightRequest::new(
        first_request.candidate(),
        first_request.operation_id(),
        AssetReferenceSetStagingAuthority::new(AssetReferenceSetId::from_bytes([75; 16]), [75; 32]),
    );
    assert_eq!(
        other_host
            .admit(&store, conflict, &CommandCancellation::new())
            .unwrap(),
        DraftMarkerSealAdmission::Conflict
    );
    let second_request = request(&session, 76, 77);
    assert_eq!(
        other_host
            .admit(&store, second_request, &CommandCancellation::new())
            .unwrap(),
        DraftMarkerSealAdmission::Saturated
    );
    let diagnostics = service.diagnostics();
    assert_eq!(diagnostics.configured_flight_limit(), 1);
    assert_eq!(diagnostics.current_flights(), 1);
    assert_eq!(diagnostics.high_water_flights(), 1);
    assert_eq!(diagnostics.admission_denials(), 2);
    assert_eq!(diagnostics.coalesced_admissions(), 1);
    assert_eq!(diagnostics.conflicts(), 1);
    assert_eq!(diagnostics.retained_draft_sized_bytes(), 0);
    assert_eq!(
        service
            .release(&store, first, DraftMarkerSealReleaseIntent::Cancelled)
            .unwrap(),
        DraftMarkerSealReleaseOutcome::ReleasedWithoutDurableSeal(
            DraftMarkerSealReleaseIntent::Cancelled
        )
    );
    assert!(matches!(
        other_host
            .admit(&store, second_request, &CommandCancellation::new())
            .unwrap(),
        DraftMarkerSealAdmission::Admitted(_)
    ));
}

#[test]
fn prebegin_release_retire_and_dispose_release_owned_capacity() {
    let (_home, mut store, storage, thread) = fixture("app-phase170-release", 90);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 91, 92);
    let service = new_service(&store, storage, state.assets(), 2, 1);
    let cancelled = CommandCancellation::new();
    cancelled.cancel();
    assert_eq!(
        service
            .admit(&store, request(&session, 93, 94), &cancelled)
            .unwrap(),
        DraftMarkerSealAdmission::CancelledBeforeAdmission
    );
    for (operation, set, intent) in [
        (95, 96, DraftMarkerSealReleaseIntent::Cancelled),
        (
            97,
            98,
            DraftMarkerSealReleaseIntent::Failed(
                syndic_storage::DraftMarkerSealFailureReasonV1::Operational,
            ),
        ),
        (101, 102, DraftMarkerSealReleaseIntent::SessionDisposed),
    ] {
        let flight = admitted(&service, &store, request(&session, operation, set));
        assert_eq!(
            service.release(&store, flight, intent).unwrap(),
            DraftMarkerSealReleaseOutcome::ReleasedWithoutDurableSeal(intent)
        );
        assert_eq!(service.diagnostics().current_flights(), 0);
    }
    let _retired = admitted(&service, &store, request(&session, 103, 104));
    let retirement = service.retire_home_generation();
    assert_eq!(retirement.released(), 1);
    assert_eq!(retirement.settling_drives(), 0);
    assert_eq!(service.diagnostics().current_flights(), 0);
    drop(service);

    let disposable = new_service(&store, storage, state.assets(), 2, 1);
    admitted(&disposable, &store, request(&session, 105, 106));
    admitted(&disposable, &store, request(&session, 107, 108));
    assert!(matches!(
        disposable.dispose(&store).unwrap(),
        beryl_app::composer_marker_seal::DraftMarkerSealDisposeOutcome::Progress {
            remaining: 1,
            ..
        }
    ));
    assert_eq!(
        disposable.dispose(&store).unwrap(),
        beryl_app::composer_marker_seal::DraftMarkerSealDisposeOutcome::Disposed
    );
    assert_eq!(disposable.diagnostics().current_flights(), 0);
    assert_eq!(disposable.diagnostics().high_water_flights(), 2);
}

#[test]
fn drive_error_requires_durable_failed_settlement_without_asset_visibility() {
    let (_home, mut store, storage, thread) = fixture("app-phase170-drive-failed", 90);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 91, 92);
    session = complete_staged(
        &storage,
        &store,
        &session,
        93,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".into())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let asset = publish_asset(&store, &state, b"phase170-drive-failed-asset");
    let marker = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([94; 16]),
        1,
        ImageLabelOrdinal::new(1).unwrap(),
        asset,
    );
    session = complete_staged(
        &storage,
        &store,
        &session,
        94,
        DraftPieceReplacementV1::new(point(1), point(1), vec![DraftPieceV1::Marker(marker)])
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    1,
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
        DraftLogicalExtentV1::new(1, 1),
    );
    let service = new_service(&store, storage, state.assets(), 1, 1);
    let request = request(&session, 95, 96);
    let key =
        DraftMarkerSealRequestV1::new(request.candidate().root(), request.operation_id()).key();
    let flight = admitted(&service, &store, request);
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );

    let staging = request.staging_authority();
    let manifest = state
        .assets()
        .staged_reference_set_manifest(&store, staging)
        .unwrap();
    let append = AppendAssetReferencePage::new(
        manifest.build_proof(),
        vec![AssetReferencePageEntry::new(
            marker.marker_id(),
            marker.label(),
            marker.asset_id(),
        )]
        .into_boxed_slice(),
    )
    .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            state
                .assets()
                .append_reference_page(state.assets().revision(&store).unwrap(), append),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));

    assert!(matches!(
        service.drive(&store, flight),
        Err(DraftMarkerSealServiceError::FrontierMismatch)
    ));
    assert_eq!(service.diagnostics().terminalizing_flights(), 1);
    let intent = DraftMarkerSealReleaseIntent::Failed(DraftMarkerSealFailureReasonV1::Operational);
    assert!(matches!(
        service.release(&store, flight, intent).unwrap(),
        DraftMarkerSealReleaseOutcome::Settled { intent: actual, .. } if actual == intent
    ));
    assert!(matches!(
        storage.draft_marker_seal_status(&store, key).unwrap(),
        DraftMarkerSealStatusV1::Failed {
            reason: DraftMarkerSealFailureReasonV1::Operational,
            ..
        }
    ));
    assert!(
        state
            .assets()
            .owner_head(&store, AssetOwner::CurrentDraft(session.draft_id()))
            .unwrap()
            .is_none()
    );
    assert_eq!(service.diagnostics().current_flights(), 0);
}

#[test]
fn staging_authority_never_remints_from_a_set_id_and_debug_redacts_its_secret() {
    let (_home, mut store, storage, thread) = fixture("app-phase171-authority", 200);
    let state = BerylState::register(&mut store).unwrap();
    let session = marker_session(storage, &store, thread, 201);
    let service = new_service(&store, storage, state.assets(), 1, 1);
    let request = request(&session, 202, 203);
    let flight = admitted(&service, &store, request);
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    drop(service);

    let reminted = DraftMarkerSealFlightRequest::new(
        request.candidate(),
        request.operation_id(),
        AssetReferenceSetStagingAuthority::new(
            AssetReferenceSetId::from_bytes([203; 16]),
            [204; 32],
        ),
    );
    assert!(format!("{reminted:?}").contains("[redacted]"));
    let fresh = new_service(&store, storage, state.assets(), 1, 1);
    let reminted_flight = admitted(&fresh, &store, reminted);
    assert!(matches!(
        fresh.drive(&store, reminted_flight),
        Err(DraftMarkerSealServiceError::AssetRead(
            beryl_state::AssetReadError::CompletionMismatch(_)
        ))
    ));
}

#[test]
fn fresh_service_resumes_building_and_replays_a_sealed_asset_proof_without_marker_retraversal() {
    let (_home, mut store, storage, thread) = fixture("app-phase171-fresh", 210);
    let state = BerylState::register(&mut store).unwrap();
    let session = published_marker_session(storage, &store, &state, thread, 211);
    let request = request(&session, 212, 213);
    let initial = new_service(&store, storage, state.assets(), 1, 1);
    let flight = admitted(&initial, &store, request);
    assert_eq!(
        initial.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );

    let resumed = new_service(&store, storage, state.assets(), 1, 1);
    let resumed_flight = match resumed
        .admit(&store, request, &CommandCancellation::new())
        .unwrap()
    {
        DraftMarkerSealAdmission::Coalesced(flight) => flight,
        other => panic!("fresh service did not coalesce the retained flight: {other:?}"),
    };
    assert_eq!(
        resumed.drive(&store, resumed_flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    assert!(matches!(
        resumed.drive(&store, resumed_flight).unwrap(),
        DraftMarkerSealDriveOutcome::ChangedNonempty { .. }
    ));

    let replay = new_service(&store, storage, state.assets(), 1, 1);
    let replay_flight = admitted(&replay, &store, request);
    assert!(matches!(
        replay.drive(&store, replay_flight).unwrap(),
        DraftMarkerSealDriveOutcome::ChangedNonempty { .. }
    ));
}

#[test]
fn independent_services_share_same_home_capacity_and_release_registry_ownership() {
    let (_home, mut store, storage, thread) = fixture("app-phase171-shared-home", 215);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 216, 217);
    let first = new_service(&store, storage, state.assets(), 1, 1);
    let second = new_service(&store, storage, state.assets(), 1, 1);
    let first_request = request(&session, 218, 219);
    let flight = admitted(&first, &store, first_request);
    assert_eq!(
        second
            .admit(&store, first_request, &CommandCancellation::new())
            .unwrap(),
        DraftMarkerSealAdmission::Coalesced(flight)
    );
    assert_eq!(
        second
            .admit(
                &store,
                request(&session, 220, 221),
                &CommandCancellation::new()
            )
            .unwrap(),
        DraftMarkerSealAdmission::Saturated
    );

    let (_other_home, mut other_store, other_storage, other_thread) =
        fixture("app-phase171-independent-home", 222);
    let other_state = BerylState::register(&mut other_store).unwrap();
    let other_durable = current(other_storage, &other_store, other_thread);
    let other_session = open_session(other_storage, &other_store, &other_durable, 223, 224);
    let other_service = new_service(&other_store, other_storage, other_state.assets(), 1, 1);
    assert!(matches!(
        other_service.admit(
            &other_store,
            request(&other_session, 225, 226),
            &CommandCancellation::new()
        ),
        Ok(DraftMarkerSealAdmission::Admitted(_))
    ));

    assert_eq!(
        second
            .release(&store, flight, DraftMarkerSealReleaseIntent::Cancelled)
            .unwrap(),
        DraftMarkerSealReleaseOutcome::ReleasedWithoutDurableSeal(
            DraftMarkerSealReleaseIntent::Cancelled
        )
    );
    assert_eq!(
        second.dispose(&store).unwrap(),
        beryl_app::composer_marker_seal::DraftMarkerSealDisposeOutcome::Disposed
    );

    let reopened = new_service(&store, storage, state.assets(), 1, 1);
    assert!(matches!(
        reopened.admit(&store, first_request, &CommandCancellation::new()),
        Ok(DraftMarkerSealAdmission::Admitted(_))
    ));
    drop(first);
    drop(second);
}

#[test]
fn shared_home_construction_requires_exact_limits_and_domain_authority() {
    let (_home, mut store, storage, thread) = fixture("app-phase171-construction", 227);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 228, 229);
    let service = new_service(&store, storage, state.assets(), 2, 1);
    admitted(&service, &store, request(&session, 230, 231));
    admitted(&service, &store, request(&session, 232, 233));

    for limits in [
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap(),
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(2).unwrap(),
            NonZeroUsize::new(2).unwrap(),
        )
        .unwrap(),
    ] {
        assert!(matches!(
            DraftMarkerSealService::new(
                &store,
                store.health().generation().unwrap(),
                storage,
                state.assets(),
                limits,
            ),
            Err(
                beryl_app::composer_marker_seal::DraftMarkerSealServiceConstructionError::LimitsMismatch
            )
        ));
    }

    let (_other_home, mut other_store, other_storage, _) =
        fixture("app-phase171-construction-other", 234);
    let _other_state = BerylState::register(&mut other_store).unwrap();
    assert!(matches!(
        DraftMarkerSealService::new(
            &store,
            store.health().generation().unwrap(),
            other_storage,
            state.assets(),
            DraftMarkerSealServiceLimits::new(
                NonZeroUsize::new(2).unwrap(),
                NonZeroUsize::new(1).unwrap(),
            )
            .unwrap(),
        ),
        Err(
            beryl_app::composer_marker_seal::DraftMarkerSealServiceConstructionError::DomainAuthorityMismatch
        )
    ));
    let diagnostics = service.diagnostics();
    assert_eq!(diagnostics.configured_flight_limit(), 2);
    assert_eq!(diagnostics.current_flights(), 2);
}

#[test]
fn requested_65_marker_page_clamps_to_64_asset_entries_and_streams_the_remainder() {
    let limits = DraftMarkerSealServiceLimits::new(
        NonZeroUsize::new(1).unwrap(),
        NonZeroUsize::new(65).unwrap(),
    )
    .unwrap();
    assert_eq!(
        limits.markers_per_page().get(),
        ASSET_REFERENCE_PAGE_MAX_ENTRIES
    );
    let maximum = DraftMarkerSealServiceLimits::new(
        NonZeroUsize::new(1).unwrap(),
        NonZeroUsize::new(256).unwrap(),
    )
    .unwrap();
    assert_eq!(
        maximum.markers_per_page().get(),
        ASSET_REFERENCE_PAGE_MAX_ENTRIES
    );

    let (_home, mut store, storage, thread) = fixture("app-phase171-page-clamp", 220);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 221, 222);
    session = complete_staged(
        &storage,
        &store,
        &session,
        223,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".repeat(66))]),
        DraftLogicalExtentV1::new(66, 1),
    );
    let asset = publish_asset(&store, &state, b"phase171-page-clamp");
    for index in 1..=65_u8 {
        let marker = DraftPieceMarkerV1::new(
            SyndicDraftMarkerId::from_bytes([index; 16]),
            u64::from(index),
            ImageLabelOrdinal::new(u64::from(index)).unwrap(),
            asset,
        );
        session = complete_staged(
            &storage,
            &store,
            &session,
            index.wrapping_add(223),
            DraftPieceReplacementV1::new(
                point(u64::from(index)),
                point(u64::from(index)),
                vec![DraftPieceV1::Marker(marker)],
            )
            .with_marker_effect(DraftPieceMarkerEffectV1::Insert(
                DraftPieceMarkerInsertionV1::new(
                    u64::from(index),
                    marker,
                    DraftPieceMarkerEffectChargesV1::for_marker(marker),
                ),
            )),
            DraftLogicalExtentV1::new(66, 1),
        );
    }
    let service = new_service(&store, storage, state.assets(), 1, 65);
    let request = request(&session, 224, 225);
    let flight = admitted(&service, &store, request);
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    let key =
        DraftMarkerSealRequestV1::new(request.candidate().root(), request.operation_id()).key();
    assert!(matches!(
        storage.draft_marker_seal_status(&store, key).unwrap(),
        DraftMarkerSealStatusV1::Open {
            completed_marker_count: 64
        }
    ));
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    assert!(matches!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::ChangedNonempty { .. }
    ));
}

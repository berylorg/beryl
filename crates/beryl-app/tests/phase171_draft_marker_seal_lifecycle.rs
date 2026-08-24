include!("../../syndic-storage/tests/phase154_durable_builder/support.rs");

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use beryl_app::composer_marker_seal::{
    DraftMarkerSealAdmission, DraftMarkerSealCommandStage, DraftMarkerSealDisposeOutcome,
    DraftMarkerSealDriveOutcome, DraftMarkerSealFlight, DraftMarkerSealFlightRequest,
    DraftMarkerSealReleaseIntent, DraftMarkerSealReleaseOutcome, DraftMarkerSealService,
    DraftMarkerSealServiceError, DraftMarkerSealServiceLimits,
};
use beryl_home_store::CommandCancellation;
use beryl_model::AssetReferenceSetId;
use beryl_state::{
    AssetOwner, AssetReferenceSetStagingAuthority, AssetState, BeginAssetReferenceSet, BerylState,
    SealAssetReferenceSet,
};
use syndic_storage::{
    DraftEditorCandidateActivationBindingV1, DraftMarkerSealFailureReasonV1,
    DraftMarkerSealOperationIdV1, DraftMarkerSealRequestV1, DraftMarkerSealStatusV1,
};

#[path = "phase171_draft_marker_seal_lifecycle/support.rs"]
mod app_support;

use app_support::*;

#[test]
fn admission_authenticates_the_exact_active_head_before_charging_capacity() {
    let (_home, mut store, storage, thread) = fixture("app-phase170-auth", 110);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let mut session = open_session(storage, &store, &durable, 111, 112);
    let service = new_service(&store, storage, state.assets(), 1);
    let active = DraftEditorCandidateActivationBindingV1::from_head(&session);
    let stale = DraftEditorCandidateActivationBindingV1::new(
        active.draft_id(),
        DraftEditorCandidateSessionIdV1::from_bytes([113; 16]),
        active.session_generation(),
        active.candidate_generation(),
        active.root(),
        active.history(),
        active.logical_extent(),
    );
    let stale_request = DraftMarkerSealFlightRequest::new(
        stale,
        DraftMarkerSealOperationIdV1::from_bytes([114; 16]),
        AssetReferenceSetStagingAuthority::new(
            AssetReferenceSetId::from_bytes([115; 16]),
            [115; 32],
        ),
    );
    assert!(matches!(
        service.admit(&store, stale_request, &CommandCancellation::new()),
        Err(DraftMarkerSealServiceError::CandidateSessionAbsent)
    ));
    session = complete_staged(
        &storage,
        &store,
        &session,
        116,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".into())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    let stale_active_request = DraftMarkerSealFlightRequest::new(
        active,
        DraftMarkerSealOperationIdV1::from_bytes([117; 16]),
        AssetReferenceSetStagingAuthority::new(
            AssetReferenceSetId::from_bytes([118; 16]),
            [118; 32],
        ),
    );
    assert!(matches!(
        service.admit(&store, stale_active_request, &CommandCancellation::new()),
        Err(DraftMarkerSealServiceError::StaleCandidateBinding)
    ));
    let diagnostics = service.diagnostics();
    assert_eq!(diagnostics.current_flights(), 0);
    assert_eq!(diagnostics.high_water_flights(), 0);
    assert_eq!(diagnostics.admission_denials(), 0);
    admitted(&service, &store, request(&session, 119, 120));
}

#[test]
fn cancelled_failed_superseded_and_session_disposed_settle_durably() {
    let (_home, mut store, storage, thread) = fixture("app-phase170-terminal", 120);
    let state = BerylState::register(&mut store).unwrap();
    let session = marker_session(storage, &store, thread, 121);
    let service = new_service(&store, storage, state.assets(), 4);
    let active = DraftEditorCandidateActivationBindingV1::from_head(&session);
    let intents = [
        DraftMarkerSealReleaseIntent::Cancelled,
        DraftMarkerSealReleaseIntent::Failed(DraftMarkerSealFailureReasonV1::Operational),
        DraftMarkerSealReleaseIntent::Superseded {
            successor_operation_id: DraftMarkerSealOperationIdV1::from_bytes([130; 16]),
            successor: active,
        },
        DraftMarkerSealReleaseIntent::SessionDisposed,
    ];
    for (offset, intent) in intents.into_iter().enumerate() {
        let operation = 122 + offset as u8;
        let set = 126 + offset as u8;
        let request = request(&session, operation, set);
        let key =
            DraftMarkerSealRequestV1::new(request.candidate().root(), request.operation_id()).key();
        let flight = admitted(&service, &store, request);
        assert_eq!(
            service.drive(&store, flight).unwrap(),
            DraftMarkerSealDriveOutcome::Progress
        );
        assert!(matches!(
            service.release(&store, flight, intent).unwrap(),
            DraftMarkerSealReleaseOutcome::Settled { intent: actual, .. } if actual == intent
        ));
        match (
            intent,
            storage.draft_marker_seal_status(&store, key).unwrap(),
        ) {
            (
                DraftMarkerSealReleaseIntent::Cancelled
                | DraftMarkerSealReleaseIntent::SessionDisposed,
                DraftMarkerSealStatusV1::Cancelled(_),
            ) => {}
            (
                DraftMarkerSealReleaseIntent::Failed(expected),
                DraftMarkerSealStatusV1::Failed { reason, .. },
            ) if expected == reason => {}
            (
                DraftMarkerSealReleaseIntent::Superseded {
                    successor_operation_id,
                    ..
                },
                DraftMarkerSealStatusV1::Superseded { successor, .. },
            ) if successor_operation_id == successor => {}
            other => panic!("unexpected durable terminal status: {other:?}"),
        }
        let staging = request.staging_authority();
        assert!(
            state
                .assets()
                .staged_reference_set_manifest(&store, staging)
                .is_ok()
        );
        assert!(
            state
                .assets()
                .owner_head(&store, AssetOwner::CurrentDraft(session.draft_id()))
                .unwrap()
                .is_none()
        );
    }
    let diagnostics = service.diagnostics();
    assert_eq!(diagnostics.current_flights(), 0);
    assert_eq!(diagnostics.high_water_flights(), 1);
    assert_eq!(diagnostics.terminalizing_flights(), 0);
}

#[test]
fn terminal_not_committed_keeps_the_slot_and_replay_settles_exactly() {
    let (_home, mut store, storage, thread) = fixture("app-phase170-terminal-replay", 140);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 141, 142);
    let service = new_service(&store, storage, state.assets(), 1);
    let request = request(&session, 143, 144);
    let key =
        DraftMarkerSealRequestV1::new(request.candidate().root(), request.operation_id()).key();
    let flight = admitted(&service, &store, request);
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    let assets = state.assets();
    service.test_arm_before_command_fault(move |store| advance_revision(store, assets, 145));
    assert_eq!(
        service
            .release(&store, flight, DraftMarkerSealReleaseIntent::Cancelled)
            .unwrap(),
        DraftMarkerSealReleaseOutcome::NotCommitted(DraftMarkerSealReleaseIntent::Cancelled)
    );
    assert!(matches!(
        storage.draft_marker_seal_status(&store, key).unwrap(),
        DraftMarkerSealStatusV1::Open { .. }
    ));
    let diagnostics = service.diagnostics();
    assert_eq!(diagnostics.current_flights(), 1);
    assert_eq!(diagnostics.terminalizing_flights(), 1);
    assert!(matches!(
        service
            .release(&store, flight, DraftMarkerSealReleaseIntent::Cancelled)
            .unwrap(),
        DraftMarkerSealReleaseOutcome::Settled { .. }
    ));
    assert!(matches!(
        storage.draft_marker_seal_status(&store, key).unwrap(),
        DraftMarkerSealStatusV1::Cancelled(_)
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
}

#[test]
fn supersession_replays_after_successor_drift_for_deferred_and_not_committed_settlement() {
    deferred_supersession_replay_after_successor_drift();
    not_committed_supersession_replay_after_successor_drift();
}

fn deferred_supersession_replay_after_successor_drift() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase171-supersession-deferred", 145, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 146, 147);
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = admitted(&service, &store, request(&session, 148, 149));
    let successor = DraftEditorCandidateActivationBindingV1::from_head(&session);
    let intent = DraftMarkerSealReleaseIntent::Superseded {
        successor_operation_id: DraftMarkerSealOperationIdV1::from_bytes([150; 16]),
        successor,
    };
    let block = faults.block_next(FaultPoint::BeforeCommit);
    let store = Arc::new(store);
    let worker_service = service.clone();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || worker_service.drive(&worker_store, flight));
    assert!(block.wait_until_reached(Duration::from_secs(5)));
    assert_eq!(
        service.release(&store, flight, intent).unwrap(),
        DraftMarkerSealReleaseOutcome::DeferredByActiveDrive(intent)
    );
    block.release();
    assert_eq!(
        worker.join().unwrap().unwrap(),
        DraftMarkerSealDriveOutcome::TerminalSettlementPending(intent)
    );
    let drifted = complete_staged(
        &storage,
        &store,
        &session,
        151,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("a".into())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    assert_ne!(
        DraftEditorCandidateActivationBindingV1::from_head(&drifted),
        successor
    );
    assert!(matches!(
        service.release(&store, flight, intent).unwrap(),
        DraftMarkerSealReleaseOutcome::Settled { intent: actual, .. } if actual == intent
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
    assert_eq!(
        service.dispose(&store).unwrap(),
        DraftMarkerSealDisposeOutcome::Disposed
    );
}

fn not_committed_supersession_replay_after_successor_drift() {
    let (_home, mut store, storage, thread) = fixture("app-phase171-supersession-replay", 152);
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 153, 154);
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = admitted(&service, &store, request(&session, 155, 156));
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    let successor = DraftEditorCandidateActivationBindingV1::from_head(&session);
    let intent = DraftMarkerSealReleaseIntent::Superseded {
        successor_operation_id: DraftMarkerSealOperationIdV1::from_bytes([157; 16]),
        successor,
    };
    let assets = state.assets();
    service.test_arm_before_command_fault(move |store| advance_revision(store, assets, 158));
    assert_eq!(
        service.release(&store, flight, intent).unwrap(),
        DraftMarkerSealReleaseOutcome::NotCommitted(intent)
    );
    let drifted = complete_staged(
        &storage,
        &store,
        &session,
        159,
        DraftPieceReplacementV1::new(point(0), point(0), vec![DraftPieceV1::Text("b".into())]),
        DraftLogicalExtentV1::new(1, 1),
    );
    assert_ne!(
        DraftEditorCandidateActivationBindingV1::from_head(&drifted),
        successor
    );
    assert!(matches!(
        service.release(&store, flight, intent).unwrap(),
        DraftMarkerSealReleaseOutcome::Settled { intent: actual, .. } if actual == intent
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
    assert_eq!(
        service.dispose(&store).unwrap(),
        DraftMarkerSealDisposeOutcome::Disposed
    );
}

#[test]
fn construction_rejects_a_new_generation_while_old_home_authority_is_live() {
    let faults = FaultController::new();
    let (_home, mut store, storage, _) =
        fixture_with_faults("app-phase171-construction-generation", 146, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let service = new_service(&store, storage, state.assets(), 1);
    let old_generation = store.health().generation().unwrap();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    let store = store.recover_same_home().unwrap().publish();
    let storage = syndic_storage::SyndicStorage::reacquire(&store).unwrap();
    let state = BerylState::reacquire(&store).unwrap();
    let current_generation = store.health().generation().unwrap();
    assert_ne!(current_generation, old_generation);
    assert!(matches!(
        DraftMarkerSealService::new(
            &store,
            current_generation,
            storage,
            state.assets(),
            DraftMarkerSealServiceLimits::new(
                NonZeroUsize::new(1).unwrap(),
                NonZeroUsize::new(1).unwrap(),
            )
            .unwrap(),
        ),
        Err(
            beryl_app::composer_marker_seal::DraftMarkerSealServiceConstructionError::HomeGenerationMismatch
        )
    ));
    drop(service);
}

#[test]
fn holding_one_home_state_does_not_block_other_home_construction_or_drop() {
    let (_home_a, mut store_a, storage_a, _) = fixture("app-phase171-registry-a", 147);
    let state_a = BerylState::register(&mut store_a).unwrap();
    let service_a = new_service(&store_a, storage_a, state_a.assets(), 1);
    let (_home_b, mut store_b, storage_b, _) = fixture("app-phase171-registry-b", 148);
    let state_b = BerylState::register(&mut store_b).unwrap();
    let (reached_send, reached_receive) = std::sync::mpsc::channel();
    let (release_send, release_receive) = std::sync::mpsc::channel();
    let holder = service_a.clone();
    let worker =
        std::thread::spawn(move || holder.test_hold_state_lock(reached_send, release_receive));
    reached_receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap();

    let service_b = DraftMarkerSealService::new(
        &store_b,
        store_b.health().generation().unwrap(),
        storage_b,
        state_b.assets(),
        DraftMarkerSealServiceLimits::new(
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    drop(service_b);
    release_send.send(()).unwrap();
    worker.join().unwrap();
}

#[test]
fn begin_page_and_asset_seal_fault_cuts_replay_without_frontier_drift() {
    let (_home, mut store, storage, thread) = fixture("app-phase170-all-cuts", 150);
    let state = BerylState::register(&mut store).unwrap();
    let asset = publish_asset(&store, &state, &[151; 32]);
    let inserted = DraftPieceMarkerV1::new(
        SyndicDraftMarkerId::from_bytes([151; 16]),
        1,
        ImageLabelOrdinal::new(1).unwrap(),
        asset,
    );
    let session = marker_session_with_marker(storage, &store, thread, 151, inserted);
    let service = new_service(&store, storage, state.assets(), 1);
    let request = request(&session, 152, 153);
    let flight = admitted(&service, &store, request);

    let assets = state.assets();
    service.test_arm_before_command_fault(move |store| advance_revision(store, assets, 200));
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::NotCommitted(DraftMarkerSealCommandStage::Begin)
    );
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    let assets = state.assets();
    service.test_arm_before_command_fault(move |store| advance_revision(store, assets, 201));
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::NotCommitted(DraftMarkerSealCommandStage::Page)
    );
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    let assets = state.assets();
    service.test_arm_before_command_fault(move |store| advance_revision(store, assets, 202));
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::NotCommitted(DraftMarkerSealCommandStage::AssetSeal)
    );
    assert!(matches!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::ChangedNonempty { .. }
    ));
}

#[test]
fn stale_final_asset_seal_reopens_to_a_competing_exact_asset_proof() {
    let (_home, mut store, storage, thread) = fixture("app-phase171-competing-asset-seal", 154);
    let state = BerylState::register(&mut store).unwrap();
    let asset = publish_asset(&store, &state, b"phase171-competing-asset-seal");
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = final_asset_seal_flight(&service, &store, storage, thread, 155, asset);
    let request = flight.request();
    let assets = state.assets();
    service.test_arm_before_command_fault(move |store| advance_revision(store, assets, 156));
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::NotCommitted(DraftMarkerSealCommandStage::AssetSeal)
    );

    let key =
        DraftMarkerSealRequestV1::new(request.candidate().root(), request.operation_id()).key();
    let DraftMarkerSealStatusV1::Sealed(syndic, _) =
        storage.draft_marker_seal_status(&store, key).unwrap()
    else {
        panic!("final marker seal was not durable before competing Asset sealing");
    };
    let manifest = assets
        .staged_reference_set_manifest(&store, request.staging_authority())
        .unwrap();
    let seal = SealAssetReferenceSet::new(
        manifest.build_proof(),
        syndic.sequential(),
        syndic.ordered_assets(),
    )
    .unwrap();
    let expected = seal.sealed_proof();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(assets.seal_reference_set(assets.revision(&store).unwrap(), seal))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));

    let DraftMarkerSealDriveOutcome::ChangedNonempty {
        syndic: actual_syndic,
        assets: actual_assets,
    } = service.drive(&store, flight).unwrap()
    else {
        panic!("stale final Asset seal did not converge to the competing proof");
    };
    assert_eq!(actual_syndic, syndic);
    assert_eq!(actual_assets, expected);
    assert_eq!(service.diagnostics().current_flights(), 0);
}

#[test]
fn indeterminate_collision_is_classified_and_releases_app_custody() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase170-collision", 160, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 161, 162);
    let source = session.newest_root();
    let seed_request =
        DraftMarkerSealRequestV1::new(source, DraftMarkerSealOperationIdV1::from_bytes([163; 16]));
    let begin = storage
        .prepare_draft_marker_seal_begin(&store, seed_request)
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.begin_draft_marker_seal(storage.revision(&store).unwrap(), begin))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed { .. }
    ));

    let service = new_service(&store, storage, state.assets(), 1);
    let request = request(&session, 164, 165);
    service.test_arm_before_reconcile_fault(move |store, storage, request| {
        let (_, collision) =
            syndic_storage::inject_draft_marker_seal_natural_identity_collision_for_test(
                store,
                storage,
                seed_request.key(),
                request.operation_id(),
            );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command.add(collision).unwrap();
        assert!(matches!(
            store.execute(command),
            CommandOutcome::Committed { .. }
        ));
    });
    let flight = admitted(&service, &store, request);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        service.drive(&store, flight),
        Err(DraftMarkerSealServiceError::ReconciliationCollision)
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
    assert_eq!(store.pending_reconciliations().len(), 1);
}

#[test]
fn concurrent_drive_release_keeps_capacity_until_the_drive_settles() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase170-drive-release", 170, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 171, 172);
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = admitted(&service, &store, request(&session, 173, 174));
    let block = faults.block_next(FaultPoint::BeforeCommit);
    let store = Arc::new(store);
    let worker_service = service.clone();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || worker_service.drive(&worker_store, flight));
    assert!(block.wait_until_reached(Duration::from_secs(5)));
    assert_eq!(
        service
            .release(&store, flight, DraftMarkerSealReleaseIntent::Cancelled)
            .unwrap(),
        DraftMarkerSealReleaseOutcome::DeferredByActiveDrive(
            DraftMarkerSealReleaseIntent::Cancelled
        )
    );
    assert!(matches!(
        service.admit(
            &store,
            request(&session, 175, 176),
            &CommandCancellation::new()
        ),
        Ok(DraftMarkerSealAdmission::Saturated)
    ));
    let diagnostics = service.diagnostics();
    assert_eq!(diagnostics.current_flights(), 1);
    assert_eq!(diagnostics.driving_flights(), 1);
    assert_eq!(diagnostics.terminalizing_flights(), 1);
    block.release();
    assert_eq!(
        worker.join().unwrap().unwrap(),
        DraftMarkerSealDriveOutcome::TerminalSettlementPending(
            DraftMarkerSealReleaseIntent::Cancelled
        )
    );
    assert!(matches!(
        service
            .release(&store, flight, DraftMarkerSealReleaseIntent::Cancelled)
            .unwrap(),
        DraftMarkerSealReleaseOutcome::Settled { .. }
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
}

#[test]
fn concurrent_dispose_and_generation_retirement_never_stale_the_active_drive() {
    concurrent_dispose_case();
    concurrent_retire_case();
}

#[test]
fn final_asset_seal_races_release_dispose_and_generation_loss() {
    final_asset_seal_release_case();
    final_asset_seal_dispose_case();
    final_asset_seal_retire_case();
}

#[test]
fn indeterminate_final_asset_seal_reconciles_to_the_complete_asset_proof() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase171-asset-indeterminate", 240, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let asset = publish_asset(&store, &state, b"phase171-asset-indeterminate");
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = final_asset_seal_flight(&service, &store, storage, thread, 241, asset);
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert!(matches!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::ChangedNonempty { .. }
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
}

fn final_asset_seal_release_case() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase171-final-release", 210, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let asset = publish_asset(&store, &state, b"phase171-final-release");
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = final_asset_seal_flight(&service, &store, storage, thread, 211, asset);
    let block = faults.block_next(FaultPoint::BeforeCommit);
    let store = Arc::new(store);
    let worker_service = service.clone();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || worker_service.drive(&worker_store, flight));
    assert!(block.wait_until_reached(Duration::from_secs(5)));
    assert_eq!(
        service
            .release(&store, flight, DraftMarkerSealReleaseIntent::Cancelled)
            .unwrap(),
        DraftMarkerSealReleaseOutcome::DeferredByActiveDrive(
            DraftMarkerSealReleaseIntent::Cancelled
        )
    );
    block.release();
    assert!(matches!(
        worker.join().unwrap().unwrap(),
        DraftMarkerSealDriveOutcome::ChangedNonempty { .. }
    ));
    assert_eq!(
        service
            .release(&store, flight, DraftMarkerSealReleaseIntent::Cancelled)
            .unwrap(),
        DraftMarkerSealReleaseOutcome::AlreadyReleased
    );
}

fn final_asset_seal_dispose_case() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase171-final-dispose", 220, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let asset = publish_asset(&store, &state, b"phase171-final-dispose");
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = final_asset_seal_flight(&service, &store, storage, thread, 221, asset);
    let block = faults.block_next(FaultPoint::BeforeCommit);
    let store = Arc::new(store);
    let worker_service = service.clone();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || worker_service.drive(&worker_store, flight));
    assert!(block.wait_until_reached(Duration::from_secs(5)));
    assert_eq!(
        service.dispose(&store).unwrap(),
        DraftMarkerSealDisposeOutcome::WaitingForDrive { remaining: 1 }
    );
    block.release();
    assert!(matches!(
        worker.join().unwrap().unwrap(),
        DraftMarkerSealDriveOutcome::ChangedNonempty { .. }
    ));
    assert!(matches!(
        service.dispose(&store),
        Err(DraftMarkerSealServiceError::ServiceDisposed)
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
}

fn final_asset_seal_retire_case() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase171-final-retire", 230, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let asset = publish_asset(&store, &state, b"phase171-final-retire");
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = final_asset_seal_flight(&service, &store, storage, thread, 231, asset);
    let block = faults.block_next(FaultPoint::BeforeCommit);
    let store = Arc::new(store);
    let worker_service = service.clone();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || worker_service.drive(&worker_store, flight));
    assert!(block.wait_until_reached(Duration::from_secs(5)));
    assert_eq!(service.retire_home_generation().settling_drives(), 1);
    block.release();
    assert!(matches!(
        worker.join().unwrap(),
        Err(DraftMarkerSealServiceError::HomeGenerationChanged)
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
}

fn final_asset_seal_flight(
    service: &DraftMarkerSealService,
    store: &HomeStore,
    storage: syndic_storage::SyndicStorage,
    thread: SyndicThreadId,
    seed: u8,
    asset: beryl_model::AssetId,
) -> DraftMarkerSealFlight {
    let session = marker_session_with_marker(
        storage,
        store,
        thread,
        seed,
        DraftPieceMarkerV1::new(
            SyndicDraftMarkerId::from_bytes([seed; 16]),
            1,
            ImageLabelOrdinal::new(1).unwrap(),
            asset,
        ),
    );
    let flight = admitted(
        service,
        store,
        request(&session, seed.wrapping_add(1), seed.wrapping_add(2)),
    );
    assert_eq!(
        service.drive(store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    assert_eq!(
        service.drive(store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    flight
}

fn concurrent_dispose_case() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase170-drive-dispose", 180, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 181, 182);
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = admitted(&service, &store, request(&session, 183, 184));
    let block = faults.block_next(FaultPoint::BeforeCommit);
    let store = Arc::new(store);
    let worker_service = service.clone();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || worker_service.drive(&worker_store, flight));
    assert!(block.wait_until_reached(Duration::from_secs(5)));
    assert_eq!(
        service.dispose(&store).unwrap(),
        DraftMarkerSealDisposeOutcome::WaitingForDrive { remaining: 1 }
    );
    block.release();
    assert_eq!(
        worker.join().unwrap().unwrap(),
        DraftMarkerSealDriveOutcome::TerminalSettlementPending(
            DraftMarkerSealReleaseIntent::ServiceDisposed
        )
    );
    assert_eq!(
        service.dispose(&store).unwrap(),
        DraftMarkerSealDisposeOutcome::Disposed
    );
    assert_eq!(service.diagnostics().current_flights(), 0);
}

fn concurrent_retire_case() {
    let faults = FaultController::new();
    let (_home, mut store, storage, thread) =
        fixture_with_faults("app-phase170-drive-retire", 190, faults.clone());
    let state = BerylState::register(&mut store).unwrap();
    let durable = current(storage, &store, thread);
    let session = open_session(storage, &store, &durable, 191, 192);
    let service = new_service(&store, storage, state.assets(), 1);
    let flight = admitted(&service, &store, request(&session, 193, 194));
    let block = faults.block_next(FaultPoint::BeforeCommit);
    let store = Arc::new(store);
    let worker_service = service.clone();
    let worker_store = store.clone();
    let worker = std::thread::spawn(move || worker_service.drive(&worker_store, flight));
    assert!(block.wait_until_reached(Duration::from_secs(5)));
    let retired = service.retire_home_generation();
    assert_eq!(retired.released(), 0);
    assert_eq!(retired.settling_drives(), 1);
    assert_eq!(service.diagnostics().current_flights(), 1);
    block.release();
    assert!(matches!(
        worker.join().unwrap(),
        Err(DraftMarkerSealServiceError::HomeGenerationChanged)
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
}

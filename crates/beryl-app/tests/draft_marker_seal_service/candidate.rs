use super::*;
use beryl_app::composer_marker_seal::DraftMarkerSealRetainedFlights;
use beryl_home_store::{HomeHealthState, HomeRecoveryCandidate};

fn fail(store: &HomeStore, faults: &FaultController) {
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(store.home_revision().is_err());
    assert_eq!(store.health().state(), HomeHealthState::Failed);
}

fn bind(
    retained: &mut DraftMarkerSealRetainedFlights,
    candidate: &mut HomeRecoveryCandidate,
) -> (SyndicStorage, BerylState) {
    let storage = SyndicStorage::reacquire_candidate(candidate).unwrap();
    let state = BerylState::reacquire_candidate(candidate).unwrap();
    retained
        .bind_candidate(candidate, storage.clone(), state.assets())
        .unwrap();
    (storage, state)
}

#[test]
fn retained_capacity_coalesces_and_terminal_release_frees_the_original_slot() {
    let faults = FaultController::new();
    let (_home, store, state, storage, thread) =
        fixture_with_state_and_faults("candidate-capacity", 8, faults.clone());
    let session = open_session(&storage, &store, &current(&storage, &store, thread), 9, 10);
    let service = new_service(&store, &storage, state.assets(), 1, 1);
    let request = request(&session, 11, 12);
    let flight = admitted(&service, &store, request);
    assert_eq!(
        service.drive(&store, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    fail(&store, &faults);
    let mut retained = service.capture_failed_home(&store).unwrap();
    assert_eq!(service.diagnostics().current_flights(), 0);
    assert_eq!(service.diagnostics().retained_flights(), 1);
    assert!(service.drive(&store, flight).is_err());
    let mut candidate = store.recover_same_home().unwrap();
    let (storage, _state) = bind(&mut retained, &mut candidate);
    assert!(matches!(
        service.dispose(&candidate.service_reference()),
        Err(DraftMarkerSealServiceError::HomeGenerationChanged)
    ));
    assert_eq!(service.diagnostics().current_flights(), 1);
    assert_eq!(
        retained
            .admit_candidate(&mut candidate, request, &CommandCancellation::new())
            .unwrap(),
        DraftMarkerSealAdmission::Coalesced(flight)
    );
    let replaced_authority = DraftMarkerSealFlightRequest::new(
        DraftEditorCandidateActivationBindingV1::from_head(&session),
        DraftMarkerSealOperationIdV1::from_bytes([11; 16]),
        AssetReferenceSetStagingAuthority::new(AssetReferenceSetId::from_bytes([12; 16]), [99; 32]),
    );
    assert_eq!(
        retained
            .admit_candidate(
                &mut candidate,
                replaced_authority,
                &CommandCancellation::new()
            )
            .unwrap(),
        DraftMarkerSealAdmission::Conflict
    );
    let next = super::request(&session, 13, 14);
    assert_eq!(
        retained
            .admit_candidate(&mut candidate, next, &CommandCancellation::new())
            .unwrap(),
        DraftMarkerSealAdmission::Saturated
    );
    assert!(matches!(
        retained
            .release_candidate(
                &mut candidate,
                flight,
                DraftMarkerSealReleaseIntent::Cancelled
            )
            .unwrap(),
        DraftMarkerSealReleaseOutcome::Settled {
            intent: DraftMarkerSealReleaseIntent::Cancelled,
            ..
        }
    ));
    let access = candidate.recovery_access().unwrap();
    let key =
        DraftMarkerSealRequestV1::new(request.candidate().root(), request.operation_id()).key();
    assert!(matches!(
        storage
            .draft_marker_seal_status_candidate(&access, key)
            .unwrap(),
        DraftMarkerSealStatusV1::Cancelled(_)
    ));
    drop(access);
    assert!(matches!(
        retained.drive_candidate(&mut candidate, flight),
        Err(DraftMarkerSealServiceError::StaleFlight)
    ));
    assert!(matches!(
        retained
            .admit_candidate(&mut candidate, next, &CommandCancellation::new())
            .unwrap(),
        DraftMarkerSealAdmission::Admitted(_)
    ));
    assert_eq!(service.diagnostics().high_water_flights(), 1);
}

#[test]
fn candidate_resumes_each_original_nonempty_stage_without_publishing_an_owner() {
    for steps in 0..3 {
        let faults = FaultController::new();
        let (_home, store, state, storage, thread) =
            fixture_with_state_and_faults("candidate-resume", 30 + steps, faults.clone());
        let session = published_marker_session(&storage, &store, &state, thread, 40 + steps);
        let service = new_service(&store, &storage, state.assets(), 1, 1);
        let flight = admitted(&service, &store, request(&session, 50, 51));
        for _ in 0..steps {
            assert_eq!(
                service.drive(&store, flight).unwrap(),
                DraftMarkerSealDriveOutcome::Progress
            );
        }
        fail(&store, &faults);
        let mut retained = service.capture_failed_home(&store).unwrap();
        let mut candidate = store.recover_same_home().unwrap();
        let (_storage, state) = bind(&mut retained, &mut candidate);
        let mut complete = false;
        for _ in 0..5 {
            match retained.drive_candidate(&mut candidate, flight).unwrap() {
                DraftMarkerSealDriveOutcome::Progress => {}
                DraftMarkerSealDriveOutcome::ChangedNonempty { .. } => {
                    complete = true;
                    break;
                }
                other => panic!("unexpected recovery stage: {other:?}"),
            }
        }
        assert!(complete);
        assert_eq!(service.diagnostics().current_flights(), 0);
        assert_eq!(service.diagnostics().retained_flights(), 0);
        let access = candidate.recovery_access().unwrap();
        assert!(
            state
                .assets()
                .owner_head_candidate(&access, AssetOwner::CurrentDraft(session.draft_id()))
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn original_indeterminate_command_reconciles_before_candidate_resume_at_each_stage() {
    for steps in 0..3 {
        let faults = FaultController::new();
        let (_home, store, state, storage, thread) =
            fixture_with_state_and_faults("candidate-original-outcome", 60 + steps, faults.clone());
        let session = published_marker_session(&storage, &store, &state, thread, 70 + steps);
        let service = new_service(&store, &storage, state.assets(), 1, 1);
        let flight = admitted(&service, &store, request(&session, 80, 81));
        for _ in 0..steps {
            assert_eq!(
                service.drive(&store, flight).unwrap(),
                DraftMarkerSealDriveOutcome::Progress
            );
        }
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let failure_faults = faults.clone();
        service.test_arm_before_reconcile_fault(move |store, _, _| fail(store, &failure_faults));
        assert!(matches!(
            service.drive(&store, flight),
            Err(DraftMarkerSealServiceError::Reconciliation(_))
        ));
        let mut retained = service.capture_failed_home(&store).unwrap();
        let mut candidate = store.recover_same_home().unwrap();
        bind(&mut retained, &mut candidate);
        let mut complete = false;
        for _ in 0..5 {
            match retained.drive_candidate(&mut candidate, flight).unwrap() {
                DraftMarkerSealDriveOutcome::Progress => {}
                DraftMarkerSealDriveOutcome::ChangedNonempty { .. } => {
                    complete = true;
                    break;
                }
                other => panic!("unexpected recovery outcome: {other:?}"),
            }
        }
        assert!(complete);
    }
}

#[test]
fn candidate_read_failure_rebinds_to_fresh_generation_with_the_same_flight() {
    let faults = FaultController::new();
    let (_home, store, state, storage, thread) =
        fixture_with_state_and_faults("candidate-rebind", 90, faults.clone());
    let session = published_marker_session(&storage, &store, &state, thread, 91);
    let service = new_service(&store, &storage, state.assets(), 1, 1);
    let flight = admitted(&service, &store, request(&session, 100, 101));
    fail(&store, &faults);
    let mut retained = service.capture_failed_home(&store).unwrap();
    let mut candidate = store.recover_same_home().unwrap();
    bind(&mut retained, &mut candidate);
    let generation = candidate.generation();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(retained.drive_candidate(&mut candidate, flight).is_err());
    let mut candidate = candidate.abort().recover_same_home().unwrap();
    assert_ne!(generation, candidate.generation());
    bind(&mut retained, &mut candidate);
    assert_eq!(
        retained.drive_candidate(&mut candidate, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    assert_eq!(
        retained.drive_candidate(&mut candidate, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
    assert!(matches!(
        retained.drive_candidate(&mut candidate, flight).unwrap(),
        DraftMarkerSealDriveOutcome::ChangedNonempty { .. }
    ));
}

#[test]
fn collision_custody_blocks_all_candidate_writes_and_survives_drop() {
    let faults = FaultController::new();
    let (_home, store, state, storage, thread) =
        fixture_with_state_and_faults("candidate-collision", 110, faults.clone());
    let session = open_session(
        &storage,
        &store,
        &current(&storage, &store, thread),
        111,
        112,
    );
    let service = new_service(&store, &storage, state.assets(), 2, 1);
    let flight = admitted(&service, &store, request(&session, 113, 114));
    service.test_fail_next_drive_as_collision();
    assert!(matches!(
        service.drive(&store, flight),
        Err(DraftMarkerSealServiceError::ReconciliationCollision)
    ));
    assert_eq!(service.diagnostics().current_flights(), 0);
    assert_eq!(service.diagnostics().retained_flights(), 1);
    fail(&store, &faults);
    let mut retained = service.capture_failed_home(&store).unwrap();
    let mut candidate = store.recover_same_home().unwrap();
    bind(&mut retained, &mut candidate);
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    assert!(matches!(
        retained.drive_candidate(&mut candidate, flight),
        Err(DraftMarkerSealServiceError::ReconciliationCollision)
    ));
    assert!(matches!(
        retained.release_candidate(
            &mut candidate,
            flight,
            DraftMarkerSealReleaseIntent::Cancelled
        ),
        Err(DraftMarkerSealServiceError::ReconciliationCollision)
    ));
    assert!(matches!(
        retained.admit_candidate(
            &mut candidate,
            request(&session, 115, 116),
            &CommandCancellation::new()
        ),
        Err(DraftMarkerSealServiceError::ReconciliationCollision)
    ));
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    drop(retained);
    assert_eq!(service.diagnostics().retained_flights(), 1);
    assert_eq!(service.retire_home_generation().released(), 0);
}

#[test]
fn candidate_command_cuts_rebind_and_reconcile_at_every_nonempty_stage() {
    for indeterminate in [false, true] {
        for steps in 0..3 {
            let faults = FaultController::new();
            let (_home, store, state, storage, thread) =
                fixture_with_state_and_faults("candidate-command-cut", 120 + steps, faults.clone());
            let session = published_marker_session(&storage, &store, &state, thread, 130 + steps);
            let service = new_service(&store, &storage, state.assets(), 1, 1);
            fail(&store, &faults);
            let mut retained = service.capture_failed_home(&store).unwrap();
            let mut candidate = store.recover_same_home().unwrap();
            bind(&mut retained, &mut candidate);
            let flight = match retained
                .admit_candidate(
                    &mut candidate,
                    request(&session, 140, 141),
                    &CommandCancellation::new(),
                )
                .unwrap()
            {
                DraftMarkerSealAdmission::Admitted(flight) => flight,
                other => panic!("fresh recovery admission failed: {other:?}"),
            };
            for _ in 0..steps {
                assert_eq!(
                    retained.drive_candidate(&mut candidate, flight).unwrap(),
                    DraftMarkerSealDriveOutcome::Progress
                );
            }
            if indeterminate {
                faults.fail_next(FaultPoint::AfterCommitBeforePersist);
                faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
                assert!(matches!(
                    retained.drive_candidate(&mut candidate, flight),
                    Err(DraftMarkerSealServiceError::Reconciliation(_))
                ));
            } else {
                faults.fail_next(FaultPoint::BeforeCommit);
                let stage = [
                    DraftMarkerSealCommandStage::Begin,
                    DraftMarkerSealCommandStage::Page,
                    DraftMarkerSealCommandStage::AssetSeal,
                ][steps as usize];
                assert_eq!(
                    retained.drive_candidate(&mut candidate, flight).unwrap(),
                    DraftMarkerSealDriveOutcome::NotCommitted(stage)
                );
            }
            let mut candidate = candidate.abort().recover_same_home().unwrap();
            let (_storage, state) = bind(&mut retained, &mut candidate);
            let mut complete = false;
            for _ in 0..5 {
                match retained.drive_candidate(&mut candidate, flight).unwrap() {
                    DraftMarkerSealDriveOutcome::Progress => {}
                    DraftMarkerSealDriveOutcome::ChangedNonempty { .. } => {
                        complete = true;
                        break;
                    }
                    other => panic!("candidate cut failed to resume: {other:?}"),
                }
            }
            assert!(complete);
            let access = candidate.recovery_access().unwrap();
            assert!(
                state
                    .assets()
                    .owner_head_candidate(&access, AssetOwner::CurrentDraft(session.draft_id()))
                    .unwrap()
                    .is_none()
            );
            assert_eq!(service.diagnostics().high_water_flights(), 1);
        }
    }
}

#[test]
fn candidate_terminal_cuts_retain_exact_intent_until_authenticated_settlement() {
    for indeterminate in [false, true] {
        for mode in 0..3 {
            let faults = FaultController::new();
            let (_home, store, state, storage, thread) =
                fixture_with_state_and_faults("candidate-terminal-cut", 150 + mode, faults.clone());
            let session = open_session(
                &storage,
                &store,
                &current(&storage, &store, thread),
                160,
                161,
            );
            let service = new_service(&store, &storage, state.assets(), 1, 1);
            let original = request(&session, 162, 163);
            let flight = admitted(&service, &store, original);
            assert_eq!(
                service.drive(&store, flight).unwrap(),
                DraftMarkerSealDriveOutcome::Progress
            );
            let successor = complete_staged(
                &storage,
                &store,
                &session,
                164,
                DraftPieceReplacementV1::new(
                    point(0),
                    point(0),
                    vec![DraftPieceV1::Text("successor".into())],
                ),
                DraftLogicalExtentV1::new(9, 1),
            );
            let intent = match mode {
                0 => DraftMarkerSealReleaseIntent::Cancelled,
                1 => DraftMarkerSealReleaseIntent::Failed(
                    DraftMarkerSealFailureReasonV1::Operational,
                ),
                _ => DraftMarkerSealReleaseIntent::Superseded {
                    successor_operation_id: DraftMarkerSealOperationIdV1::from_bytes([165; 16]),
                    successor: DraftEditorCandidateActivationBindingV1::from_head(&successor),
                },
            };
            fail(&store, &faults);
            let mut retained = service.capture_failed_home(&store).unwrap();
            let mut candidate = store.recover_same_home().unwrap();
            bind(&mut retained, &mut candidate);
            if indeterminate {
                faults.fail_next(FaultPoint::AfterCommitBeforePersist);
                faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
                assert!(matches!(
                    retained.release_candidate(&mut candidate, flight, intent),
                    Err(DraftMarkerSealServiceError::Reconciliation(_))
                ));
            } else {
                faults.fail_next(FaultPoint::BeforeCommit);
                assert_eq!(
                    retained
                        .release_candidate(&mut candidate, flight, intent)
                        .unwrap(),
                    DraftMarkerSealReleaseOutcome::NotCommitted(intent)
                );
            }
            assert_eq!(service.diagnostics().current_flights(), 1);
            let mut candidate = candidate.abort().recover_same_home().unwrap();
            let (storage, _state) = bind(&mut retained, &mut candidate);
            assert!(matches!(
                retained.drive_candidate(&mut candidate, flight),
                Err(DraftMarkerSealServiceError::TerminalSettlementRequired)
            ));
            assert!(
                matches!(retained.release_candidate(&mut candidate, flight, intent).unwrap(), DraftMarkerSealReleaseOutcome::Settled { intent: actual, .. } if actual == intent)
            );
            let access = candidate.recovery_access().unwrap();
            let status = storage
                .draft_marker_seal_status_candidate(
                    &access,
                    DraftMarkerSealRequestV1::new(
                        original.candidate().root(),
                        original.operation_id(),
                    )
                    .key(),
                )
                .unwrap();
            assert!(match (mode, status) {
                (0, DraftMarkerSealStatusV1::Cancelled(_)) => true,
                (
                    1,
                    DraftMarkerSealStatusV1::Failed {
                        reason: DraftMarkerSealFailureReasonV1::Operational,
                        ..
                    },
                ) => true,
                (2, DraftMarkerSealStatusV1::Superseded { successor, .. }) =>
                    successor == DraftMarkerSealOperationIdV1::from_bytes([165; 16]),
                _ => false,
            });
            assert_eq!(service.diagnostics().current_flights(), 0);
        }
    }
}

#[test]
fn candidate_binding_refuses_stale_and_foreign_backends_without_consuming_custody() {
    let faults = FaultController::new();
    let (_home, store, state, storage, thread) =
        fixture_with_state_and_faults("candidate-bind-refusal", 180, faults.clone());
    let session = open_session(
        &storage,
        &store,
        &current(&storage, &store, thread),
        181,
        182,
    );
    let service = new_service(&store, &storage, state.assets(), 1, 1);
    let flight = admitted(&service, &store, request(&session, 183, 184));
    fail(&store, &faults);
    let mut retained = service.capture_failed_home(&store).unwrap();
    let mut candidate = store.recover_same_home().unwrap();
    assert!(
        retained
            .bind_candidate(&mut candidate, storage, state.assets())
            .is_err()
    );
    assert_eq!(
        retained.captured_flights().collect::<Vec<_>>(),
        vec![flight]
    );
    let (_foreign_home, _foreign_store, foreign_state, foreign_storage, _) =
        fixture_with_state("candidate-foreign-backends", 190);
    assert!(
        retained
            .bind_candidate(&mut candidate, foreign_storage, foreign_state.assets())
            .is_err()
    );
    assert_eq!(service.diagnostics().retained_flights(), 1);
    bind(&mut retained, &mut candidate);
    assert_eq!(
        retained.drive_candidate(&mut candidate, flight).unwrap(),
        DraftMarkerSealDriveOutcome::Progress
    );
}

use super::*;

#[test]
fn candidate_cancellation_inspection_matches_prior_committed_and_mixed_outcomes() {
    for expected in [
        BindingPublicationStatus::Prior,
        BindingPublicationStatus::Exact,
        BindingPublicationStatus::Collision,
    ] {
        let home = TestHome::new("candidate-cancellation-outcome");
        let mut candidate = open(home.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let store = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let fixture = activate_pending(&store, &storage, 170, false);
        let request = cancellation(&store, &storage, &fixture);
        let old = state(&store, &storage, fixture.turn);
        if expected != BindingPublicationStatus::Prior {
            execute(
                &store,
                storage
                    .cancel_binding_activation(storage.revision(&store).unwrap(), request.clone()),
            );
        }
        if expected == BindingPublicationStatus::Collision {
            commit(
                &store,
                storage.clone(),
                batch([FixtureRecord::TurnState(old)]),
            );
        }
        assert_eq!(
            storage
                .cancelled_binding_activation_status(&store, &request, point_limit())
                .unwrap(),
            expected
        );
        store.close().unwrap();
        let mut candidate = open(home.path());
        let fresh = SyndicStorage::register(&mut candidate).unwrap();
        let mut candidate = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap();
        let access = candidate.recovery_access().unwrap();
        assert_eq!(
            fresh
                .cancelled_binding_activation_status_candidate(&access, &request, point_limit())
                .unwrap(),
            expected
        );
        assert!(
            storage
                .cancelled_binding_activation_status_candidate(&access, &request, point_limit())
                .is_err()
        );
        assert!(
            fresh
                .cancelled_binding_activation_status_candidate(
                    &access,
                    &request,
                    SyndicPointReadLimit::new(1).unwrap()
                )
                .is_err()
        );
        candidate.publish().unwrap().close().unwrap();
    }
}

#[test]
fn uncertain_cancellation_reconciles_through_fresh_candidate_handles() {
    let home = TestHome::new("candidate-cancellation-uncertainty");
    let faults = FaultController::new();
    let mut candidate = beryl_home_store::HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let fixture = activate_pending(&store, &storage, 170, false);
    let request = cancellation(&store, &storage, &fixture);
    let contribution =
        storage.cancel_binding_activation(storage.revision(&store).unwrap(), request.clone());
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let CommandOutcome::Indeterminate { reconciliation, .. } =
        execute_outcome(&store, contribution)
    else {
        panic!("expected uncertain cancellation");
    };
    let handle = reconciliation.install_and_handle();
    if store.health().state() == beryl_home_store::HomeHealthState::Healthy {
        faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(store.home_revision().is_err());
    }
    let mut candidate = store.recover_same_home().unwrap();
    let fresh = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(
        fresh
            .cancelled_binding_activation_status_candidate(&access, &request, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    assert!(
        storage
            .cancelled_binding_activation_status_candidate(&access, &request, point_limit())
            .is_err()
    );
    assert!(matches!(
        access.reconcile(&handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    candidate.publish().unwrap().close().unwrap();
}

#[test]
fn indeterminate_activation_and_cancellation_reconcile_the_complete_publication() {
    for cancel in [false, true] {
        let home = TestHome::new("dispatch-indeterminate-publication");
        let faults = FaultController::new();
        let mut store = beryl_home_store::HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        let store = store
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let fixture = activate_pending(&store, &storage, 170, false);
        let cancellation = cancellation(&store, &storage, &fixture);
        if !cancel {
            execute(
                &store,
                storage.cancel_binding_activation(
                    storage.revision(&store).unwrap(),
                    cancellation.clone(),
                ),
            );
        }
        let activation = retry(
            &store,
            &storage,
            &fixture,
            SyndicExecutionSnapshotId::from_bytes([190; 16]),
        );
        let contribution = if cancel {
            storage
                .cancel_binding_activation(storage.revision(&store).unwrap(), cancellation.clone())
        } else {
            storage.activate_binding(storage.revision(&store).unwrap(), activation.clone())
        };
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let CommandOutcome::Indeterminate { reconciliation, .. } =
            execute_outcome(&store, contribution)
        else {
            panic!("expected indeterminate dispatch publication");
        };
        let handle = reconciliation.install_and_handle();
        assert!(matches!(
            store.reconcile(&handle).unwrap(),
            ReconciliationResolution::ExactNew { .. }
        ));
        let status = if cancel {
            storage.cancelled_binding_activation_status(&store, &cancellation, point_limit())
        } else {
            storage.binding_activation_status(&store, &activation, point_limit())
        }
        .unwrap();
        assert_eq!(status, BindingPublicationStatus::Exact);
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
        store.close().unwrap();
        let mut reopened_candidate = open(home.path());
        SyndicStorage::register(&mut reopened_candidate).unwrap();
        let reopened = reopened_candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        reopened
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap();
        reopened.close().unwrap();
    }
}

#[test]
fn indeterminate_dispatch_publication_rejects_a_mixed_turn_state() {
    for cancel in [false, true] {
        let home = TestHome::new("dispatch-indeterminate-mixed-state");
        let faults = FaultController::new();
        let mut store = beryl_home_store::HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
        let store = store
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let fixture = activate_pending(&store, &storage, 170, false);
        let cancellation = cancellation(&store, &storage, &fixture);
        if !cancel {
            execute(
                &store,
                storage.cancel_binding_activation(
                    storage.revision(&store).unwrap(),
                    cancellation.clone(),
                ),
            );
        }
        let before = state(&store, &storage, fixture.turn);
        let activation = retry(
            &store,
            &storage,
            &fixture,
            SyndicExecutionSnapshotId::from_bytes([190; 16]),
        );
        let contribution = if cancel {
            storage
                .cancel_binding_activation(storage.revision(&store).unwrap(), cancellation.clone())
        } else {
            storage.activate_binding(storage.revision(&store).unwrap(), activation.clone())
        };
        faults.fail_next(FaultPoint::AfterCommitBeforePersist);
        let CommandOutcome::Indeterminate { reconciliation, .. } =
            execute_outcome(&store, contribution)
        else {
            panic!("expected indeterminate dispatch publication");
        };
        let handle = reconciliation.install_and_handle();
        commit(
            &store,
            storage.clone(),
            batch([FixtureRecord::TurnState(before)]),
        );
        let status = if cancel {
            storage.cancelled_binding_activation_status(&store, &cancellation, point_limit())
        } else {
            storage.binding_activation_status(&store, &activation, point_limit())
        }
        .unwrap();
        assert_eq!(status, BindingPublicationStatus::Collision);
        assert_eq!(
            store.reconcile(&handle).unwrap(),
            ReconciliationResolution::Collision
        );
        drop(store);
    }
}

use super::*;

#[test]
fn indeterminate_activation_and_cancellation_reconcile_the_complete_publication() {
    for cancel in [false, true] {
        let home = TestHome::new("dispatch-indeterminate-publication");
        let faults = FaultController::new();
        let mut store = HomeStore::open_with_faults(
            HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
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
        let mut reopened = open(home.path());
        SyndicStorage::register(&mut reopened).unwrap();
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
        let mut store = HomeStore::open_with_faults(
            HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut store).unwrap();
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

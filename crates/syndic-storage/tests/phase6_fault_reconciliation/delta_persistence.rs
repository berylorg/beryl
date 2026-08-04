use super::*;

#[test]
fn delta_persistence_cuts_reconcile_to_wholly_old_or_wholly_new_history() {
    for (name, point, delta_persisted) in [
        (
            "phase6-delta-before-commit",
            FaultPoint::BeforeCommit,
            false,
        ),
        (
            "phase6-delta-after-commit-before-persist",
            FaultPoint::AfterCommitBeforePersist,
            true,
        ),
        ("phase6-delta-after-persist", FaultPoint::AfterPersist, true),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = open_with_faults(home.path(), faults.clone());
        let storage = SyndicStorage::register(&mut store).unwrap();
        commit(&store, storage, batch(populated_records()));
        let item = SyndicItemId::from_bytes([71; 16]);
        let cas_item = CasItemId::new("phase6-fault-item").unwrap();
        start_item(&store, storage, item, &cas_item);
        let baseline_turn_events = storage
            .turn_state(&store, active_turn(), limit())
            .unwrap()
            .unwrap()
            .source_event_count();
        let baseline_item_events = storage
            .canonical_item(&store, item, limit())
            .unwrap()
            .unwrap()
            .source_event_count();
        let delta_frame = stage_item_frame_for_publication(
            &store,
            storage,
            item,
            agent_delta(cas_item, "atomic delta"),
        );
        let delta = source_event(
            &store,
            storage,
            SourceEventPayload::ItemFrame {
                item_id: item,
                frame: Box::new(delta_frame),
            },
            timestamp(10),
        );
        let mut command = HomeCommand::new(store.home_revision().unwrap());
        command
            .add(storage.admit_live_source_event(storage.revision(&store).unwrap(), delta.clone()))
            .unwrap();

        faults.fail_next(point);
        assert!(store.execute(command).is_err());
        assert_eq!(store.health().state(), HomeHealthState::Verifying);
        store.verify_health().unwrap();
        assert_eq!(
            storage
                .turn_state(&store, active_turn(), limit())
                .unwrap()
                .unwrap()
                .source_event_count(),
            baseline_turn_events + u64::from(delta_persisted)
        );
        assert_eq!(
            storage
                .canonical_item(&store, item, limit())
                .unwrap()
                .unwrap()
                .source_event_count(),
            baseline_item_events + u64::from(delta_persisted)
        );
        assert_eq!(
            item_text(&store, storage, item),
            if delta_persisted { "atomic delta" } else { "" }
        );
        store.validate_registered_domains().unwrap();
        store.close().unwrap();

        let mut reopened = open(home.path());
        let storage = SyndicStorage::register(&mut reopened).unwrap();
        reopened.validate_registered_domains().unwrap();
        assert_eq!(
            item_text(&reopened, storage, item),
            if delta_persisted { "atomic delta" } else { "" }
        );
        let retry = execute(
            &reopened,
            storage.admit_live_source_event(storage.revision(&reopened).unwrap(), delta),
        );
        if !delta_persisted {
            retry.unwrap();
        } else {
            assert!(matches!(
                typed_error(&retry.unwrap_err()),
                SyndicMutationError::SourceEventAlreadyAdmitted
            ));
        }
        reopened.validate_registered_domains().unwrap();
        assert_eq!(item_text(&reopened, storage, item), "atomic delta");
        assert_eq!(
            storage
                .turn_state(&reopened, active_turn(), limit())
                .unwrap()
                .unwrap()
                .source_event_count(),
            baseline_turn_events + 1
        );
        reopened.close().unwrap();
    }
}

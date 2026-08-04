use super::*;

#[test]
fn proven_not_dispatched_activation_returns_to_the_same_valid_projection() {
    let home = TestHome::new("phase13-binding-activation-cancellation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(31);
    let draft = draft_id(32);
    let replacement = draft_id(33);
    create_thread(&store, storage, thread, draft);
    save_text(&store, storage, thread, "not dispatched", timestamp(2));
    let (turn, selected) = submit_root_turn(
        &store,
        storage,
        thread,
        draft,
        replacement,
        SyndicItemId::from_bytes([34; 16]),
        timestamp(3),
    );
    let cas_thread = CasThreadId::new("phase13-cancelled-start").unwrap();
    let valid = valid_request(thread, selected, cas_thread.clone());
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), valid),
    );
    let snapshot = SyndicExecutionSnapshotId::from_bytes([35; 16]);
    let activation = ActivateBinding::new(
        thread,
        BindingRevision::new(3).unwrap(),
        InputGateRevision::new(2).unwrap(),
        selected,
        snapshot,
        turn,
        loaded_generation(),
        timestamp(4),
    );
    execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activation),
    );
    let cancellation = CancelBindingActivation::new(
        thread,
        BindingRevision::new(4).unwrap(),
        InputGateRevision::new(3).unwrap(),
        selected,
        snapshot,
        turn,
    );
    assert_eq!(
        storage
            .cancelled_binding_activation_status(&store, &cancellation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.cancel_binding_activation(storage.revision(&store).unwrap(), cancellation.clone()),
    );
    assert_eq!(
        storage
            .cancelled_binding_activation_status(&store, &cancellation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    let binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("cancelled activation did not restore a valid binding");
    };
    assert_eq!(usable.cas_thread_id(), &cas_thread);
    assert_eq!(
        usable.native_turn_count(),
        beryl_model::CasNativeTurnCount::ZERO
    );
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.revision(), InputGateRevision::new(4).unwrap());
    assert_eq!(gate.state(), &InputGateState::PendingTurn(turn));
    assert!(
        storage
            .active_cas_turn(&store, snapshot, point_limit())
            .unwrap()
            .is_none()
    );
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    assert_eq!(
        storage
            .cancelled_binding_activation_status(&reopened, &cancellation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    reopened.close().unwrap();
}

use super::*;

fn admit_next_turn_input(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    next_draft: SyndicDraftId,
    text: &str,
    admitted_at: SyndicTimestamp,
) {
    save_text(store, storage, thread, text, admitted_at);
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        thread,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        next_draft,
        None,
        admitted_at,
    );
    execute(
        store,
        storage.admit_accepted_input(storage.revision(store).unwrap(), admission),
    )
    .unwrap();
}

#[test]
fn consecutive_empty_active_epochs_allocate_distinct_generations() {
    let home = TestHome::new("phase13-consecutive-empty-route-generations");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, parent, turn, selected) = non_root_pending(&store, storage);
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.route_generation_high_water(),
        Some(AcceptedRouteGeneration::FIRST)
    );

    let parent = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let recovered = RecoveredInjectionProof::new(
        RecoveryProjectionVersion::V1,
        represented,
        RecoveryItemSequenceDigest::from_bytes([0xD6; 32]),
        RecoveryItemCount::new(1).unwrap(),
        RecoveryUtf8ByteCount::new(1).unwrap(),
        timestamp(6),
        loaded_generation(26, 27),
    )
    .unwrap();
    publish_valid(
        &store,
        storage,
        valid_request(
            &store,
            storage,
            thread,
            selected,
            CasThreadId::new("phase13-consecutive-empty-cas").unwrap(),
            represented,
            CasLineageProof::recovered(recovered),
        ),
    );
    execute(
        &store,
        storage.activate_binding(
            storage.revision(&store).unwrap(),
            ActivateBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                current_gate_revision(&store, storage, thread),
                selected,
                SyndicExecutionSnapshotId::from_bytes([0xD7; 16]),
                turn,
                loaded_generation(26, 28),
                timestamp(8),
            ),
        ),
    )
    .unwrap();
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.route_generation_high_water(),
        Some(AcceptedRouteGeneration::new(2).unwrap())
    );
    assert_eq!(
        gate.selected_route().unwrap().generation(),
        AcceptedRouteGeneration::new(2).unwrap()
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn unselected_next_turn_generations_and_later_activation_share_one_allocator() {
    let home = TestHome::new("phase13-route-generation-interleaving");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn, selected) = root_pending(&store, storage);

    admit_next_turn_input(
        &store,
        storage,
        thread,
        draft_id(7),
        "first queued input",
        timestamp(4),
    );
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.route_generation_high_water(),
        Some(AcceptedRouteGeneration::FIRST)
    );
    assert!(gate.selected_route().is_none());

    admit_next_turn_input(
        &store,
        storage,
        thread,
        draft_id(8),
        "second queued input",
        timestamp(5),
    );
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.route_generation_high_water(),
        Some(AcceptedRouteGeneration::new(2).unwrap())
    );
    assert!(gate.selected_route().is_none());

    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    publish_valid(
        &store,
        storage,
        valid_request(
            &store,
            storage,
            thread,
            selected,
            CasThreadId::new("phase13-route-allocator-cas").unwrap(),
            represented,
            CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
        ),
    );
    execute(
        &store,
        storage.activate_binding(
            storage.revision(&store).unwrap(),
            ActivateBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                current_gate_revision(&store, storage, thread),
                selected,
                SyndicExecutionSnapshotId::from_bytes([0xD4; 16]),
                turn,
                loaded_generation(22, 23),
                timestamp(6),
            ),
        ),
    )
    .unwrap();

    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let active_generation = AcceptedRouteGeneration::new(3).unwrap();
    assert_eq!(gate.route_generation_high_water(), Some(active_generation));
    assert_eq!(
        gate.selected_route().unwrap().generation(),
        active_generation
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn route_generation_exhaustion_rejects_without_overwrite() {
    let home = TestHome::new("phase13-route-generation-exhaustion");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn, selected) = root_pending(&store, storage);
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let exhausted = InputGateRecord::new(
        thread,
        gate.revision(),
        gate.state().clone(),
        gate.accepted_high_water(),
        Some(AcceptedRouteGeneration::new(u64::MAX).unwrap()),
        gate.selected_route(),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    commit(
        &store,
        storage,
        batch([FixtureRecord::InputGate(exhausted)]),
    );

    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    publish_valid(
        &store,
        storage,
        valid_request(
            &store,
            storage,
            thread,
            selected,
            CasThreadId::new("phase13-route-exhaustion-cas").unwrap(),
            represented,
            CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
        ),
    );
    let error = execute(
        &store,
        storage.activate_binding(
            storage.revision(&store).unwrap(),
            ActivateBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                current_gate_revision(&store, storage, thread),
                selected,
                SyndicExecutionSnapshotId::from_bytes([0xD5; 16]),
                turn,
                loaded_generation(24, 25),
                timestamp(6),
            ),
        ),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::Value(SyndicValueError::OrdinalExhausted {
            kind: "accepted-route generation"
        })
    ));
    assert!(
        storage
            .execution_snapshot(
                &store,
                SyndicExecutionSnapshotId::from_bytes([0xD5; 16]),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    store.close().unwrap();
}

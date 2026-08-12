use super::*;

#[test]
fn valid_activation_and_one_way_turn_publication_are_atomic_and_reopen_cleanly() {
    let home = TestHome::new("phase9-binding-lifecycle");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(1);
    let draft = draft_id(2);
    let replacement = draft_id(3);
    create_thread(&store, storage, thread, draft);
    save_text(&store, storage, thread, "start", timestamp(2));
    let (turn, selected) = submit_root_turn(
        &store,
        storage,
        thread,
        draft,
        replacement,
        SyndicItemId::from_bytes([4; 16]),
        timestamp(3),
    );

    let cas_thread = CasThreadId::new("phase9-active-thread").unwrap();
    let valid = valid_request(thread, selected, cas_thread.clone());
    assert_eq!(
        storage
            .valid_binding_publication_status(&store, &valid, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), valid.clone()),
    );
    assert_eq!(
        storage
            .valid_binding_publication_status(&store, &valid, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );

    let snapshot = SyndicExecutionSnapshotId::from_bytes([5; 16]);
    let activate = ActivateBinding::new(
        thread,
        BindingRevision::new(3).unwrap(),
        InputGateRevision::new(2).unwrap(),
        selected,
        snapshot,
        turn,
        loaded_generation(),
        timestamp(4),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activate, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activate.clone()),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activate, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    let immutable_snapshot = storage
        .execution_snapshot(&store, snapshot, point_limit())
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(
        immutable_snapshot.activation_gate_revision(),
        InputGateRevision::new(3).unwrap()
    );
    assert_eq!(
        immutable_snapshot.represented_base_native_turn_count(),
        beryl_model::CasNativeTurnCount::ZERO
    );
    assert_eq!(immutable_snapshot.tool_profile(), valid.tool_profile());

    save_text(&store, storage, thread, "steer", timestamp(5));
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        thread,
        current.thread().revision(),
        replacement,
        current.draft().revision(),
        current.draft().content(),
        InputGateRevision::new(3).unwrap(),
        draft_id(6),
        None,
        timestamp(6),
    );
    let accepted = admission.accepted_input_id();
    execute(
        &store,
        storage.admit_accepted_input(storage.revision(&store).unwrap(), admission.clone()),
    );

    let publish = PublishActiveCasTurn::new(
        thread,
        BindingRevision::new(4).unwrap(),
        InputGateRevision::new(4).unwrap(),
        snapshot,
        cas_thread.clone(),
        CasTurnId::new("phase9-active-turn").unwrap(),
        timestamp(7),
    );
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&store, &publish, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Absent
    );
    execute(
        &store,
        storage.publish_active_cas_turn(storage.revision(&store).unwrap(), publish.clone()),
    );
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&store, &publish, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Exact
    );
    assert_eq!(
        storage
            .accepted_input_status(&store, &admission, point_limit())
            .unwrap(),
        InputAdmissionStatus::ExactAccepted
    );
    let cas_turn_index = storage
        .cas_turn_owner(
            &store,
            publish.cas_thread_id().clone(),
            publish.cas_turn_id().clone(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(cas_turn_index.post_turn_native_count().get(), 1);

    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.revision(), InputGateRevision::new(5).unwrap());
    assert!(matches!(gate.state(), InputGateState::Steerable(_)));
    let proof = gate.selected_route().unwrap();
    let page = storage
        .accepted_route_page(&store, thread, proof.generation(), proof.revision(), None)
        .unwrap();
    let input = page
        .records()
        .iter()
        .find(|row| row.input().id() == accepted)
        .unwrap();
    assert_eq!(input.leaf().revision().get(), 1);
    assert_eq!(input.effective_state(), AcceptedRouteEffectiveState::Ready);
    assert_eq!(
        storage
            .execution_snapshot(&store, snapshot, point_limit())
            .unwrap()
            .unwrap(),
        immutable_snapshot
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&reopened, &publish, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Exact
    );
    reopened.close().unwrap();
}

#[test]
fn queued_admission_revision_descendant_preserves_binding_activation() {
    let home = TestHome::new("phase62-binding-compatible-descendant");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(62);
    let draft = draft_id(63);
    let replacement = draft_id(64);
    create_thread(&store, storage, thread, draft);
    save_text(&store, storage, thread, "pending", timestamp(2));
    let (turn, projected_path) = submit_root_turn(
        &store,
        storage,
        thread,
        draft,
        replacement,
        SyndicItemId::from_bytes([65; 16]),
        timestamp(3),
    );

    let cas_thread = CasThreadId::new("phase62-descendant-activation").unwrap();
    let valid = valid_request(thread, projected_path, cas_thread);
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), valid),
    );

    save_text(&store, storage, thread, "queued", timestamp(4));
    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.admit_accepted_input(
            storage.revision(&store).unwrap(),
            AcceptedInputAdmission::new(
                thread,
                current.thread().revision(),
                current.draft().id(),
                current.draft().revision(),
                current.draft().content(),
                gate.revision(),
                draft_id(66),
                None,
                timestamp(5),
            ),
        ),
    );

    let current = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let current_path = SelectedPathProof::new(
        current.thread().committed_tail(),
        current.thread().revision(),
        current.thread().selected_path_digest(),
    );
    assert!(current_path.is_compatible_descendant_of(projected_path));
    assert_ne!(
        current_path.thread_revision(),
        projected_path.thread_revision()
    );
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let activation = ActivateBinding::new(
        thread,
        BindingRevision::new(3).unwrap(),
        gate.revision(),
        current_path,
        SyndicExecutionSnapshotId::from_bytes([67; 16]),
        turn,
        loaded_generation(),
        timestamp(6),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activation.clone()),
    );
    assert_eq!(
        storage
            .binding_activation_status(&store, &activation, point_limit())
            .unwrap(),
        BindingPublicationStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

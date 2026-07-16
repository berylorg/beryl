use super::*;

#[test]
fn recovered_lineage_activation_requires_its_exact_loaded_generation() {
    let home = TestHome::new("phase9-recovered-loaded-generation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, parent, turn, selected) = non_root_pending(&store, storage);
    let parent = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.record().id()),
        selected.thread_revision(),
        parent.record().chain_digest(),
    );
    let required_generation = loaded_generation(7, 11);
    let recovered = RecoveredInjectionProof::new(
        RecoveryProjectionVersion::V1,
        represented,
        RecoveryItemSequenceDigest::from_bytes([12; 32]),
        RecoveryItemCount::new(1).unwrap(),
        RecoveryUtf8ByteCount::new(5).unwrap(),
        timestamp(6),
        required_generation,
    )
    .unwrap();
    let valid = valid_request(
        &store,
        storage,
        thread,
        selected,
        CasThreadId::new("recovered-generation-cas").unwrap(),
        represented,
        CasLineageProof::recovered(recovered),
    );
    publish_valid(&store, storage, valid);
    let established = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(established) = established.binding().state() else {
        panic!("recovered establishment is not valid");
    };
    assert_eq!(established.native_turn_count(), CasNativeTurnCount::ZERO);

    let snapshot = SyndicExecutionSnapshotId::from_bytes([13; 16]);
    let wrong = ActivateBinding::new(
        thread,
        current_binding_revision(&store, storage, thread),
        current_gate_revision(&store, storage, thread),
        selected,
        snapshot,
        turn,
        loaded_generation(7, 12),
        timestamp(8),
    );
    let error = execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), wrong),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingStateConflict
    ));
    assert!(
        storage
            .execution_snapshot(&store, snapshot, point_limit())
            .unwrap()
            .is_none()
    );

    let too_early = ActivateBinding::new(
        thread,
        current_binding_revision(&store, storage, thread),
        current_gate_revision(&store, storage, thread),
        selected,
        snapshot,
        turn,
        required_generation,
        timestamp(5),
    );
    let error = execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), too_early),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::TimestampRegressed
    ));
    assert!(
        storage
            .execution_snapshot(&store, snapshot, point_limit())
            .unwrap()
            .is_none()
    );

    let exact = ActivateBinding::new(
        thread,
        current_binding_revision(&store, storage, thread),
        current_gate_revision(&store, storage, thread),
        selected,
        snapshot,
        turn,
        required_generation,
        timestamp(8),
    );
    execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), exact),
    )
    .unwrap();
    store.validate_registered_domains().unwrap();

    let binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("recovered binding is not active");
    };
    let persisted_snapshot = storage
        .execution_snapshot(&store, snapshot, point_limit())
        .unwrap()
        .unwrap();
    let impossible_start = timestamp(5);
    let corrupt_active = ActiveCasBinding::new(
        active.usable().clone(),
        active.snapshot_id(),
        active.turn_id(),
        active.activation_gate_revision(),
        impossible_start,
    );
    let persisted_snapshot = persisted_snapshot.record();
    let corrupt_snapshot = ExecutionSnapshotRecord::new(
        persisted_snapshot.id(),
        persisted_snapshot.thread_id(),
        persisted_snapshot.binding_revision(),
        persisted_snapshot.activation_gate_revision(),
        persisted_snapshot.active_turn_id(),
        persisted_snapshot.cas_thread_id().clone(),
        persisted_snapshot.selected_path(),
        persisted_snapshot.represented_base_prefix(),
        persisted_snapshot.represented_base_native_turn_count(),
        persisted_snapshot.tool_profile(),
        persisted_snapshot.lineage(),
        persisted_snapshot.execution().clone(),
        persisted_snapshot.loaded_generation(),
        impossible_start,
    );
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::Binding(BindingRecord::new(
                binding.binding().thread_id(),
                binding.binding().revision(),
                binding.binding().selected_path(),
                BindingState::active(corrupt_active),
            )),
            FixtureRecord::ExecutionSnapshot(corrupt_snapshot),
        ]),
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("impossible recovered chronology registered successfully"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(
                source.to_string(),
                "recovered execution snapshot predates injection completion"
            );
        }
        other => panic!("expected recovered chronology rejection, got {other:?}"),
    }
    reopened.close().unwrap();
}

#[test]
fn recovered_cas_identity_cannot_be_redefined_as_native_lineage() {
    let home = TestHome::new("phase9-recovered-lineage-redefinition");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, parent, _, selected) = non_root_pending(&store, storage);
    let parent = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.record().id()),
        selected.thread_revision(),
        parent.record().chain_digest(),
    );
    let cas_thread = CasThreadId::new("recovered-cannot-become-native").unwrap();
    let recovered = RecoveredInjectionProof::new(
        RecoveryProjectionVersion::V1,
        represented,
        RecoveryItemSequenceDigest::from_bytes([44; 32]),
        RecoveryItemCount::new(1).unwrap(),
        RecoveryUtf8ByteCount::new(5).unwrap(),
        timestamp(6),
        loaded_generation(7, 11),
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
            cas_thread.clone(),
            represented,
            CasLineageProof::recovered(recovered),
        ),
    );
    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                selected,
                "reload proof required",
            )
            .unwrap(),
        ),
    )
    .unwrap();

    for (mechanism, cas_name) in [
        (NativeCasLineage::Continuation, "continuation"),
        (NativeCasLineage::Resume, "resume"),
        (NativeCasLineage::Fork, "fork"),
    ] {
        let request = valid_request(
            &store,
            storage,
            thread,
            selected,
            cas_thread.clone(),
            represented,
            CasLineageProof::native(mechanism, represented).unwrap(),
        );
        let error = execute(
            &store,
            storage.publish_valid_binding(storage.revision(&store).unwrap(), request),
        )
        .unwrap_err();
        assert!(
            matches!(
                typed_error(&error),
                SyndicMutationError::BindingPathConflict
            ),
            "{cas_name} redefined recovered lineage"
        );
    }
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn active_cas_turn_rejects_pre_start_time_and_reconciles_different_second_publication() {
    let home = TestHome::new("phase9-active-cas-turn-collisions");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn, selected) = root_pending(&store, storage);
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let cas_thread = CasThreadId::new("active-publication-cas").unwrap();
    let valid = valid_request(
        &store,
        storage,
        thread,
        selected,
        cas_thread.clone(),
        represented,
        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
    );
    publish_valid(&store, storage, valid);

    let snapshot = SyndicExecutionSnapshotId::from_bytes([14; 16]);
    let activate = ActivateBinding::new(
        thread,
        current_binding_revision(&store, storage, thread),
        current_gate_revision(&store, storage, thread),
        selected,
        snapshot,
        turn,
        loaded_generation(20, 30),
        timestamp(10),
    );
    execute(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), activate),
    )
    .unwrap();

    let cas_turn = CasTurnId::new("first-active-turn").unwrap();
    let regressed = PublishActiveCasTurn::new(
        thread,
        current_binding_revision(&store, storage, thread),
        current_gate_revision(&store, storage, thread),
        snapshot,
        cas_thread.clone(),
        cas_turn.clone(),
        timestamp(9),
    );
    let error = execute(
        &store,
        storage.publish_active_cas_turn(storage.revision(&store).unwrap(), regressed),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::TimestampRegressed
    ));

    let publication = PublishActiveCasTurn::new(
        thread,
        current_binding_revision(&store, storage, thread),
        current_gate_revision(&store, storage, thread),
        snapshot,
        cas_thread.clone(),
        cas_turn.clone(),
        timestamp(11),
    );
    execute(
        &store,
        storage.publish_active_cas_turn(storage.revision(&store).unwrap(), publication.clone()),
    )
    .unwrap();
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&store, &publication, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Exact
    );

    let different_turn = PublishActiveCasTurn::new(
        thread,
        publication.binding_revision(),
        publication.expected_gate_revision(),
        snapshot,
        cas_thread.clone(),
        CasTurnId::new("different-active-turn").unwrap(),
        timestamp(11),
    );
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&store, &different_turn, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Collision
    );
    let different_time = PublishActiveCasTurn::new(
        thread,
        publication.binding_revision(),
        publication.expected_gate_revision(),
        snapshot,
        cas_thread,
        cas_turn,
        timestamp(12),
    );
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&store, &different_time, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Collision
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

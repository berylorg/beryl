use super::*;

fn recovered_proof(
    represented: CasRepresentedPrefixProof,
    generation: CasLoadedSessionGeneration,
) -> RecoveredInjectionProof {
    RecoveredInjectionProof::new(
        RecoveryProjectionVersion::V1,
        represented,
        beryl_model::RecoveryItemSequenceDigest::from_bytes([12; 32]),
        RecoveryItemCount::new(1).unwrap(),
        RecoveryUtf8ByteCount::new(5).unwrap(),
        timestamp(6),
        generation,
    )
    .unwrap()
}

#[test]
fn recovered_lineage_activation_requires_its_process_and_preserves_chronology() {
    let home = TestHome::new("phase9-recovered-injection-process");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, Some(parent), turn, selected) = fault_pending_path(&store, storage, 170, true)
    else {
        unreachable!()
    };
    let parent = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let injection_generation = loaded_generation(7, 11);
    let handoff_generation = loaded_generation(7, 12);
    let recovered = recovered_proof(represented, injection_generation);
    let request = valid_request_with_count(
        &store,
        storage,
        thread,
        selected,
        CasThreadId::new("recovered-generation-cas").unwrap(),
        represented,
        CasNativeTurnCount::ZERO,
        CasLineageProof::recovered(recovered),
    );
    publish_valid(&store, storage, request);

    let snapshot = SyndicExecutionSnapshotId::from_bytes([173; 16]);
    let wrong_process = ActivateBinding::new(
        thread,
        current_binding_revision(&store, storage, thread),
        current_gate_revision(&store, storage, thread),
        selected,
        snapshot,
        turn,
        loaded_generation(8, 12),
        timestamp(8),
    );
    let outcome = execute_outcome(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), wrong_process),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::BindingStateConflict
    ));
    assert!(storage
        .execution_snapshot(&store, snapshot, point_limit())
        .unwrap()
        .is_none());

    let too_early = ActivateBinding::new(
        thread,
        current_binding_revision(&store, storage, thread),
        current_gate_revision(&store, storage, thread),
        selected,
        snapshot,
        turn,
        handoff_generation,
        timestamp(5),
    );
    let outcome = execute_outcome(
        &store,
        storage.activate_binding(storage.revision(&store).unwrap(), too_early),
    );
    assert!(matches!(
        typed_error(&outcome),
        SyndicMutationError::TimestampRegressed
    ));

    execute(
        &store,
        storage.activate_binding(
            storage.revision(&store).unwrap(),
            ActivateBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                current_gate_revision(&store, storage, thread),
                selected,
                snapshot,
                turn,
                handoff_generation,
                timestamp(8),
            ),
        ),
    );
    let binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("recovered binding did not activate")
    };
    assert_eq!(
        active.usable().lineage(),
        CasLineageProof::recovered(recovered)
    );
    let persisted = storage
        .execution_snapshot(&store, snapshot, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(persisted.loaded_generation(), handoff_generation);
    assert!(persisted.started_at() >= recovered.completed_at());
    store.close().unwrap();
}

#[test]
fn recovered_cas_identity_cannot_be_redefined_as_native_lineage() {
    let home = TestHome::new("phase9-recovered-lineage-redefinition");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, Some(parent), _, selected) = fault_pending_path(&store, storage, 180, true) else {
        unreachable!()
    };
    let parent = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let cas_thread = CasThreadId::new("recovered-cannot-become-native").unwrap();
    let recovered = recovered_proof(represented, loaded_generation(9, 11));
    publish_valid(
        &store,
        storage,
        valid_request_with_count(
            &store,
            storage,
            thread,
            selected,
            cas_thread.clone(),
            represented,
            CasNativeTurnCount::ZERO,
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
                "recovered injection must not become native",
            )
            .unwrap(),
        ),
    );

    for mechanism in [
        NativeCasLineage::Continuation,
        NativeCasLineage::Resume,
        NativeCasLineage::Fork,
    ] {
        let request = valid_request_with_count(
            &store,
            storage,
            thread,
            selected,
            cas_thread.clone(),
            represented,
            CasNativeTurnCount::ZERO,
            CasLineageProof::native(mechanism, represented).unwrap(),
        );
        let outcome = execute_outcome(
            &store,
            storage.publish_valid_binding(storage.revision(&store).unwrap(), request),
        );
        assert!(matches!(
            typed_error(&outcome),
            SyndicMutationError::BindingPathConflict
        ));
    }
    let current = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        current.binding().state(),
        BindingState::Unbound { .. }
    ));
    store.close().unwrap();
}

#[test]
fn active_cas_turn_rejects_pre_start_and_reconciles_exact_or_colliding_publication() {
    let home = TestHome::new("phase9-active-cas-turn-collisions");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, None, turn, selected) = fault_pending_path(&store, storage, 184, false) else {
        unreachable!()
    };
    let cas_thread = CasThreadId::new("active-publication-cas").unwrap();
    publish_valid(
        &store,
        storage,
        valid_request(&store, storage, thread, selected, cas_thread.clone()),
    );

    let snapshot = SyndicExecutionSnapshotId::from_bytes([187; 16]);
    execute(
        &store,
        storage.activate_binding(
            storage.revision(&store).unwrap(),
            ActivateBinding::new(
                thread,
                current_binding_revision(&store, storage, thread),
                current_gate_revision(&store, storage, thread),
                selected,
                snapshot,
                turn,
                loaded_generation(20, 30),
                timestamp(10),
            ),
        ),
    );

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
    let outcome = execute_outcome(
        &store,
        storage.publish_active_cas_turn(storage.revision(&store).unwrap(), regressed),
    );
    assert!(matches!(
        typed_error(&outcome),
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
    );
    assert_eq!(
        storage
            .active_cas_turn_publication_status(&store, &publication, point_limit())
            .unwrap(),
        ActiveCasTurnPublicationStatus::Exact
    );
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
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();
}

use super::*;

#[test]
fn misplaced_input_record_does_not_prove_a_different_requested_identity() {
    let home = support::TestHome::new("parent-execution-key-conflict");
    let (store, storage, mut request) = seeded(&home);
    activate(&store, &storage, &request);
    let input = storage
        .accepted_input(&store, request.input_id, limit())
        .unwrap()
        .unwrap();
    request.input_id = SyndicAcceptedInputId::from_bytes([248; 16]);
    support::commit(
        &store,
        storage.clone(),
        support::batch([
            syndic_storage::test_faults::FixtureRecord::AcceptedInputAtKey {
                key: request.input_id,
                input,
            },
        ]),
    );
    assert!(
        storage
            .prepare_discussion_parent_execution(&store, request)
            .is_err()
    );
}

#[test]
fn contradictory_terminal_event_cannot_authorize_archive() {
    let home = support::TestHome::new("parent-execution-source-conflict");
    let (store, storage, request) = seeded(&home);
    let source = activate(&store, &storage, &request);
    finish(
        &store,
        &storage,
        &request,
        &source,
        TurnEndStatus::complete(),
    );
    let state = storage
        .turn_state(&store, request.turn_id, limit())
        .unwrap()
        .unwrap();
    let sequence = SourceEventSequence::new(state.source_event_count()).unwrap();
    let proof = proven(&store, &storage, &request);
    let corrupt = SourceEventRecord::new(
        request.turn_id,
        sequence,
        Some(source),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
        ),
    )
    .unwrap();
    support::commit(
        &store,
        storage.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::SourceEvent(
            corrupt,
        )]),
    );
    assert!(
        storage
            .prepare_discussion_parent_execution(&store, request.clone())
            .is_err()
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            proof
                .into_terminal_settlement(support::timestamp(200))
                .unwrap()
                .contribution(),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
}

#[test]
fn contradictory_activation_tuple_never_proves_acceptance() {
    let home = support::TestHome::new("parent-execution-activation-conflict");
    let (store, storage, request) = seeded(&home);
    activate(&store, &storage, &request);
    let current = storage
        .current_binding(&store, request.parent_thread_id, limit())
        .unwrap()
        .unwrap();
    let binding = current.binding();
    let BindingState::Active(active) = binding.state() else {
        panic!("active binding")
    };
    let corrupt = BindingRecord::new(
        binding.thread_id(),
        binding.revision(),
        binding.selected_path(),
        BindingState::Active(ActiveCasBinding::new(
            active.usable().clone(),
            active.snapshot_id(),
            active.turn_id(),
            active.activation_gate_revision(),
            support::timestamp(102),
        )),
    );
    support::commit(
        &store,
        storage.clone(),
        support::batch([syndic_storage::test_faults::FixtureRecord::Binding(corrupt)]),
    );
    assert!(
        storage
            .prepare_discussion_parent_execution(&store, request)
            .is_err()
    );
}

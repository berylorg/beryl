use super::*;

#[test]
fn source_less_terminal_requires_a_retired_or_unbound_projection() {
    let home = TestHome::new("phase9-source-less-valid-parent");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let thread = id(50);
    let draft = draft_id(51);
    create_thread(&store, storage, thread, draft);
    save_text(&store, storage, thread, "not yet sent", timestamp(2));
    let (turn, selected) = submit_root_turn(
        &store,
        storage,
        thread,
        draft,
        draft_id(52),
        SyndicItemId::from_bytes([53; 16]),
        timestamp(3),
    );
    let valid = valid_request(
        thread,
        selected,
        CasThreadId::new("phase9-valid-parent-only").unwrap(),
    );
    execute(
        &store,
        storage.publish_valid_binding(storage.revision(&store).unwrap(), valid),
    );

    let state = storage
        .turn_state(&store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let local_terminal = LiveSourceEvent::new(
        thread,
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::FIRST,
        None,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(4),
    )
    .unwrap();
    let error = not_committed_error(execute_outcome(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), local_terminal.clone()),
    )
    ));
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::SourceIdentityConflict
    ));

    let binding = storage
        .current_binding(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                thread,
                binding.binding().revision(),
                selected,
                "local terminal has no CAS authority",
            )
            .unwrap(),
        ),
    );
    execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), local_terminal),
    );
    store.validate_registered_domains().unwrap();
    assert_eq!(
        storage
            .turn_state(&store, turn, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Interrupted
    );
    assert_eq!(
        storage
            .input_gate(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .state(),
        &InputGateState::FinalizingHistory(turn)
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.close().unwrap();
}

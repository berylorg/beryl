use super::*;

#[test]
fn every_proven_terminal_outcome_persists_its_exact_gate_semantics() {
    for (name, outcome, expected) in [
        (
            "complete",
            TurnTerminalOutcome::Complete,
            TurnLifecycle::Complete,
        ),
        (
            "interrupted",
            TurnTerminalOutcome::Interrupted,
            TurnLifecycle::Interrupted,
        ),
        ("failed", TurnTerminalOutcome::Failed, TurnLifecycle::Failed),
        (
            "incomplete",
            TurnTerminalOutcome::Incomplete,
            TurnLifecycle::Incomplete,
        ),
    ] {
        let home = TestHome::new(&format!("phase6-terminal-{name}"));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let (thread, turn) = seed_pending_turn(&store, storage);
        let source = establish_turn(&store, storage, thread, turn, timestamp(4));
        admit(
            &store,
            storage,
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(4),
        );
        admit(
            &store,
            storage,
            thread,
            turn,
            &source,
            SourceEventPayload::TurnEnded(
                TurnEndStatus::new(outcome, Some(TurnIncompleteReason::ItemAuditFailed)).unwrap(),
            ),
            timestamp(5),
        );
        let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
        assert_eq!(state.lifecycle(), expected);
        assert_eq!(state.terminal_outcome(), Some(outcome));
        assert_eq!(
            state.incomplete_reason(),
            Some(TurnIncompleteReason::ItemAuditFailed)
        );
        let gate = storage
            .input_gate(&store, thread, limit())
            .unwrap()
            .unwrap();
        assert_eq!(gate.state(), &InputGateState::FinalizingHistory(turn));
        store.validate_registered_domains().unwrap();
        store.close().unwrap();
    }
}

#[test]
fn active_sourced_unknown_terminal_enters_queue_only_wait_without_stop_authority() {
    let home = TestHome::new("phase6-active-unknown-terminal-awaiting");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, turn) = seed_pending_turn(&store, storage);
    let source = establish_turn(&store, storage, thread, turn, timestamp(4));

    admit(
        &store,
        storage,
        thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(4),
    );
    execute(
        &store,
        storage.admit_live_source_event(
            storage.revision(&store).unwrap(),
            next_event(
                &store,
                storage,
                thread,
                turn,
                &source,
                SourceEventPayload::TurnEnded(
                    TurnEndStatus::new(
                        TurnTerminalOutcome::UnknownTerminal,
                        Some(TurnIncompleteReason::ItemAuditFailed),
                    )
                    .unwrap(),
                ),
                timestamp(5),
            ),
        ),
    )
    .unwrap();

    let state = storage.turn_state(&store, turn, limit()).unwrap().unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::UnknownTerminal);
    assert_eq!(state.source_event_count(), 2);
    let gate = storage
        .input_gate(&store, thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::AwaitingTerminal(turn));
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.state().stop_operation_nonce(), None);
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

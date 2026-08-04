use super::*;

#[test]
fn a_live_event_cannot_mutate_another_threads_turn_or_gate() {
    let home = TestHome::new("phase6-cross-thread-rejection");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (first_thread, first_turn) = seed_pending_turn(&store, storage);
    let source = establish_turn(&store, storage, first_thread, first_turn, timestamp(4));
    let other_thread = id(40);
    execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                other_thread,
                draft_id(41),
                crate::support::exact_cas::execution_binding(),
                timestamp(4),
            ),
        ),
    )
    .unwrap();
    let state = storage
        .turn_state(&store, first_turn, limit())
        .unwrap()
        .unwrap();
    let other_gate = storage
        .input_gate(&store, other_thread, limit())
        .unwrap()
        .unwrap();
    let first_gate = storage
        .input_gate(&store, first_thread, limit())
        .unwrap()
        .unwrap();
    let mismatched = LiveSourceEvent::new(
        other_thread,
        first_turn,
        state.revision(),
        other_gate.revision(),
        SourceEventSequence::FIRST,
        Some(source),
        SourceEventPayload::TurnActivated,
        timestamp(5),
    )
    .unwrap();
    let error = execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), mismatched),
    )
    .unwrap_err();
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::LiveTurnConflict
    ));
    assert_eq!(
        storage
            .turn_state(&store, first_turn, limit())
            .unwrap()
            .unwrap(),
        state
    );
    assert_eq!(
        storage
            .input_gate(&store, other_thread, limit())
            .unwrap()
            .unwrap(),
        other_gate
    );
    assert_eq!(
        storage
            .input_gate(&store, first_thread, limit())
            .unwrap()
            .unwrap(),
        first_gate
    );
    assert_eq!(
        storage
            .turn(&store, first_turn, limit())
            .unwrap()
            .unwrap()
            .parent(),
        ConversationParent::Root
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

use super::*;

#[test]
fn reopen_rejects_terminal_native_count_that_did_not_advance_once() {
    let home = TestHome::new("phase10-terminal-native-count-corruption");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = activate_root_turn(&store, storage, false);
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.publish_active_cas_turn(
            storage.revision(&store).unwrap(),
            PublishActiveCasTurn::new(
                fixture.thread,
                binding.binding().revision(),
                gate.revision(),
                fixture.snapshot,
                fixture.cas_thread.clone(),
                fixture.cas_turn.clone(),
                timestamp(6),
            ),
        ),
    );
    execute(
        &store,
        storage.admit_live_source_event(
            storage.revision(&store).unwrap(),
            terminal_event(
                &store,
                storage,
                &fixture,
                Some(CasTurnSource::new(
                    fixture.cas_thread.clone(),
                    fixture.cas_turn.clone(),
                )),
                TurnTerminalOutcome::Complete,
                timestamp(7),
            ),
        ),
    );
    let current = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = current.binding().state() else {
        panic!("terminal fixture did not become valid");
    };
    assert_eq!(usable.native_turn_count().get(), 1);
    let corrupt = UsableCasBinding::new(
        usable.execution().clone(),
        usable.cas_thread_id().clone(),
        usable.represented_prefix(),
        beryl_model::CasNativeTurnCount::new(2),
        usable.tool_profile(),
        usable.lineage(),
    );
    commit(
        &store,
        storage,
        batch([FixtureRecord::Binding(BindingRecord::new(
            current.binding().thread_id(),
            current.binding().revision(),
            current.binding().selected_path(),
            BindingState::valid(corrupt),
        ))]),
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("corrupt terminal native count reopened successfully"),
        Err(error) => error,
    };
    let DomainRegistrationError::Validation { domain, source } = error else {
        panic!("expected terminal native-count validation failure, got {error:?}");
    };
    assert_eq!(domain, "syndic");
    assert_eq!(
        source.to_string(),
        "valid active successor lacks exact terminal CAS authority"
    );
    reopened.close().unwrap();
}

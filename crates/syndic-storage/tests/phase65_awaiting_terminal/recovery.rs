use super::*;

fn only_recovery_source(store: &HomeStore, storage: SyndicStorage) -> DeliveryRecoverySource {
    let page = storage
        .delivery_recovery_startup_page(store, None, cursor_limits())
        .unwrap();
    assert_eq!(page.records().len(), 1);
    page.records()[0].clone()
}

#[test]
fn restart_classifies_awaiting_terminal_as_active_possible_dispatch_and_abandons_it() {
    let fixture = active_fixture("phase65-awaiting-terminal-restart");
    let first = accept_text(&fixture, "before uncertainty", draft_id(43), 6);
    admit_unknown(&fixture, 8);
    let second = accept_text(&fixture, "during uncertainty", draft_id(44), 9);
    fixture.store.validate_registered_domains().unwrap();

    let ActiveFixture {
        _home: home,
        store,
        thread,
        turn,
        ..
    } = fixture;
    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let source = only_recovery_source(&reopened, storage);
    let DeliveryRecoveryCase::Active(active) = storage
        .classify_delivery_recovery(&reopened, &source, point_limit())
        .unwrap()
    else {
        panic!("awaiting-terminal startup source must classify as active");
    };
    assert_eq!(active.thread_id(), thread);
    assert_eq!(active.turn_id(), turn);
    let abandonment = active
        .generic_abandonment(
            "awaiting-terminal authority is not replayable after restart",
            active.minimum_timestamp(),
        )
        .unwrap();
    assert!(matches!(
        abandonment.target(),
        AcceptedRouteLostTarget::AwaitingTerminal(_)
    ));
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&reopened, &abandonment, point_limit(),)
            .unwrap(),
        BindingPublicationStatus::Prior
    );
    let AcceptedRouteLostTarget::AwaitingTerminal(retained_target) = abandonment.target() else {
        unreachable!("awaiting-terminal recovery produced another lost-target witness");
    };
    let wrong_target = AbandonActiveBinding::new(
        abandonment.thread_id(),
        abandonment.expected_binding_revision(),
        abandonment.route_generation(),
        AcceptedRouteLostTarget::Steering(retained_target.clone()),
        abandonment.selected_path(),
        abandonment.stale().clone(),
    );
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&reopened, &wrong_target, point_limit(),)
            .unwrap(),
        BindingPublicationStatus::Collision
    );
    execute(
        &reopened,
        storage.abandon_active_binding(storage.revision(&reopened).unwrap(), abandonment.clone()),
    )
    .unwrap();
    assert_eq!(
        storage
            .abandoned_active_binding_publication_status(&reopened, &abandonment, point_limit(),)
            .unwrap(),
        BindingPublicationStatus::Exact
    );

    let gate = storage
        .input_gate(&reopened, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::PendingTurn(turn));
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.live_next_turn_count(), 2);
    assert!(
        storage
            .accepted_input(&reopened, first, point_limit())
            .unwrap()
            .is_some()
    );
    assert!(
        storage
            .accepted_input(&reopened, second, point_limit())
            .unwrap()
            .is_some()
    );
    let recovered = only_recovery_source(&reopened, storage);
    assert!(matches!(
        storage.classify_delivery_recovery(&reopened, &recovered, point_limit()),
        Ok(DeliveryRecoveryCase::PostAbandonment {
            thread_id,
            turn_id,
            ..
        }) if thread_id == thread && turn_id == turn
    ));
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn restart_abandons_an_empty_retained_route_with_later_unknown_interval_work() {
    let fixture = active_fixture("phase65-awaiting-terminal-restart-empty-route");
    admit_unknown(&fixture, 8);
    let queued = accept_text(&fixture, "during uncertainty", draft_id(43), 9);
    fixture.store.validate_registered_domains().unwrap();

    let ActiveFixture {
        _home: home,
        store,
        thread,
        turn,
        ..
    } = fixture;
    store.close().unwrap();
    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let source = only_recovery_source(&reopened, storage);
    let DeliveryRecoveryCase::Active(active) = storage
        .classify_delivery_recovery(&reopened, &source, point_limit())
        .unwrap()
    else {
        panic!("empty awaiting-terminal route must remain active recovery authority");
    };
    let abandonment = active
        .generic_abandonment(
            "empty awaiting-terminal authority is not replayable after restart",
            active.minimum_timestamp(),
        )
        .unwrap();
    execute(
        &reopened,
        storage.abandon_active_binding(storage.revision(&reopened).unwrap(), abandonment),
    )
    .unwrap();

    let gate = storage
        .input_gate(&reopened, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::PendingTurn(turn));
    assert_eq!(gate.live_next_turn_count(), 1);
    assert!(
        storage
            .accepted_input(&reopened, queued, point_limit())
            .unwrap()
            .is_some()
    );
    let state = storage
        .turn_state(&reopened, turn, point_limit())
        .unwrap()
        .unwrap();
    let terminal = LiveSourceEvent::new(
        thread,
        turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        None,
        SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
            TurnIncompleteReason::AuthorityLost,
        )),
        timestamp(100),
    )
    .unwrap();
    execute(
        &reopened,
        storage.admit_live_source_event(storage.revision(&reopened).unwrap(), terminal),
    )
    .unwrap();
    converge_and_release_terminal_history(&reopened, storage, thread, turn);
    let gate = storage
        .input_gate(&reopened, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

fn assert_uncertain_transition_whole(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
) -> bool {
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let proof = gate.selected_route().unwrap();
    let page = storage
        .accepted_route_page(store, thread, proof.generation(), proof.revision(), None)
        .unwrap();
    assert_eq!(page.records().len(), 1);
    let sources = storage
        .accepted_next_source_page(
            store,
            storage.revision(store).unwrap(),
            None,
            cursor_limits(),
        )
        .unwrap();
    match state.lifecycle() {
        TurnLifecycle::Active => {
            assert_eq!(gate.state(), &InputGateState::Steerable(turn));
            assert_eq!(gate.live_steering_count(), 1);
            assert_eq!(gate.live_next_turn_count(), 0);
            assert_eq!(
                page.records()[0].effective_state(),
                AcceptedRouteEffectiveState::Ready
            );
            assert!(sources.records().is_empty());
            false
        }
        TurnLifecycle::UnknownTerminal => {
            assert_eq!(gate.state(), &InputGateState::AwaitingTerminal(turn));
            assert_eq!(gate.live_steering_count(), 0);
            assert_eq!(gate.live_next_turn_count(), 1);
            assert_eq!(
                page.records()[0].effective_state(),
                AcceptedRouteEffectiveState::NextTurn(NextTurnReason::UnknownTerminal)
            );
            assert_eq!(sources.records().len(), 1);
            true
        }
        lifecycle => panic!("transition cut left unsupported lifecycle {lifecycle:?}"),
    }
}

#[test]
fn uncertain_terminal_fault_cuts_recover_only_prior_or_exact_whole_states() {
    for (name, point, expected_exact) in [
        ("before", FaultPoint::BeforeCommit, Some(false)),
        ("ambiguous", FaultPoint::AfterCommitBeforePersist, None),
        ("persisted", FaultPoint::AfterPersist, Some(true)),
    ] {
        let faults = FaultController::new();
        let fixture = active_fixture_with_faults(
            &format!("phase65-awaiting-terminal-fault-{name}"),
            faults.clone(),
        );
        accept_text(&fixture, "atomic input", draft_id(43), 6);
        let event = unknown_event(&fixture, 8);
        faults.fail_next(point);
        assert!(
            fixture
                .store
                .execute_current(
                    fixture
                        .storage
                        .current_admit_live_source_event(event.clone()),
                )
                .is_err()
        );
        assert_eq!(fixture.store.health().state(), HomeHealthState::Verifying);
        fixture.store.verify_health().unwrap();
        let exact = assert_uncertain_transition_whole(
            &fixture.store,
            fixture.storage,
            fixture.thread,
            fixture.turn,
        );
        if let Some(expected) = expected_exact {
            assert_eq!(exact, expected);
        }
        fixture.store.validate_registered_domains().unwrap();

        let ActiveFixture {
            _home: home,
            store,
            thread,
            turn,
            ..
        } = fixture;
        store.close().unwrap();
        let mut reopened = open(home.path());
        let storage = SyndicStorage::register(&mut reopened).unwrap();
        assert_eq!(
            assert_uncertain_transition_whole(&reopened, storage, thread, turn),
            exact
        );
        reopened.validate_registered_domains().unwrap();
        reopened.close().unwrap();
    }
}

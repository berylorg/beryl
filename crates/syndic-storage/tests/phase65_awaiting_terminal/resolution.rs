use super::*;

#[test]
fn exact_late_terminal_enters_history_without_retargeting_unknown_interval_work() {
    let fixture = active_fixture("phase65-awaiting-terminal-late-terminal");
    let first = accept_text(&fixture, "before uncertainty", draft_id(43), 6);
    admit_unknown(&fixture, 8);
    let awaiting = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let retained = awaiting.selected_route().unwrap();
    let second = accept_text(&fixture, "during uncertainty", draft_id(44), 9);

    admit_event(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Incomplete,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(12),
    );
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.lifecycle(), TurnLifecycle::Incomplete);
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.state(),
        &InputGateState::FinalizingHistory(fixture.turn)
    );
    assert_eq!(gate.selected_route(), Some(retained));
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.live_next_turn_count(), 2);
    assert_eq!(gate.state().stop_operation_nonce(), None);
    let retained_page = route_page(&fixture, retained);
    assert_eq!(retained_page.records()[0].input().id(), first);
    assert_eq!(
        retained_page.records()[0].effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::UnknownTerminal)
    );
    let second_record = fixture
        .storage
        .accepted_input(&fixture.store, second, point_limit())
        .unwrap()
        .unwrap();
    assert_ne!(second_record.route_generation(), retained.generation());
    assert_eq!(next_sources(&fixture).len(), 2);
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn late_terminal_releases_unknown_interval_work_for_exact_promotion() {
    let fixture = active_fixture("phase65-awaiting-terminal-promotion");
    let first = accept_text(&fixture, "before uncertainty", draft_id(43), 6);
    admit_unknown(&fixture, 8);
    let retained = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap()
        .selected_route()
        .unwrap();
    let second = accept_text(&fixture, "during uncertainty", draft_id(44), 9);

    admit_event(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Incomplete,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(12),
    );
    converge_and_release_terminal_history(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        fixture.turn,
    );

    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    assert_eq!(gate.live_next_turn_count(), 2);
    let sources = next_sources(&fixture);
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].generation(), retained.generation());
    let candidate = fixture
        .storage
        .accepted_next_candidate_page(&fixture.store, sources[0], None, cursor_limits())
        .unwrap()
        .into_candidate()
        .expect("released unknown-terminal work must be promotable");
    assert_eq!(candidate.input_id(), first);
    assert_eq!(
        candidate.next_turn_reason(),
        NextTurnReason::UnknownTerminal
    );
    let promotion = PromoteAcceptedInput::new(
        candidate,
        SyndicTurnId::from_bytes([80; 16]),
        SyndicItemId::from_bytes([81; 16]),
        timestamp(13),
    );
    assert_clean(execute(
        &fixture.store,
        fixture.storage.promote_accepted_input(promotion.clone()),
    ));

    assert_eq!(
        fixture
            .storage
            .accepted_input_promotion_status(&fixture.store, &promotion, point_limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.state(),
        &InputGateState::PendingTurn(promotion.successor_turn_id())
    );
    assert_eq!(gate.live_next_turn_count(), 1);
    let second_record = fixture
        .storage
        .accepted_input(&fixture.store, second, point_limit())
        .unwrap()
        .unwrap();
    assert_ne!(second_record.route_generation(), retained.generation());
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn exact_terminal_reclassifies_ready_work_into_terminal_history_atomically() {
    let fixture = active_fixture("phase65-awaiting-terminal-direct-terminal");
    let input = accept_text(&fixture, "ready before terminal", draft_id(43), 6);

    admit_event(
        &fixture.store,
        &fixture.storage,
        fixture.thread,
        fixture.turn,
        &fixture.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Incomplete,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(8),
    );

    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        gate.state(),
        &InputGateState::FinalizingHistory(fixture.turn)
    );
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.live_next_turn_count(), 1);
    let page = route_page(&fixture, gate.selected_route().unwrap());
    assert_eq!(page.records().len(), 1);
    assert_eq!(page.records()[0].input().id(), input);
    assert_eq!(
        page.records()[0].effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::TerminalHistory)
    );
    assert_eq!(next_sources(&fixture).len(), 1);
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn uncertain_terminal_refuses_to_overtake_delivering_work() {
    let fixture = active_fixture("phase65-awaiting-terminal-delivering");
    let input = accept_text(&fixture, "already claimed", draft_id(43), 6);
    assert_clean(execute(
        &fixture.store,
        fixture.storage.begin_accepted_input_delivery(
            fixture.storage.revision(&fixture.store).unwrap(),
            BeginAcceptedInputDelivery::new(
                fixture.thread,
                input,
                AcceptedInputRevision::new(1).unwrap(),
                steering_target(&fixture),
            ),
        ),
    ));
    let state = fixture
        .storage
        .turn_state(&fixture.store, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let event = LiveSourceEvent::new(
        fixture.thread,
        fixture.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        Some(fixture.source.clone()),
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::UnknownTerminal,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(8),
    )
    .unwrap();
    let outcome = execute(
        &fixture.store,
        fixture
            .storage
            .admit_live_source_event(fixture.storage.revision(&fixture.store).unwrap(), event),
    );
    with_typed_error(outcome, |error| {
        assert!(matches!(
            error,
            SyndicMutationError::ActiveSteeringRouteConflict
                | SyndicMutationError::InputGateStateConflict
        ));
    });
    assert_eq!(
        fixture
            .storage
            .turn_state(&fixture.store, fixture.turn, point_limit())
            .unwrap()
            .unwrap(),
        state
    );
    assert_eq!(
        fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        gate
    );
    fixture
        .store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

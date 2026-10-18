use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{AcceptedInputRevision, InputGateRevision};
use syndic_storage::test_faults::{FixtureRecord, fixture_route_leaf_with_transition};
use syndic_storage::*;

use crate::support::{
    TestHome, converge_and_release_terminal_history, id, open,
    populated::{active_snapshot, active_turn, cas_thread, cas_turn, steering_input},
    seed_populated, timestamp,
};

fn abandonment_request(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
) -> AbandonActiveBinding {
    let binding = storage
        .current_binding(store, id(40), super::limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("prior-witness fixture binding is active");
    };
    let snapshot = storage
        .execution_snapshot(store, active_snapshot(), super::limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, id(40), super::limit())
        .unwrap()
        .unwrap();
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count()),
        Some(snapshot.loaded_generation()),
        " prior retry witness projection loss",
        timestamp(9),
    )
    .unwrap();
    AbandonActiveBinding::new(
        id(40),
        binding.binding().revision(),
        gate.selected_route().unwrap().generation(),
        AcceptedRouteLostTarget::Steering(SteeringTargetProof::new(
            PendingSteeringTargetProof::new(
                binding.binding().revision(),
                active_snapshot(),
                active_turn(),
                cas_thread(),
            ),
            cas_turn(),
        )),
        binding.binding().selected_path(),
        stale,
    )
}

fn route_entry(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    route: AcceptedRouteHeadProof,
) -> AcceptedRouteEntry {
    storage
        .accepted_route_page(store, id(40), route.generation(), route.revision(), None)
        .unwrap()
        .records()
        .iter()
        .find(|entry| entry.input().id() == steering_input())
        .expect("steering input remains permanent route history")
        .clone()
}

#[test]
fn prior_retry_witness_survives_projection_loss_terminal_release_promotion_and_reopen() {
    let home = TestHome::new("promotion-prior-delivery-witness");
    let mut store_candidate = open(home.path());
    let storage = SyndicStorage::register(&mut store_candidate).unwrap();
    let store = store_candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    let ready = storage
        .ready_steering_input(&store, steering_input(), super::limit())
        .unwrap()
        .expect("earliest accepted input is initially ready");
    let begin = BeginAcceptedInputDelivery::new(
        id(40),
        steering_input(),
        ready.accepted_input_revision(),
        ready.target().clone(),
    );
    assert!(matches!(
        store.execute_current(storage.current_begin_accepted_input_delivery(begin)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));

    let delivering = storage
        .delivering_steering_input(&store, steering_input(), super::limit())
        .unwrap()
        .expect("claimed accepted input is delivering");
    let retry = RetryAcceptedInputDelivery::new(
        id(40),
        steering_input(),
        delivering.accepted_input_revision(),
        delivering.target().clone(),
    );
    assert_eq!(
        storage
            .retry_accepted_input_delivery_status(&store, &retry, super::limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Prior
    );
    assert!(matches!(
        store.execute_current(storage.current_retry_accepted_input_delivery(retry.clone())),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .retry_accepted_input_delivery_status(&store, &retry, super::limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Exact
    );

    let transition_route = storage
        .input_gate(&store, id(40), super::limit())
        .unwrap()
        .unwrap()
        .selected_route()
        .unwrap();
    let transition = route_entry(&store, &storage, transition_route)
        .leaf()
        .last_transition()
        .expect("retry persists its exact transition witness");
    assert_eq!(transition.kind(), AcceptedRouteLeafTransitionKind::Retry);

    let abandonment = abandonment_request(&store, &storage);
    assert!(matches!(
        store.execute_current(storage.current_abandon_active_binding(abandonment)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .retry_accepted_input_delivery_status(&store, &retry, super::limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Exact
    );

    let state = storage
        .turn_state(&store, active_turn(), super::limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, id(40), super::limit())
        .unwrap()
        .unwrap();
    let terminal = LiveSourceEvent::new(
        id(40),
        active_turn(),
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        None,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(10),
    )
    .unwrap();
    assert!(matches!(
        store.execute_current(storage.current_admit_live_source_event(
            terminal.clone(),
            crate::support::fixture_activity(
                &store,
                &storage,
                terminal.thread_id(),
                terminal.turn_id()
            )
        )),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    converge_and_release_terminal_history(&store, storage.clone(), id(40), active_turn());
    let source_route = storage
        .input_gate(&store, id(40), super::limit())
        .unwrap()
        .unwrap()
        .selected_route()
        .unwrap();
    let source_entry = route_entry(&store, &storage, source_route);
    assert_eq!(
        source_entry.effective_state(),
        AcceptedRouteEffectiveState::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert_eq!(source_entry.leaf().last_transition(), Some(transition));
    assert_eq!(
        storage
            .retry_accepted_input_delivery_status(&store, &retry, super::limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Exact
    );

    let promotion = super::promotion(&store, &storage);
    assert_eq!(promotion.accepted_input_id(), steering_input());
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.promote_accepted_input(promotion.clone()))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let promoted_route = AcceptedRouteHeadProof::new(
        source_route.generation(),
        source_route.revision().checked_next().unwrap(),
    );
    let promoted_entry = route_entry(&store, &storage, promoted_route);
    assert_eq!(promoted_entry.leaf().last_transition(), Some(transition));
    assert!(promoted_entry.leaf().promotion().is_some());
    assert_eq!(
        storage
            .retry_accepted_input_delivery_status(&store, &retry, super::limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened_candidate = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened_candidate).unwrap();
    let reopened = reopened_candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    assert_eq!(
        reopened_storage
            .accepted_input_promotion_status(&reopened, &promotion, super::limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    assert_eq!(
        reopened_storage
            .retry_accepted_input_delivery_status(&reopened, &retry, super::limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Exact
    );
    let reopened_entry = route_entry(&reopened, &reopened_storage, promoted_route);
    assert_eq!(reopened_entry.leaf().last_transition(), Some(transition));
    assert!(reopened_entry.leaf().promotion().is_some());
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

fn retry_after_terminal_prefix() -> super::promotion_support::Fixture {
    let mut fixture = super::promotion_support::promotion_fixture_for_generations(
        95,
        id(95),
        &[super::accepted_next_support::GenerationSpec::new(
            1,
            2,
            NextTurnReason::TerminalHistory,
        )],
    );
    let generation = AcceptedRouteGeneration::FIRST;
    let revision = AcceptedRouteRevision::new(5).unwrap();
    for record in &mut fixture.records {
        match record {
            FixtureRecord::InputGate(gate) => {
                *gate = InputGateRecord::new(
                    gate.thread_id(),
                    InputGateRevision::new(8).unwrap(),
                    gate.state().clone(),
                    gate.accepted_high_water(),
                    gate.route_generation_high_water(),
                    gate.selected_route(),
                    gate.live_steering_count(),
                    gate.live_next_turn_count(),
                    gate.live_logical_utf8_bytes(),
                )
                .unwrap();
            }
            FixtureRecord::AcceptedRouteGeneration(route) => {
                *route = AcceptedRouteGenerationRecord::new(
                    route.thread_id(),
                    generation,
                    revision,
                    route.target().clone(),
                    route.first_ordinal(),
                    route.last_ordinal(),
                    route.input_count(),
                    route.ready_retryable_count(),
                    route.delivering_count(),
                    route.next_turn_count(),
                    route.terminal_count(),
                    route.live_logical_utf8_bytes(),
                    route.delivering_logical_utf8_bytes(),
                )
                .unwrap();
            }
            FixtureRecord::AcceptedNextSource(source) => {
                *source = AcceptedNextSourceRecord::new(
                    source.thread_id(),
                    generation,
                    revision,
                    source.first_ordinal(),
                    source.last_ordinal(),
                );
            }
            FixtureRecord::AcceptedRouteLeaf(leaf) if leaf.ordinal().get() == 2 => {
                *leaf = fixture_route_leaf_with_transition(
                    AcceptedRouteLeafRecord::new(
                        leaf.input_id(),
                        leaf.thread_id(),
                        generation,
                        leaf.ordinal(),
                        AcceptedInputRevision::new(3).unwrap(),
                        AcceptedRouteLeafState::Routed,
                        AcceptedInputLifecycle::Retryable,
                    ),
                    AcceptedRouteLeafTransitionProof::new(
                        InputGateRevision::new(7).unwrap(),
                        AcceptedRouteHeadProof::new(
                            generation,
                            AcceptedRouteRevision::new(4).unwrap(),
                        ),
                        AcceptedInputRevision::new(2).unwrap(),
                        AcceptedRouteLeafTransitionKind::Retry,
                    ),
                );
            }
            _ => {}
        }
    }
    fixture
}

#[test]
fn candidate_scan_reserves_maximal_promoted_rows_before_advancing() {
    let (_home, store, storage, fixture) =
        super::seeded_fixture("promoted-row-remainder", retry_after_terminal_prefix());
    let candidate = super::candidate(&store, &storage);
    assert_eq!(candidate.ordinal().get(), 2);
    let request = PromoteAcceptedInput::new(
        candidate,
        beryl_model::SyndicTurnId::from_bytes([120; 16]),
        beryl_model::SyndicItemId::from_bytes([121; 16]),
        timestamp(20),
    );
    assert!(matches!(
        super::execute_promotion(&store, &storage, request.clone()),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &request, super::limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    let turn = request.successor_turn_id();
    let source = crate::support::exact_cas::establish_turn(
        &store,
        storage.clone(),
        fixture.thread,
        turn,
        timestamp(21),
    );
    crate::support::exact_cas::admit_event(
        &store,
        storage.clone(),
        fixture.thread,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(22),
    );
    crate::support::exact_cas::correlate_user_item(
        &store,
        storage.clone(),
        fixture.thread,
        turn,
        request.successor_item_id(),
        &source,
        timestamp(23),
    );
    crate::support::exact_cas::admit_event(
        &store,
        storage.clone(),
        fixture.thread,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
        ),
        timestamp(24),
    );
    converge_and_release_terminal_history(&store, storage.clone(), fixture.thread, turn);
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.thread, super::limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::Idle);
    let route = syndic_storage::test_faults::accepted_route_generation(
        &store,
        storage.clone(),
        fixture.thread,
        AcceptedRouteGeneration::FIRST,
    )
    .unwrap();
    let entries = storage
        .accepted_route_page(
            &store,
            fixture.thread,
            route.generation(),
            route.revision(),
            None,
        )
        .unwrap();
    assert_eq!(entries.records().len(), 3);
    let promoted = entries.records()[1].leaf();
    assert_eq!(promoted.input_id(), request.accepted_input_id());
    assert_eq!(promoted.state(), AcceptedRouteLeafState::Routed);
    assert_eq!(promoted.lifecycle(), AcceptedInputLifecycle::Promoted);
    assert_eq!(
        promoted.last_transition().unwrap().kind(),
        AcceptedRouteLeafTransitionKind::Retry
    );
    assert!(promoted.promotion().is_some());
    let limits = beryl_home_store::CursorReadLimits::new(256, 65_536).unwrap();
    let source_page = storage
        .accepted_next_source_page(&store, storage.revision(&store).unwrap(), None, limits)
        .unwrap();
    assert_eq!(source_page.records().len(), 1);
    let source = source_page.records()[0];
    assert_eq!(source.generation(), AcceptedRouteGeneration::FIRST);
    assert!(source_page.next_cursor().is_none());
    let too_small = beryl_home_store::CursorReadLimits::new(256, 189).unwrap();
    assert!(matches!(
        storage.accepted_next_candidate_page(&store, source, None, too_small),
        Err(SyndicReadError::Read(
            beryl_home_store::ReadError::BoundExceeded {
                family: "accepted-route-leaves",
                maximum: 112,
                actual: 113,
                ..
            }
        ))
    ));
    let revision_before = storage.revision(&store).unwrap();
    for remainder in [261, 262] {
        let max_bytes = 190 + remainder;
        let limits = beryl_home_store::CursorReadLimits::new(256, max_bytes).unwrap();
        let first = storage
            .accepted_next_candidate_page(&store, source, None, limits)
            .unwrap();
        assert!(first.candidate().is_none());
        assert_eq!(
            first.stored_bytes(),
            if remainder == 261 { 190 } else { 452 }
        );
        assert!(first.decoded_bytes() <= max_bytes);
        let mut cursor = first
            .next_cursor()
            .expect("terminal prefix has a continuation");
        let mut stored = first.stored_bytes();
        if remainder == 261 {
            let promoted = storage
                .accepted_next_candidate_page(&store, source, Some(cursor), limits)
                .unwrap();
            assert!(promoted.candidate().is_none());
            assert_eq!(promoted.stored_bytes(), 262);
            assert!(promoted.decoded_bytes() <= max_bytes);
            let next = promoted.next_cursor().expect("later candidate remains");
            assert_ne!(next, cursor);
            cursor = next;
            stored += promoted.stored_bytes();
        }
        assert_eq!(stored, 452);
        let last = storage
            .accepted_next_candidate_page(&store, source, Some(cursor), limits)
            .unwrap();
        let candidate = last
            .candidate()
            .expect("third input is the first live candidate");
        assert_eq!(candidate.thread_id(), fixture.thread);
        assert_eq!(candidate.input_id(), entries.records()[2].input().id());
        assert_eq!(candidate.ordinal().get(), 3);
        assert_eq!(
            candidate.next_turn_reason(),
            NextTurnReason::TerminalHistory
        );
        assert_eq!(last.stored_bytes(), 158);
        assert!(last.decoded_bytes() <= max_bytes);
        assert!(last.next_cursor().is_none());
    }
    assert_eq!(storage.revision(&store).unwrap(), revision_before);
    store.close().unwrap();
}

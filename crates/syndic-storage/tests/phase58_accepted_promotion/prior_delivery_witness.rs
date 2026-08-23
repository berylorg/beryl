use beryl_home_store::{CommandOutcome, HomeCommand};
use syndic_storage::*;

use crate::support::{
    TestHome, converge_and_release_terminal_history, id, open,
    populated::{active_snapshot, active_turn, cas_thread, cas_turn, steering_input},
    seed_populated, timestamp,
};

fn abandonment_request(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
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
        "phase58 prior retry witness projection loss",
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
    storage: SyndicStorage,
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
    let home = TestHome::new("phase58-promotion-prior-delivery-witness");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
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
    let transition = route_entry(&store, storage, transition_route)
        .leaf()
        .last_transition()
        .expect("retry persists its exact transition witness");
    assert_eq!(transition.kind(), AcceptedRouteLeafTransitionKind::Retry);

    let abandonment = abandonment_request(&store, storage);
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
        store.execute_current(storage.current_admit_live_source_event(terminal)),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    converge_and_release_terminal_history(&store, storage, id(40), active_turn());
    let source_route = storage
        .input_gate(&store, id(40), super::limit())
        .unwrap()
        .unwrap()
        .selected_route()
        .unwrap();
    let source_entry = route_entry(&store, storage, source_route);
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

    let promotion = super::promotion(&store, storage);
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
    let promoted_entry = route_entry(&store, storage, promoted_route);
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

    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
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
    let reopened_entry = route_entry(&reopened, reopened_storage, promoted_route);
    assert_eq!(reopened_entry.leaf().last_transition(), Some(transition));
    assert!(reopened_entry.leaf().promotion().is_some());
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

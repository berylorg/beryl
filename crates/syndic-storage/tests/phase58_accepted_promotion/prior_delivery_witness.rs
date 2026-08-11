use beryl_home_store::{CommandOutcome, HomeCommand};
use syndic_storage::*;

use crate::support::{
    TestHome, batch, commit, converge_and_release_terminal_history, id, open,
    phase11::{abandonment_request, mixed_abandonment_records},
    populated::{active_turn, steering_input},
    timestamp,
};

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
fn terminal_history_release_preserves_prior_retry_witness_across_reopen() {
    let home = TestHome::new("phase58-promotion-prior-delivery-witness");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(mixed_abandonment_records()));
    store.validate_registered_domains().unwrap();

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
    match store.execute_current(storage.current_begin_accepted_input_delivery(begin)) {
        CommandOutcome::Committed {
            later_failure: None, ..
        } => {}
        outcome => panic!("expected delivery begin to commit without later failure, got {outcome:?}"),
    }

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
    match store.execute_current(storage.current_retry_accepted_input_delivery(retry.clone())) {
        CommandOutcome::Committed {
            later_failure: None, ..
        } => {}
        outcome => panic!("expected delivery retry to commit without later failure, got {outcome:?}"),
    }
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
    assert_eq!(
        transition.expected_input_revision(),
        retry.expected_input_revision()
    );

    let abandonment = abandonment_request(&store, storage);
    match store.execute_current(storage.current_abandon_active_binding(abandonment)) {
        CommandOutcome::Committed {
            later_failure: None, ..
        } => {}
        outcome => panic!("expected binding abandonment to commit without later failure, got {outcome:?}"),
    }
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
    match store.execute_current(storage.current_admit_live_source_event(terminal)) {
        CommandOutcome::Committed {
            later_failure: None, ..
        } => {}
        outcome => panic!("expected terminal event to commit without later failure, got {outcome:?}"),
    }

    let finalizing_gate = storage
        .input_gate(&store, id(40), super::limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        finalizing_gate.state(),
        &InputGateState::FinalizingHistory(active_turn())
    );
    let stale_transcript = storage
        .transcript_view_head(&store, id(40), super::limit())
        .unwrap()
        .unwrap();
    assert_eq!(stale_transcript.lifecycle(), ProjectionLifecycle::Stale);
    converge_and_release_terminal_history(&store, storage, id(40), active_turn());
    let idle_gate = storage
        .input_gate(&store, id(40), super::limit())
        .unwrap()
        .unwrap();
    assert_eq!(idle_gate.state(), &InputGateState::Idle);
    let source_route = idle_gate
        .selected_route()
        .expect("projection-lost route remains selected before promotion");
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
    store.validate_registered_domains().unwrap();

    let promotion = super::promotion(&store, storage);
    assert_eq!(promotion.accepted_input_id(), steering_input());
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, super::limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Prior
    );
    let promoted_route = AcceptedRouteHeadProof::new(
        source_route.generation(),
        source_route.revision().checked_next().unwrap(),
    );
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.promote_accepted_input(promotion.clone()))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None, ..
        } => {}
        outcome => panic!("expected promotion to commit without later failure, got {outcome:?}"),
    }

    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, super::limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    assert_eq!(
        storage
            .retry_accepted_input_delivery_status(&store, &retry, super::limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Exact
    );
    let promoted_entry = route_entry(&store, storage, promoted_route);
    assert_eq!(
        promoted_entry.effective_state(),
        AcceptedRouteEffectiveState::Promoted
    );
    assert_eq!(promoted_entry.leaf().last_transition(), Some(transition));
    assert!(promoted_entry.leaf().promotion().is_some());
    store.validate_registered_domains().unwrap();
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
    assert_eq!(
        reopened_entry.effective_state(),
        AcceptedRouteEffectiveState::Promoted
    );
    assert_eq!(reopened_entry.leaf().last_transition(), Some(transition));
    assert!(reopened_entry.leaf().promotion().is_some());
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

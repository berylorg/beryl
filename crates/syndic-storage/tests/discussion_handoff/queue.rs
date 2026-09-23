use super::*;
#[path = "edit_gates.rs"]
mod edit_gates;
#[path = "../draft_edit_history_retention/common.rs"]
mod edit_support;
#[path = "mutation_gates.rs"]
mod mutation_gates;
#[path = "../draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

use crate::support::discussion_input::{accept_next, committed, prepare_acceptance};

fn blocked(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    let outcome = store.execute(command);
    assert!(
        matches!(outcome, CommandOutcome::NotCommitted { .. }),
        "{outcome:?}"
    );
    assert!(
        format!("{outcome:?}").contains("DiscussionMutationBlocked"),
        "{outcome:?}"
    );
}

#[test]
fn queued_future_input_fences_prepared_and_fresh_handoff_admission() {
    let home = TestHome::new("handoff-queue-race");
    let (store, storage, mut request) = seeded(&home, FaultController::new());
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request.clone()))
        .unwrap();
    let gate = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    let waiting = InputGateRecord::new(
        id(36),
        gate.revision().checked_next().unwrap(),
        InputGateState::AwaitingTerminal(request.resolving_target.pending().active_turn_id()),
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        0,
        0,
        0,
    )
    .unwrap();
    crate::support::commit(
        &store,
        storage.clone(),
        crate::support::batch([syndic_storage::test_faults::FixtureRecord::InputGate(
            waiting,
        )]),
    );
    accept_next(&store, &storage);
    let queued = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    assert_eq!(queued.live_next_turn_count(), 1);
    assert!(matches!(
        store.execute(command(&store, prepared)),
        CommandOutcome::NotCommitted { .. }
    ));
    let restored = InputGateRecord::new(
        id(36),
        queued.revision().checked_next().unwrap(),
        gate.state().clone(),
        queued.accepted_high_water(),
        queued.route_generation_high_water(),
        queued.selected_route(),
        queued.live_steering_count(),
        queued.live_next_turn_count(),
        queued.live_logical_utf8_bytes(),
    )
    .unwrap();
    request.input_gate_revision = restored.revision();
    request.thread_revision = storage
        .thread(&store, id(36), limit())
        .unwrap()
        .unwrap()
        .revision();
    crate::support::commit(
        &store,
        storage.clone(),
        crate::support::batch([syndic_storage::test_faults::FixtureRecord::InputGate(
            restored,
        )]),
    );
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    assert!(matches!(
        store.execute(command(&store, prepared)),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        storage
            .discussion_handoff_gate(&store, id(36), limit())
            .unwrap()
            .unwrap()
            .state(),
        DiscussionHandoffGateState::Open
    );
    store.close().unwrap();
}

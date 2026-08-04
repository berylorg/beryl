use syndic_storage::*;

use crate::{
    accepted_support::{AcceptedOperation, assert_operation_committed, limit, route_entry, seeded},
    support::{id, open},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Aggregates {
    ready: u64,
    delivering: u64,
    next: u64,
    terminal: u64,
    gate_steering: u64,
    gate_next: u64,
    gate_live_bytes: u64,
}

#[test]
fn every_delivery_transition_executes_current_and_persists_exact_aggregates() {
    for operation in AcceptedOperation::ALL {
        let name = format!("phase53-current-{}", operation.name());
        let (home, store, storage) = seeded(&name, operation.records());
        assert_eq!(
            operation.status(&store, storage),
            AcceptedInputDeliveryTransitionStatus::Prior
        );

        store
            .execute_current(operation.current_command(storage))
            .unwrap();

        assert_eq!(
            operation.status(&store, storage),
            AcceptedInputDeliveryTransitionStatus::Exact
        );
        assert_operation_committed(&store, storage, operation);
        assert_eq!(aggregates(&store, storage), expected_aggregates(operation));
        let (_, entry) = route_entry(&store, storage, operation.input());
        assert_eq!(
            entry.leaf().last_transition(),
            Some(AcceptedRouteLeafTransitionProof::new(
                operation.expected_gate_revision(),
                operation.expected_route(),
                operation.expected_input_revision(),
                operation.expected_transition_kind(),
            ))
        );
        store.close().unwrap();

        let mut reopened = open(home.path());
        let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
        assert_eq!(
            operation.status(&reopened, reopened_storage),
            AcceptedInputDeliveryTransitionStatus::Exact
        );
        assert_operation_committed(&reopened, reopened_storage, operation);
        assert_eq!(
            aggregates(&reopened, reopened_storage),
            expected_aggregates(operation)
        );
        reopened.close().unwrap();
    }
}

fn aggregates(store: &beryl_home_store::HomeStore, storage: SyndicStorage) -> Aggregates {
    let gate = storage.input_gate(store, id(40), limit()).unwrap().unwrap();
    let proof = gate.selected_route().unwrap();
    let page = storage
        .accepted_route_page(store, id(40), proof.generation(), proof.revision(), None)
        .unwrap();
    let mut ready = 0;
    let mut delivering = 0;
    let mut next = 0;
    let mut terminal = 0;
    for entry in page.records() {
        match entry.effective_state() {
            AcceptedRouteEffectiveState::Ready => ready += 1,
            AcceptedRouteEffectiveState::Delivering => delivering += 1,
            AcceptedRouteEffectiveState::NextTurn(_) => next += 1,
            AcceptedRouteEffectiveState::Delivered
            | AcceptedRouteEffectiveState::Failed
            | AcceptedRouteEffectiveState::Promoted
            | AcceptedRouteEffectiveState::DeliveryUnknown => terminal += 1,
        }
    }
    Aggregates {
        ready,
        delivering,
        next,
        terminal,
        gate_steering: gate.live_steering_count(),
        gate_next: gate.live_next_turn_count(),
        gate_live_bytes: gate.live_logical_utf8_bytes(),
    }
}

fn expected_aggregates(operation: AcceptedOperation) -> Aggregates {
    match operation {
        AcceptedOperation::Begin => Aggregates {
            ready: 0,
            delivering: 1,
            next: 1,
            terminal: 0,
            gate_steering: 1,
            gate_next: 1,
            gate_live_bytes: 0,
        },
        AcceptedOperation::Retry => Aggregates {
            ready: 3,
            delivering: 0,
            next: 1,
            terminal: 0,
            gate_steering: 3,
            gate_next: 1,
            gate_live_bytes: 19,
        },
        AcceptedOperation::Complete => Aggregates {
            ready: 2,
            delivering: 0,
            next: 1,
            terminal: 1,
            gate_steering: 2,
            gate_next: 1,
            gate_live_bytes: 0,
        },
        AcceptedOperation::Reject => Aggregates {
            ready: 2,
            delivering: 0,
            next: 2,
            terminal: 0,
            gate_steering: 2,
            gate_next: 2,
            gate_live_bytes: 19,
        },
    }
}

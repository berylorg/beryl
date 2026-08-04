use beryl_model::{AcceptedInputRevision, CasTurnId, InputGateRevision};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use crate::{
    accepted_support::{AcceptedOperation, limit, seeded},
    support::{
        batch, commit, id, open,
        phase11::{
            abandonment_request, delivering_input, mixed_abandonment_records, retryable_input,
        },
        populated::{populated_records, steering_input},
    },
};

fn route(revision: u64) -> AcceptedRouteHeadProof {
    AcceptedRouteHeadProof::new(
        AcceptedRouteGeneration::FIRST,
        AcceptedRouteRevision::new(revision).unwrap(),
    )
}

fn wrong_target(operation: AcceptedOperation) -> SteeringTargetProof {
    SteeringTargetProof::new(
        operation.target().pending().clone(),
        CasTurnId::new("different-target").unwrap(),
    )
}

#[test]
fn every_transition_specific_status_rejects_target_drift_as_collision() {
    for operation in AcceptedOperation::ALL {
        let name = format!("phase53-{}-collision", operation.name());
        let (_home, store, storage) = seeded(&name, operation.records());
        let status = match operation {
            AcceptedOperation::Begin => storage.begin_accepted_input_delivery_status(
                &store,
                &BeginAcceptedInputDelivery::new(
                    operation.thread(),
                    operation.input(),
                    operation.expected_input_revision(),
                    wrong_target(operation),
                ),
                limit(),
            ),
            AcceptedOperation::Retry => storage.retry_accepted_input_delivery_status(
                &store,
                &RetryAcceptedInputDelivery::new(
                    operation.thread(),
                    operation.input(),
                    operation.expected_input_revision(),
                    wrong_target(operation),
                ),
                limit(),
            ),
            AcceptedOperation::Complete => storage.complete_accepted_input_delivery_status(
                &store,
                &CompleteAcceptedInputDelivery::new(
                    operation.thread(),
                    operation.input(),
                    operation.expected_input_revision(),
                    wrong_target(operation),
                ),
                limit(),
            ),
            AcceptedOperation::Reject => storage.steering_rejection_status(
                &store,
                &SteeringRejection::new(
                    operation.thread(),
                    operation.input(),
                    operation.expected_input_revision(),
                    wrong_target(operation),
                ),
                limit(),
            ),
        }
        .unwrap();
        assert_eq!(
            status,
            AcceptedInputDeliveryTransitionStatus::Collision,
            "{} must reject a different steering target",
            operation.name(),
        );
        store.close().unwrap();
    }
}

#[derive(Clone, Copy)]
enum WitnessCorruption {
    FutureGate,
    FutureRoute,
}

#[test]
fn impossible_future_authority_in_transition_witness_fails_validation_and_reopen() {
    for (name, corruption) in [
        ("phase53-future-gate-witness", WitnessCorruption::FutureGate),
        (
            "phase53-future-route-witness",
            WitnessCorruption::FutureRoute,
        ),
    ] {
        assert_witness_corruption_rejected(name, corruption);
    }
}

fn assert_witness_corruption_rejected(name: &str, corruption: WitnessCorruption) {
    let records = populated_records();
    let initial_generation = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedRouteGeneration(generation)
                if generation.thread_id() == id(40) =>
            {
                Some(generation.clone())
            }
            _ => None,
        })
        .unwrap();
    let initial_next_source = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedNextSource(source) if source.thread_id() == id(40) => {
                Some(*source)
            }
            _ => None,
        })
        .unwrap();
    let claimed_bytes = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedInput(input) if input.id() == steering_input() => {
                Some(input.content().summary().logical_utf8_bytes())
            }
            _ => None,
        })
        .unwrap();
    let (home, store, storage) = seeded(name, records);
    let request = BeginAcceptedInputDelivery::new(
        id(40),
        steering_input(),
        AcceptedInputRevision::new(1).unwrap(),
        AcceptedOperation::Begin.target(),
    );
    store
        .execute_current(storage.current_begin_accepted_input_delivery(request.clone()))
        .unwrap();
    let gate = storage
        .input_gate(&store, id(40), limit())
        .unwrap()
        .unwrap();

    let selected_route = match corruption {
        WitnessCorruption::FutureGate => gate.selected_route().unwrap(),
        WitnessCorruption::FutureRoute => route(1),
    };
    let gate_revision = match corruption {
        WitnessCorruption::FutureGate => InputGateRevision::new(3).unwrap(),
        WitnessCorruption::FutureRoute => gate.revision(),
    };
    let corrupted_gate = InputGateRecord::new(
        gate.thread_id(),
        gate_revision,
        gate.state().clone(),
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        Some(selected_route),
        gate.live_steering_count(),
        gate.live_next_turn_count(),
        gate.live_logical_utf8_bytes(),
    )
    .unwrap();
    let mut corruptions = vec![FixtureRecord::InputGate(corrupted_gate)];
    if matches!(corruption, WitnessCorruption::FutureRoute) {
        corruptions.extend([
            FixtureRecord::AcceptedRouteGenerationHead(AcceptedRouteGenerationHeadRecord::new(
                id(40),
                route(1),
            )),
            FixtureRecord::AcceptedRouteGeneration(
                AcceptedRouteGenerationRecord::new(
                    initial_generation.thread_id(),
                    initial_generation.generation(),
                    route(1).revision(),
                    initial_generation.target().clone(),
                    initial_generation.first_ordinal(),
                    initial_generation.last_ordinal(),
                    initial_generation.input_count(),
                    initial_generation.ready_retryable_count() - 1,
                    initial_generation.delivering_count() + 1,
                    initial_generation.next_turn_count(),
                    initial_generation.terminal_count(),
                    initial_generation.live_logical_utf8_bytes(),
                    initial_generation.delivering_logical_utf8_bytes() + claimed_bytes,
                )
                .unwrap(),
            ),
            FixtureRecord::AcceptedNextSource(initial_next_source),
        ]);
    }
    commit(&store, storage, batch(corruptions));

    let error = store.validate_registered_domains().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("accepted-route leaf transition proof disagrees")
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("impossible transition witness survived reopen validation"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("accepted-route leaf transition proof disagrees")
    );
    reopened.close().unwrap();
}

#[test]
fn target_and_leaf_revision_drift_classify_collision() {
    let (_home, store, storage) =
        seeded("phase53-reconciliation-drift", mixed_abandonment_records());
    let exact_prior = BeginAcceptedInputDelivery::new(
        id(40),
        steering_input(),
        AcceptedInputRevision::new(1).unwrap(),
        AcceptedOperation::Begin.target(),
    );
    let wrong_target = BeginAcceptedInputDelivery::new(
        id(40),
        steering_input(),
        AcceptedInputRevision::new(1).unwrap(),
        wrong_target(AcceptedOperation::Begin),
    );
    let wrong_leaf_revision = BeginAcceptedInputDelivery::new(
        id(40),
        steering_input(),
        AcceptedInputRevision::new(2).unwrap(),
        AcceptedOperation::Begin.target(),
    );

    assert_eq!(
        storage
            .begin_accepted_input_delivery_status(&store, &exact_prior, limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Prior
    );
    for request in [&wrong_target, &wrong_leaf_revision] {
        assert_eq!(
            storage
                .begin_accepted_input_delivery_status(&store, request, limit())
                .unwrap(),
            AcceptedInputDeliveryTransitionStatus::Collision
        );
    }
    assert!(
        store
            .execute_current(storage.current_begin_accepted_input_delivery(wrong_target))
            .is_err()
    );

    let abandonment = abandonment_request(&store, storage);
    store
        .execute_current(storage.current_abandon_active_binding(abandonment))
        .unwrap();
    assert_eq!(
        storage
            .begin_accepted_input_delivery_status(&store, &exact_prior, limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Collision,
        "projection-loss target drift must not preserve Prior",
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn same_stable_intent_survives_unrelated_aggregate_advancement() {
    let (_home, store, storage) = seeded(
        "phase53-reconciliation-false-exact",
        mixed_abandonment_records(),
    );
    let request_a = BeginAcceptedInputDelivery::new(
        id(40),
        retryable_input(),
        AcceptedInputRevision::new(2).unwrap(),
        AcceptedOperation::Begin.target(),
    );
    let unrelated = BeginAcceptedInputDelivery::new(
        id(40),
        steering_input(),
        AcceptedInputRevision::new(1).unwrap(),
        AcceptedOperation::Begin.target(),
    );
    store
        .execute_current(storage.current_begin_accepted_input_delivery(unrelated))
        .unwrap();
    assert_eq!(
        storage
            .begin_accepted_input_delivery_status(&store, &request_a, limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Prior,
        "same-generation aggregate advancement must preserve the stable source intent",
    );

    let request_b = BeginAcceptedInputDelivery::new(
        id(40),
        retryable_input(),
        AcceptedInputRevision::new(2).unwrap(),
        AcceptedOperation::Begin.target(),
    );
    store
        .execute_current(storage.current_begin_accepted_input_delivery(request_b.clone()))
        .unwrap();
    assert_eq!(
        storage
            .begin_accepted_input_delivery_status(&store, &request_b, limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Exact
    );
    assert_eq!(
        storage
            .begin_accepted_input_delivery_status(&store, &request_a, limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Exact,
        "equivalent stable intents must authenticate the same successor witness",
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#[test]
fn exact_leaf_witness_survives_unrelated_later_route_and_gate_work() {
    let (_home, store, storage) = seeded(
        "phase53-reconciliation-later-route-work",
        mixed_abandonment_records(),
    );
    let claimed = BeginAcceptedInputDelivery::new(
        id(40),
        retryable_input(),
        AcceptedInputRevision::new(2).unwrap(),
        AcceptedOperation::Begin.target(),
    );
    store
        .execute_current(storage.current_begin_accepted_input_delivery(claimed.clone()))
        .unwrap();
    let unrelated_retry = RetryAcceptedInputDelivery::new(
        id(40),
        delivering_input(),
        AcceptedInputRevision::new(2).unwrap(),
        AcceptedOperation::Retry.target(),
    );
    store
        .execute_current(storage.current_retry_accepted_input_delivery(unrelated_retry))
        .unwrap();

    assert_eq!(
        storage
            .begin_accepted_input_delivery_status(&store, &claimed, limit())
            .unwrap(),
        AcceptedInputDeliveryTransitionStatus::Exact
    );
    store.validate_registered_domains().unwrap();
    store.close().unwrap();
}

#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CursorReadLimits, DomainRegistrationError, HomeCommand, HomeStore};
use beryl_model::{AcceptedInputRevision, InputGateRevision, SyndicItemId};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use support::{
    TestHome, batch, commit, id, open,
    phase11::{
        DELIVERY_UNKNOWN_LOGICAL_BYTES, abandonment_request, delivering_input,
        mixed_abandonment_records, retryable_input,
    },
    populated::{active_turn, cas_thread, next_input, populated_records, steering_input},
};

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn page_limit() -> CursorReadLimits {
    CursorReadLimits::new(16, 1_000_000).unwrap()
}

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn delivery_request(
    store: &HomeStore,
    storage: SyndicStorage,
) -> (InputGateRevision, AcceptedInputRevision) {
    let gate = storage
        .input_gate(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    let input = storage
        .accepted_input(store, steering_input(), point_limit())
        .unwrap()
        .unwrap();
    (gate.record().revision(), input.record().revision())
}

fn begin(store: &HomeStore, storage: SyndicStorage) {
    let (gate, input) = delivery_request(store, storage);
    execute(
        store,
        storage.begin_accepted_input_delivery(
            storage.revision(store).unwrap(),
            BeginAcceptedInputDelivery::new(id(40), gate, steering_input(), input),
        ),
    );
}

fn retry(store: &HomeStore, storage: SyndicStorage) {
    let (gate, input) = delivery_request(store, storage);
    execute(
        store,
        storage.retry_accepted_input_delivery(
            storage.revision(store).unwrap(),
            RetryAcceptedInputDelivery::new(id(40), gate, steering_input(), input),
        ),
    );
}

fn assert_permanent_input(
    store: &HomeStore,
    storage: SyndicStorage,
    lifecycle: AcceptedInputLifecycle,
    expected_revision: u64,
) {
    let input = storage
        .accepted_input(store, steering_input(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(input.record().lifecycle(), lifecycle);
    assert_eq!(input.record().revision().get(), expected_revision);
    assert_eq!(input.record().marker_count(), 1);
    let order = storage
        .accepted_order(store, id(40), None, page_limit())
        .unwrap();
    assert_eq!(order.records().len(), 2);
    assert_eq!(order.records()[0].input_id(), steering_input());
    assert_eq!(order.records()[0].input_revision().get(), expected_revision);
}

#[test]
fn delivery_claim_retry_and_success_preserve_identity_and_exact_live_accounting() {
    let home = TestHome::new("phase11-delivery-success");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(populated_records()));
    let original = storage
        .accepted_input(&store, steering_input(), point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();

    begin(&store, storage);
    assert_permanent_input(&store, storage, AcceptedInputLifecycle::Delivering, 2);
    retry(&store, storage);
    assert_permanent_input(&store, storage, AcceptedInputLifecycle::Retryable, 3);
    begin(&store, storage);
    assert_permanent_input(&store, storage, AcceptedInputLifecycle::Delivering, 4);

    let (gate_revision, input_revision) = delivery_request(&store, storage);
    execute(
        &store,
        storage.complete_accepted_input_delivery(
            storage.revision(&store).unwrap(),
            CompleteAcceptedInputDelivery::new(
                id(40),
                gate_revision,
                steering_input(),
                input_revision,
            ),
        ),
    );
    assert_permanent_input(&store, storage, AcceptedInputLifecycle::Delivered, 5);
    let completed = storage
        .accepted_input(&store, steering_input(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(completed.record().content(), original.content());
    assert_eq!(completed.record().disposition(), original.disposition());
    let gate = storage
        .input_gate(&store, id(40), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().revision().get(), 7);
    assert_eq!(gate.record().live_steering_count(), 0);
    assert_eq!(gate.record().live_next_turn_count(), 1);
    assert_eq!(gate.record().live_logical_utf8_bytes(), 0);
    assert!(
        storage
            .accepted_steering(&store, id(40), active_turn(), None, page_limit())
            .unwrap()
            .records()
            .is_empty()
    );
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    SyndicStorage::register(&mut reopened).unwrap();
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn atomic_active_abandonment_retains_unknown_after_later_binding_head_advance() {
    let home = TestHome::new("phase11-mixed-active-abandonment");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    commit(&store, storage, batch(mixed_abandonment_records()));
    store.validate_registered_domains().unwrap();
    assert_eq!(
        storage
            .input_gate(&store, id(40), point_limit())
            .unwrap()
            .unwrap()
            .record()
            .live_logical_utf8_bytes(),
        DELIVERY_UNKNOWN_LOGICAL_BYTES
    );

    let request = abandonment_request(&store, storage);
    execute(
        &store,
        storage.abandon_active_binding(storage.revision(&store).unwrap(), request),
    );

    let admitted = storage
        .accepted_input(&store, steering_input(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        admitted.record().lifecycle(),
        AcceptedInputLifecycle::Admitted
    );
    assert_eq!(
        admitted.record().disposition(),
        &AcceptedInputDisposition::NextTurn(NextTurnReason::ProjectionLost)
    );
    let unknown_id = delivering_input();
    let unknown = storage
        .accepted_input(&store, unknown_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        unknown.record().lifecycle(),
        AcceptedInputLifecycle::DeliveryUnknown
    );
    assert_eq!(
        unknown.record().content().summary().logical_utf8_bytes(),
        DELIVERY_UNKNOWN_LOGICAL_BYTES
    );
    assert!(matches!(
        unknown.record().disposition(),
        AcceptedInputDisposition::SteerActiveTurn(_)
    ));
    assert!(
        storage
            .canonical_item(
                &store,
                SyndicItemId::from_bytes(*unknown_id.as_bytes()),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    let retryable = storage
        .accepted_input(&store, retryable_input(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        retryable.record().lifecycle(),
        AcceptedInputLifecycle::Retryable
    );
    assert_eq!(
        retryable.record().disposition(),
        &AcceptedInputDisposition::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert!(
        storage
            .accepted_steering(&store, id(40), active_turn(), None, page_limit())
            .unwrap()
            .records()
            .is_empty()
    );
    let next = storage
        .accepted_next_turn(&store, id(40), None, page_limit())
        .unwrap();
    let next_ids = next
        .records()
        .iter()
        .map(AcceptedNextTurnIndexRecord::input_id)
        .collect::<Vec<_>>();
    assert_eq!(
        next_ids,
        vec![steering_input(), next_input(), retryable_input(),]
    );
    let gate = storage
        .input_gate(&store, id(40), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().live_steering_count(), 0);
    assert_eq!(gate.record().live_next_turn_count(), 3);
    assert_eq!(gate.record().live_logical_utf8_bytes(), 0);
    assert_eq!(
        gate.record().state(),
        &InputGateState::PendingTurn(active_turn())
    );
    assert_eq!(
        storage
            .accepted_order(&store, id(40), None, page_limit())
            .unwrap()
            .records()
            .len(),
        4
    );
    let abandoned = storage
        .current_binding(&store, id(40), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(abandoned.head().revision().get(), 4);
    assert_eq!(abandoned.head().lifecycle(), BindingLifecycle::Stale);
    assert!(matches!(
        abandoned.binding().state(),
        BindingState::Stale(_)
    ));
    let retired = storage
        .cas_thread_owner(&store, cas_thread(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(retired.record().latest_binding_revision().get(), 4);
    assert_eq!(
        retired
            .record()
            .retired_binding_revision()
            .map(|value| value.get()),
        Some(4)
    );
    let retirement_membership = storage
        .fixture_cas_thread_binding_membership(
            &store,
            cas_thread(),
            abandoned.binding().revision(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(retirement_membership.record().thread_id(), id(40));
    assert_eq!(retirement_membership.record().binding_revision().get(), 4);
    store.validate_registered_domains().unwrap();

    execute(
        &store,
        storage.publish_unbound_binding(
            storage.revision(&store).unwrap(),
            PublishUnboundBinding::new(
                id(40),
                abandoned.binding().revision(),
                abandoned.binding().selected_path(),
                "active projection retirement is complete",
            )
            .unwrap(),
        ),
    );
    let advanced = storage
        .current_binding(&store, id(40), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(advanced.head().revision().get(), 5);
    assert_eq!(advanced.head().lifecycle(), BindingLifecycle::Unbound);
    assert!(matches!(
        advanced.binding().state(),
        BindingState::Unbound { .. }
    ));
    assert_eq!(
        storage
            .cas_thread_owner(&store, cas_thread(), point_limit())
            .unwrap()
            .unwrap()
            .record()
            .retired_binding_revision()
            .map(|value| value.get()),
        Some(4)
    );
    store.validate_registered_domains().unwrap();

    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .current_binding(&reopened, id(40), point_limit())
            .unwrap()
            .unwrap()
            .head()
            .revision()
            .get(),
        5
    );
    assert_eq!(
        reopened_storage
            .accepted_input(&reopened, delivering_input(), point_limit())
            .unwrap()
            .unwrap()
            .record()
            .lifecycle(),
        AcceptedInputLifecycle::DeliveryUnknown
    );
    reopened.validate_registered_domains().unwrap();
    reopened.close().unwrap();
}

#[test]
fn reopen_rejects_delivery_unknown_without_exact_dispatch_provenance() {
    let home = TestHome::new("phase11-corrupt-delivery-unknown");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut records = populated_records();
    records.retain(|record| {
        !matches!(
            record,
            FixtureRecord::AcceptedSteering(index) if index.input_id() == steering_input()
        )
    });
    for record in &mut records {
        match record {
            FixtureRecord::AcceptedInput(input) if input.id() == steering_input() => {
                *input = AcceptedInputRecord::new(
                    input.id(),
                    input.thread_id(),
                    input.revision(),
                    input.ordinal(),
                    input.gate_revision(),
                    AcceptedInputDisposition::NextTurn(NextTurnReason::ProjectionLost),
                    AcceptedInputLifecycle::DeliveryUnknown,
                    input.content(),
                    input.marker_count(),
                    input.admitted_at(),
                );
            }
            FixtureRecord::InputGate(gate) if gate.thread_id() == id(40) => {
                *gate = InputGateRecord::new(
                    gate.thread_id(),
                    gate.revision(),
                    gate.state().clone(),
                    gate.accepted_high_water(),
                    0,
                    gate.live_next_turn_count(),
                    0,
                )
                .unwrap();
            }
            _ => {}
        }
    }
    commit(&store, storage, batch(records));
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("delivery-unknown input reopened without dispatch provenance"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(
                source.to_string(),
                "delivery-unknown accepted input lacks dispatch provenance"
            );
        }
        other => panic!("expected delivery-provenance validation error, got {other:?}"),
    }
    reopened.close().unwrap();
}

#[test]
fn reopen_rejects_delivery_unknown_without_atomic_projection_retirement() {
    let home = TestHome::new("phase11-corrupt-delivery-unknown-retirement");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut records = mixed_abandonment_records();
    records.retain(|record| {
        !matches!(
            record,
            FixtureRecord::AcceptedSteering(index) if index.input_id() == delivering_input()
        )
    });
    for record in &mut records {
        match record {
            FixtureRecord::AcceptedInput(input) if input.id() == delivering_input() => {
                *input = AcceptedInputRecord::new(
                    input.id(),
                    input.thread_id(),
                    input.revision(),
                    input.ordinal(),
                    input.gate_revision(),
                    input.disposition().clone(),
                    AcceptedInputLifecycle::DeliveryUnknown,
                    input.content(),
                    input.marker_count(),
                    input.admitted_at(),
                );
            }
            FixtureRecord::InputGate(gate) if gate.thread_id() == id(40) => {
                *gate = InputGateRecord::new(
                    gate.thread_id(),
                    gate.revision(),
                    gate.state().clone(),
                    gate.accepted_high_water(),
                    2,
                    gate.live_next_turn_count(),
                    0,
                )
                .unwrap();
            }
            _ => {}
        }
    }
    commit(&store, storage, batch(records));
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register(&mut reopened) {
        Ok(_) => panic!("delivery-unknown input reopened without projection retirement"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(
                source.to_string(),
                "delivery-unknown CAS projection lacks exact retirement history"
            );
        }
        other => panic!("expected retirement-history validation error, got {other:?}"),
    }
    reopened.close().unwrap();
}

#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CursorReadLimits, HomeCommand, HomeHealthState, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use beryl_model::{BindingRevision, SyndicAcceptedInputId, SyndicItemId};
use syndic_storage::*;

use support::{
    TestHome, batch, commit, id,
    phase11::{
        DELIVERY_UNKNOWN_LOGICAL_BYTES, abandonment_request, delivering_input,
        mixed_abandonment_records, retryable_input,
    },
    populated::{active_snapshot, active_turn, cas_thread, cas_turn, next_input, steering_input},
};

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn page_limit() -> CursorReadLimits {
    CursorReadLimits::new(16, 1_000_000).unwrap()
}

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

fn command(store: &HomeStore, contribution: beryl_home_store::MutationContribution) -> HomeCommand {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    command
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AbandonmentCutState {
    Old,
    New,
}

fn accepted(
    store: &HomeStore,
    storage: SyndicStorage,
    input_id: SyndicAcceptedInputId,
    revision: u64,
    lifecycle: AcceptedInputLifecycle,
) -> AcceptedInputRecord {
    let input = storage
        .accepted_input(store, input_id, point_limit())
        .unwrap()
        .unwrap()
        .record()
        .clone();
    assert_eq!(input.thread_id(), id(40));
    assert_eq!(input.revision().get(), revision);
    assert_eq!(input.lifecycle(), lifecycle);
    input
}

fn assert_steering_target(input: &AcceptedInputRecord) {
    let AcceptedInputDisposition::SteerActiveTurn(target) = input.disposition() else {
        panic!("accepted input lost its exact steering target");
    };
    assert_eq!(target.pending().binding_revision().get(), 3);
    assert_eq!(target.pending().snapshot_id(), active_snapshot());
    assert_eq!(target.pending().active_turn_id(), active_turn());
    assert_eq!(target.pending().cas_thread_id(), &cas_thread());
    assert_eq!(target.cas_turn_id(), &cas_turn());
}

fn assert_order(
    store: &HomeStore,
    storage: SyndicStorage,
    expected: &[(SyndicAcceptedInputId, u64)],
) {
    let order = storage
        .accepted_order(store, id(40), None, page_limit())
        .unwrap();
    let actual = order
        .records()
        .iter()
        .map(|record| (record.input_id(), record.input_revision().get()))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn assert_old_binding(store: &HomeStore, storage: SyndicStorage) {
    let current = storage
        .current_binding(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.head().revision().get(), 3);
    assert_eq!(current.head().lifecycle(), BindingLifecycle::Active);
    assert!(matches!(current.binding().state(), BindingState::Active(_)));
    assert!(
        storage
            .binding(
                store,
                id(40),
                BindingRevision::new(4).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    let owner = storage
        .cas_thread_owner(store, cas_thread(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(owner.record().thread_id(), id(40));
    assert_eq!(owner.record().first_binding_revision().get(), 2);
    assert_eq!(owner.record().latest_binding_revision().get(), 3);
    assert_eq!(owner.record().retired_binding_revision(), None);
    let membership = storage
        .fixture_cas_thread_binding_membership(
            store,
            cas_thread(),
            BindingRevision::new(3).unwrap(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(membership.record().thread_id(), id(40));
    assert_eq!(membership.record().binding_revision().get(), 3);
    assert!(
        storage
            .fixture_cas_thread_binding_membership(
                store,
                cas_thread(),
                BindingRevision::new(4).unwrap(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
}

fn assert_new_binding(store: &HomeStore, storage: SyndicStorage) {
    let current = storage
        .current_binding(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.head().revision().get(), 4);
    assert_eq!(current.head().lifecycle(), BindingLifecycle::Stale);
    let BindingState::Stale(stale) = current.binding().state() else {
        panic!("completed abandonment did not publish stale provenance");
    };
    assert_eq!(stale.cas_thread_id(), &cas_thread());
    let prior = storage
        .binding(
            store,
            id(40),
            BindingRevision::new(3).unwrap(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(prior.record().state(), BindingState::Active(_)));
    let owner = storage
        .cas_thread_owner(store, cas_thread(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(owner.record().thread_id(), id(40));
    assert_eq!(owner.record().first_binding_revision().get(), 2);
    assert_eq!(owner.record().latest_binding_revision().get(), 4);
    assert_eq!(
        owner
            .record()
            .retired_binding_revision()
            .map(|value| value.get()),
        Some(4)
    );
    for revision in [3, 4] {
        let membership = storage
            .fixture_cas_thread_binding_membership(
                store,
                cas_thread(),
                BindingRevision::new(revision).unwrap(),
                point_limit(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(membership.record().thread_id(), id(40));
        assert_eq!(membership.record().binding_revision().get(), revision);
    }
}

fn observe_old(store: &HomeStore, storage: SyndicStorage) -> AbandonmentCutState {
    assert_old_binding(store, storage);
    let admitted = accepted(
        store,
        storage,
        steering_input(),
        1,
        AcceptedInputLifecycle::Admitted,
    );
    let next = accepted(
        store,
        storage,
        next_input(),
        1,
        AcceptedInputLifecycle::Admitted,
    );
    let delivering = accepted(
        store,
        storage,
        delivering_input(),
        1,
        AcceptedInputLifecycle::Delivering,
    );
    let retryable = accepted(
        store,
        storage,
        retryable_input(),
        1,
        AcceptedInputLifecycle::Retryable,
    );
    assert_steering_target(&admitted);
    assert_steering_target(&delivering);
    assert_steering_target(&retryable);
    assert_eq!(admitted.gate_revision().get(), 2);
    assert_eq!(delivering.gate_revision().get(), 2);
    assert_eq!(retryable.gate_revision().get(), 2);
    assert_eq!(next.gate_revision().get(), 3);
    assert_eq!(
        next.disposition(),
        &AcceptedInputDisposition::NextTurn(NextTurnReason::WorkerCapacity)
    );
    assert_eq!(
        delivering.content().summary().logical_utf8_bytes(),
        DELIVERY_UNKNOWN_LOGICAL_BYTES
    );
    assert_order(
        store,
        storage,
        &[
            (steering_input(), 1),
            (next_input(), 1),
            (delivering_input(), 1),
            (retryable_input(), 1),
        ],
    );
    let steering = storage
        .accepted_steering(store, id(40), active_turn(), None, page_limit())
        .unwrap();
    assert_eq!(
        steering
            .records()
            .iter()
            .map(|record| (record.input_id(), record.input_revision().get()))
            .collect::<Vec<_>>(),
        vec![
            (steering_input(), 1),
            (delivering_input(), 1),
            (retryable_input(), 1),
        ]
    );
    let next_routes = storage
        .accepted_next_turn(store, id(40), None, page_limit())
        .unwrap();
    assert_eq!(
        next_routes
            .records()
            .iter()
            .map(|record| (record.input_id(), record.input_revision().get()))
            .collect::<Vec<_>>(),
        vec![(next_input(), 1)]
    );
    let gate = storage
        .input_gate(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    let AcceptedInputDisposition::SteerActiveTurn(target) = delivering.disposition() else {
        unreachable!("steering target was already asserted");
    };
    assert_eq!(gate.record().revision().get(), 3);
    assert_eq!(
        gate.record().state(),
        &InputGateState::Steerable(target.clone())
    );
    assert_eq!(gate.record().accepted_high_water(), 4);
    assert_eq!(gate.record().live_steering_count(), 3);
    assert_eq!(gate.record().live_next_turn_count(), 1);
    assert_eq!(
        gate.record().live_logical_utf8_bytes(),
        DELIVERY_UNKNOWN_LOGICAL_BYTES
    );
    assert!(
        storage
            .canonical_item(
                store,
                SyndicItemId::from_bytes(*delivering_input().as_bytes()),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    AbandonmentCutState::Old
}

fn observe_new(store: &HomeStore, storage: SyndicStorage) -> AbandonmentCutState {
    assert_new_binding(store, storage);
    let admitted = accepted(
        store,
        storage,
        steering_input(),
        2,
        AcceptedInputLifecycle::Admitted,
    );
    let next = accepted(
        store,
        storage,
        next_input(),
        1,
        AcceptedInputLifecycle::Admitted,
    );
    let unknown = accepted(
        store,
        storage,
        delivering_input(),
        2,
        AcceptedInputLifecycle::DeliveryUnknown,
    );
    let retryable = accepted(
        store,
        storage,
        retryable_input(),
        2,
        AcceptedInputLifecycle::Retryable,
    );
    assert_eq!(
        admitted.disposition(),
        &AcceptedInputDisposition::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert_eq!(
        next.disposition(),
        &AcceptedInputDisposition::NextTurn(NextTurnReason::WorkerCapacity)
    );
    assert_steering_target(&unknown);
    assert_eq!(
        retryable.disposition(),
        &AcceptedInputDisposition::NextTurn(NextTurnReason::ProjectionLost)
    );
    assert_eq!(admitted.gate_revision().get(), 2);
    assert_eq!(unknown.gate_revision().get(), 2);
    assert_eq!(retryable.gate_revision().get(), 2);
    assert_eq!(next.gate_revision().get(), 3);
    assert_eq!(
        unknown.content().summary().logical_utf8_bytes(),
        DELIVERY_UNKNOWN_LOGICAL_BYTES
    );
    assert_order(
        store,
        storage,
        &[
            (steering_input(), 2),
            (next_input(), 1),
            (delivering_input(), 2),
            (retryable_input(), 2),
        ],
    );
    assert!(
        storage
            .accepted_steering(store, id(40), active_turn(), None, page_limit())
            .unwrap()
            .records()
            .is_empty()
    );
    let next_routes = storage
        .accepted_next_turn(store, id(40), None, page_limit())
        .unwrap();
    assert_eq!(
        next_routes
            .records()
            .iter()
            .map(|record| (record.input_id(), record.input_revision().get()))
            .collect::<Vec<_>>(),
        vec![
            (steering_input(), 2),
            (next_input(), 1),
            (retryable_input(), 2),
        ]
    );
    let gate = storage
        .input_gate(store, id(40), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.record().revision().get(), 4);
    assert_eq!(
        gate.record().state(),
        &InputGateState::PendingTurn(active_turn())
    );
    assert_eq!(gate.record().accepted_high_water(), 4);
    assert_eq!(gate.record().live_steering_count(), 0);
    assert_eq!(gate.record().live_next_turn_count(), 3);
    assert_eq!(gate.record().live_logical_utf8_bytes(), 0);
    assert!(
        storage
            .canonical_item(
                store,
                SyndicItemId::from_bytes(*delivering_input().as_bytes()),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
    AbandonmentCutState::New
}

fn observe(store: &HomeStore, storage: SyndicStorage) -> AbandonmentCutState {
    let revision = storage
        .current_binding(store, id(40), point_limit())
        .unwrap()
        .unwrap()
        .head()
        .revision()
        .get();
    match revision {
        3 => observe_old(store, storage),
        4 => observe_new(store, storage),
        other => panic!("fault cut exposed partial binding revision {other}"),
    }
}

#[test]
fn mixed_active_abandonment_persistence_cuts_recover_only_complete_old_or_new_state() {
    for (name, point, expected) in [
        (
            "phase11-abandonment-before-commit",
            FaultPoint::BeforeCommit,
            Some(AbandonmentCutState::Old),
        ),
        (
            "phase11-abandonment-after-commit-before-persist",
            FaultPoint::AfterCommitBeforePersist,
            None,
        ),
        (
            "phase11-abandonment-after-persist",
            FaultPoint::AfterPersist,
            Some(AbandonmentCutState::New),
        ),
    ] {
        let home = TestHome::new(name);
        let faults = FaultController::new();
        let mut store = open_with_faults(home.path(), faults.clone());
        let storage = SyndicStorage::register(&mut store).unwrap();
        commit(&store, storage, batch(mixed_abandonment_records()));
        store.validate_registered_domains().unwrap();

        let request = abandonment_request(&store, storage);
        let contribution =
            storage.abandon_active_binding(storage.revision(&store).unwrap(), request);
        faults.fail_next(point);
        assert!(store.execute(command(&store, contribution)).is_err());
        assert_eq!(store.health().state(), HomeHealthState::Verifying);
        store.verify_health().unwrap();
        let recovered = observe(&store, storage);
        if let Some(expected) = expected {
            assert_eq!(recovered, expected);
        }
        store.validate_registered_domains().unwrap();
        store.close().unwrap();

        let mut reopened = open_with_faults(home.path(), FaultController::new());
        let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
        assert_eq!(observe(&reopened, reopened_storage), recovered);
        reopened.validate_registered_domains().unwrap();
        reopened.close().unwrap();
    }
}

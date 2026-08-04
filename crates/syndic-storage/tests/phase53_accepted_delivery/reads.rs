use beryl_model::{
    AcceptedInputRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration,
    CasProcessGeneration, CasTurnId, InputGateRevision, RootId, RuntimeId, SyndicAcceptedInputId,
};
use std::sync::Mutex;
use syndic_storage::test_faults::{
    FixtureRecord, ready_steering_read_metrics, reset_ready_steering_read_metrics,
};
use syndic_storage::*;

use crate::{
    accepted_support::{large_ready_generation, limit, seeded},
    support::{
        id,
        phase11::{delivering_input, mixed_abandonment_records, retryable_input},
        populated::{
            active_snapshot, active_turn, cas_thread, cas_turn, next_input, populated_records,
            steering_input,
        },
    },
};

static READY_READ_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn exact_ready_input_resolves_with_fixed_point_work_on_a_large_generation() {
    let _ready_read_guard = READY_READ_LOCK.lock().unwrap();
    let (_home, store, storage) =
        seeded("phase53-large-ready-steering", large_ready_generation(384));

    reset_ready_steering_read_metrics();
    let resolved = storage
        .ready_steering_input(&store, steering_input(), limit())
        .unwrap()
        .expect("fixture input is exactly ready");

    assert_eq!(resolved.input().id(), steering_input());
    assert_eq!(
        resolved.accepted_input_revision(),
        AcceptedInputRevision::new(1).unwrap()
    );
    assert_eq!(resolved.lifecycle(), AcceptedInputLifecycle::Admitted);
    assert_eq!(
        resolved.gate_revision(),
        InputGateRevision::new(385).unwrap()
    );
    assert_eq!(
        resolved.route(),
        AcceptedRouteHeadProof::new(
            AcceptedRouteGeneration::FIRST,
            AcceptedRouteRevision::new(2).unwrap(),
        )
    );
    assert_eq!(resolved.target().pending().snapshot_id(), active_snapshot());
    assert_eq!(resolved.target().pending().active_turn_id(), active_turn());
    assert_eq!(resolved.target().pending().cas_thread_id(), &cas_thread());
    assert_eq!(resolved.target().cas_turn_id(), &cas_turn());
    assert_eq!(
        resolved.execution().runtime_id(),
        RuntimeId::from_bytes([48; 16])
    );
    assert_eq!(resolved.execution().root_id(), RootId::from_bytes([49; 16]));
    assert_eq!(
        resolved.loaded_generation(),
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(1).unwrap(),
            CasLoadedThreadGeneration::new(1).unwrap(),
        )
    );
    assert_eq!(
        ready_steering_read_metrics().point_reads(),
        12,
        "the exact ready read must not scale with route-generation membership",
    );
    store.close().unwrap();
}

#[test]
fn admitted_and_retryable_are_ready_but_delivering_next_and_missing_are_not() {
    let _ready_read_guard = READY_READ_LOCK.lock().unwrap();
    let (_home, store, storage) = seeded("phase53-ready-lifecycle", mixed_abandonment_records());

    let admitted = storage
        .ready_steering_input(&store, steering_input(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(admitted.lifecycle(), AcceptedInputLifecycle::Admitted);
    let retryable = storage
        .ready_steering_input(&store, retryable_input(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(retryable.lifecycle(), AcceptedInputLifecycle::Retryable);
    for input in [
        delivering_input(),
        next_input(),
        SyndicAcceptedInputId::from_bytes([0xee; 16]),
    ] {
        assert!(
            storage
                .ready_steering_input(&store, input, limit())
                .unwrap()
                .is_none()
        );
    }
    store.close().unwrap();
}

#[test]
fn inconsistent_active_cas_turn_target_is_an_invariant_failure() {
    let _ready_read_guard = READY_READ_LOCK.lock().unwrap();
    let mut records = populated_records();
    let publication = records
        .iter_mut()
        .find_map(|record| match record {
            FixtureRecord::ActiveCasTurn(publication)
                if publication.snapshot_id() == active_snapshot() =>
            {
                Some(publication)
            }
            _ => None,
        })
        .unwrap();
    *publication = ActiveCasTurnRecord::new(
        publication.snapshot_id(),
        publication.thread_id(),
        publication.turn_id(),
        publication.binding_revision(),
        publication.cas_thread_id().clone(),
        CasTurnId::new("wrong-ready-steering-turn").unwrap(),
        publication.published_at(),
    );
    let (_home, store, storage) = seeded("phase53-ready-target-corruption", records);

    assert!(matches!(
        storage.ready_steering_input(&store, steering_input(), limit()),
        Err(SyndicReadError::Invariant(
            "ready steering execution relationships disagree"
        ))
    ));
    assert_eq!(
        storage
            .input_gate(&store, id(40), limit())
            .unwrap()
            .unwrap()
            .revision(),
        InputGateRevision::new(4).unwrap()
    );
    store.close().unwrap();
}

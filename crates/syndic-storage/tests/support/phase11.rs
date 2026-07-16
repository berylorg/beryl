use beryl_home_store::HomeStore;
use beryl_model::{AcceptedInputRevision, InputGateRevision, SyndicAcceptedInputId};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::*;

use super::{
    composer_content_records, id,
    populated::{active_snapshot, active_turn, next_input, populated_records, steering_input},
    timestamp,
};

pub const DELIVERY_UNKNOWN_LOGICAL_BYTES: u64 = 19;

pub fn delivering_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([70; 16])
}

pub fn retryable_input() -> SyndicAcceptedInputId {
    SyndicAcceptedInputId::from_bytes([71; 16])
}

pub fn mixed_abandonment_records() -> Vec<FixtureRecord> {
    let mut records = populated_records();
    let steering = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedInput(input) if input.id() == steering_input() => {
                Some(input.clone())
            }
            _ => None,
        })
        .unwrap();
    let empty_content = records
        .iter()
        .find_map(|record| match record {
            FixtureRecord::AcceptedInput(input) if input.id() == next_input() => {
                Some(input.content())
            }
            _ => None,
        })
        .unwrap();
    let payload =
        ComposerPayload::new(vec![ComposerAtom::text("possibly dispatched").unwrap()]).unwrap();
    let (delivering_content, delivering_content_records) = composer_content_records(&payload);
    assert_eq!(
        delivering_content.summary().logical_utf8_bytes(),
        DELIVERY_UNKNOWN_LOGICAL_BYTES
    );
    for record in &mut records {
        if let FixtureRecord::InputGate(gate) = record
            && gate.thread_id() == id(40)
        {
            *gate = InputGateRecord::new(
                gate.thread_id(),
                gate.revision(),
                gate.state().clone(),
                4,
                3,
                1,
                DELIVERY_UNKNOWN_LOGICAL_BYTES,
            )
            .unwrap();
        }
    }
    for (input_id, ordinal, lifecycle, content) in [
        (
            delivering_input(),
            3,
            AcceptedInputLifecycle::Delivering,
            delivering_content,
        ),
        (
            retryable_input(),
            4,
            AcceptedInputLifecycle::Retryable,
            empty_content,
        ),
    ] {
        let ordinal = AcceptedInputOrdinal::new(ordinal).unwrap();
        let revision = AcceptedInputRevision::new(1).unwrap();
        records.extend([
            FixtureRecord::AcceptedInput(AcceptedInputRecord::new(
                input_id,
                id(40),
                revision,
                ordinal,
                InputGateRevision::new(2).unwrap(),
                steering.disposition().clone(),
                lifecycle,
                content,
                0,
                timestamp(8),
            )),
            FixtureRecord::AcceptedOrder(AcceptedOrderIndexRecord::new(
                id(40),
                ordinal,
                input_id,
                revision,
            )),
            FixtureRecord::AcceptedSteering(AcceptedSteeringIndexRecord::new(
                id(40),
                active_turn(),
                ordinal,
                input_id,
                revision,
            )),
        ]);
    }
    records.extend(delivering_content_records);
    records
}

pub fn abandonment_request(store: &HomeStore, storage: SyndicStorage) -> AbandonActiveBinding {
    let limit = SyndicPointReadLimit::new(1_000_000).unwrap();
    let binding = storage
        .current_binding(store, id(40), limit)
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("fixture binding is not active");
    };
    let snapshot = storage
        .execution_snapshot(store, active_snapshot(), limit)
        .unwrap()
        .unwrap();
    let stale = StaleCasBinding::new(
        active.usable().execution().clone(),
        active.usable().cas_thread_id().clone(),
        Some(active.usable().tool_profile()),
        Some(active.usable().represented_prefix()),
        Some(active.usable().lineage()),
        Some(active.usable().native_turn_count()),
        Some(snapshot.record().loaded_generation()),
        "active projection authority lost",
        timestamp(9),
    )
    .unwrap();
    AbandonActiveBinding::new(
        id(40),
        binding.binding().revision(),
        InputGateRevision::new(3).unwrap(),
        binding.binding().selected_path(),
        stale,
    )
}

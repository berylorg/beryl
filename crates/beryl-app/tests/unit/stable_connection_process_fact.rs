use beryl_model::{CasProcessGeneration, RuntimeId};

use super::*;

#[test]
fn connection_fact_drop_retires_while_router_observations_remain_read_only() {
    let runtime_id = RuntimeId::from_bytes([186; 16]);
    let process_generation = CasProcessGeneration::new(82_186).unwrap();
    let fact = ConnectionProcessFact::register(runtime_id, process_generation, 186).unwrap();
    let first_observation = fact.observe();
    let retained_observation = first_observation.clone();

    let active = first_observation.snapshot().unwrap();
    assert_eq!(active.active_connection_count(), 1);
    assert_eq!(
        active.latest_connection_fact().unwrap().state(),
        LiveEventConnectionState::Active
    );

    drop(fact);
    let retired = first_observation.snapshot().unwrap();
    assert_eq!(retired.active_connection_count(), 0);
    assert_eq!(
        retired.latest_connection_fact().unwrap().state(),
        LiveEventConnectionState::Retired(LiveEventTargetCloseReason::WorkerStopped)
    );

    let retirement_revision = retired.revision();
    drop(first_observation);
    assert_eq!(
        retained_observation.snapshot().unwrap().revision(),
        retirement_revision
    );
}

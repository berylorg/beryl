use std::sync::{Arc, Mutex};

use beryl_model::{CasProcessGeneration, RuntimeId};
use beryl_stream::PagePoolDiagnostics;

use super::*;

fn stable_connection_without_epoch(seed: u8) -> Arc<ProjectionConnection> {
    let runtime_id = RuntimeId::from_bytes([seed; 16]);
    let process_generation = CasProcessGeneration::new(u64::from(seed) + 82_000).unwrap();
    let authority = Arc::new(
        ConnectionRegistryAuthority::new(runtime_id, process_generation)
            .expect("test connection generation is available"),
    );
    let process_fact = StableConnectionProcessFact::register(
        runtime_id,
        process_generation,
        authority.generation.get(),
    )
    .expect("test process fact is available");
    let forwarding_hub = ForwardingHub::new(Arc::clone(&authority));
    Arc::new(ProjectionConnection {
        authority,
        runtime_id,
        process_generation,
        process_fact,
        forwarding_hub,
        ordinary_shutdown: Mutex::new(OrdinaryShutdownSettlement::Unsettled),
        runtime: Mutex::new(None),
        provider_pages: Mutex::new(PagePoolDiagnostics {
            page_capacity: 0,
            page_count: 0,
            available: 0,
            leased: 0,
            high_water: 0,
            total_leases: 0,
            exhausted: 0,
        }),
        recovery_diagnostics: Arc::new(
            super::super::recovery_source_broker::RecoveryReplayDiagnosticsSlot::new(),
        ),
    })
}

#[test]
fn phase82_dropped_epoch_adoption_barrier_leaves_forwarding_hub_inert() {
    let connection = stable_connection_without_epoch(182);

    let barrier = connection.lock_epoch_for_adoption().unwrap();
    drop(barrier);

    let guard = connection.lock_forwarding_epoch_for_adoption().unwrap();
    assert!(guard.is_inert());
    drop(guard);
    assert!(matches!(
        connection.current_epoch(),
        Err(ProjectionCoordinatorError::ProjectionWorkerStopped)
    ));
}

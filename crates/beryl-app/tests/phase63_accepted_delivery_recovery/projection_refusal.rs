use std::path::Path;

use beryl_app::cas_projection::{
    ProjectionConnectionService, ProjectionConnectionServiceCloseError, ProjectionServiceConfig,
};
use beryl_backend::ManagedBackendClientConnector;
use beryl_model::{CasProcessGeneration, RuntimeId};
use syndic_storage::AcceptedRouteEffectiveState;

use crate::{
    app_support::promote_installed_next,
    phase62_support::{
        AUTHORIZATION, NormalTerminalServer, SessionSlot, TIMEOUT, accepted_route_state,
        execution_binding, install_next_records, open_registered_home, ready_provider, wait_until,
    },
};

#[test]
fn accepted_next_projection_refusal_fails_scheduler_closed_before_backend_request() {
    assert_unsupported_projection_fails_closed(193, 63_412, false);
}

#[test]
fn recovered_pending_projection_refusal_fails_scheduler_closed_before_backend_request() {
    assert_unsupported_projection_fails_closed(194, 63_413, true);
}

fn assert_unsupported_projection_fails_closed(
    seed: u8,
    process_generation: u64,
    promote_before_service: bool,
) {
    let (directory, home, storage, state) = open_registered_home();
    let execution = execution_binding(RuntimeId::from_bytes([seed; 16]));
    let ids = install_next_records(&home, storage, seed, execution.clone());
    let runtime_id = execution.runtime_id();
    if promote_before_service {
        promote_installed_next(&home, storage, &state, ids, seed.wrapping_add(30));
    }

    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let provider = ready_provider(provider_slot, state.assets());
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        Box::new(provider),
    )
    .unwrap();
    assert!(
        service
            .accepted_input_scheduler_diagnostics()
            .recovery_handed_off()
    );

    let server = NormalTerminalServer::spawn_admission_only();
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = service
        .admit(
            &connector,
            runtime_id,
            CasProcessGeneration::new(process_generation).unwrap(),
            Path::new(crate::EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    server.wait_for_admission();

    service.notify_scheduled_ordinary_execution_ready();
    let diagnostics = wait_until("unsupported projection scheduler refusal", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.fatal() && diagnostics.workers_active() == 0).then_some(diagnostics)
    });
    assert!(diagnostics.workers_started() >= 1);
    assert_eq!(
        accepted_route_state(&service, storage, &ids),
        AcceptedRouteEffectiveState::Promoted
    );
    wait_until("refused projection returns its session", || {
        slot.is_ready().then_some(())
    });

    assert!(matches!(
        service.close(),
        Err(ProjectionConnectionServiceCloseError::SchedulerShutdown)
    ));
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

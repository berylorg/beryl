use super::*;

use beryl_app::cas_projection::RunningSessionRecoveryShutdownError;
use beryl_home_store::{
    HomeCommand, HomeHealthState,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete};

#[test]
fn ambiguous_promotion_collision_fails_closed_before_cas_dispatch() {
    let faults = FaultController::new();
    let slot = SessionSlot::default();
    let directory = tempfile::tempdir().unwrap();
    let mut home = HomeStore::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut home).unwrap();
    beryl_state::BerylState::register(&mut home).unwrap();
    let execution = execution_binding(RuntimeId::from_bytes([170; 16]));
    let ids = install_next_records(&home, storage, 170, execution.clone());
    let supervisor = RunningSessionRecoverySupervisor::start(
        home,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        Box::new(ReadyProviderFactory::every_epoch(slot.clone())),
    )
    .unwrap();
    let service = supervisor.acquire().unwrap();
    let initial_home_generation = service.home_generation();
    let initial_service_generation = service.service_generation();
    let initial_service_pointer = std::ptr::from_ref::<ProjectionConnectionService>(&*service);

    let server = NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            execution.runtime_id(),
            CasProcessGeneration::new(62_007).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    server.wait_for_admission();

    let reserved = install_scheduled_promotion_barrier(ids.thread);
    let reconciling = install_scheduled_promotion_reconciliation_barrier(ids.thread);
    service.notify_scheduled_ordinary_execution_ready();
    assert!(reserved.wait_until_paused(TIMEOUT));
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    reserved.release();
    assert!(
        reconciling.wait_until_paused(TIMEOUT),
        "ambiguous command did not reach reconciliation"
    );
    drop(reserved);
    {
        let command_home = service.live_home_command().unwrap();
        assert_eq!(
            command_home.home().health().state(),
            HomeHealthState::Verifying
        );
    }

    wait_until("supervisor same-generation verification", || {
        (supervisor.diagnostics().verification_successes() == 1).then_some(())
    });
    assert_eq!(service.home_generation(), initial_home_generation);
    assert_eq!(service.service_generation(), initial_service_generation);
    assert_eq!(
        std::ptr::from_ref::<ProjectionConnectionService>(&*service),
        initial_service_pointer
    );
    assert_eq!(
        supervisor.diagnostics().current_home_generation(),
        Some(initial_home_generation)
    );
    assert_eq!(
        supervisor.diagnostics().current_service_generation(),
        Some(initial_service_generation)
    );
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        let mut batch = FixtureBatch::new();
        batch
            .delete(FixtureDelete::AcceptedInput(ids.accepted_input))
            .unwrap();
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command
            .add(storage.fixture_contribution(storage.revision(home).unwrap(), batch))
            .unwrap();
        home.execute(command).unwrap();
    }
    reconciling.release();
    drop(reconciling);

    wait_until("promotion collision scheduler failure", || {
        service
            .accepted_input_scheduler_diagnostics()
            .fatal()
            .then_some(())
    });
    wait_until("promotion collision session return", || {
        slot.is_ready().then_some(())
    });
    assert_eq!(
        service
            .accepted_input_scheduler_diagnostics()
            .workers_started(),
        1
    );

    drop(service);
    assert!(matches!(
        supervisor.shutdown(),
        Err(RunningSessionRecoveryShutdownError::Service(
            beryl_app::cas_projection::ProjectionConnectionServiceCloseError::SchedulerShutdown
        ))
    ));
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

use super::*;

use beryl_home_store::{
    HomeCommand,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::test_faults::{FixtureBatch, FixtureDelete};

#[test]
fn ambiguous_promotion_collision_fails_closed_before_cas_dispatch() {
    let faults = FaultController::new();
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider_and_faults(
        170,
        faults.clone(),
        move |assets| Box::new(ready_provider(provider_slot, assets)),
    );
    let parent = fixture.submit_text("phase62 collision parent");
    fixture.complete_with_assistant(parent, "phase62 collision answer");
    let storage = fixture.storage;
    let thread_id = fixture.thread;
    let execution = syndic::execution_binding();
    let server = NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = fixture
        .store
        .admit(
            &connector,
            execution.runtime_id(),
            CasProcessGeneration::new(62_007).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    server.wait_for_admission();

    let reserved = install_scheduled_promotion_barrier(thread_id);
    let reconciling = install_scheduled_promotion_reconciliation_barrier(thread_id);
    let ids = admit_runtime_next_input(&mut fixture, 170);
    assert!(reserved.wait_until_paused(TIMEOUT));
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    reserved.release();
    assert!(
        reconciling.wait_until_paused(TIMEOUT),
        "ambiguous command did not reach reconciliation"
    );

    fixture.store.verify_health().unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::AcceptedInput(ids.accepted_input))
        .unwrap();
    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(storage.fixture_contribution(storage.revision(&fixture.store).unwrap(), batch))
        .unwrap();
    fixture.store.execute(command).unwrap();
    reconciling.release();

    wait_until("promotion collision scheduler failure", || {
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .fatal()
            .then_some(())
    });
    wait_until("promotion collision session return", || {
        slot.is_ready().then_some(())
    });
    assert_eq!(
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .workers_started(),
        1
    );

    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close(),
        Err(beryl_app::cas_projection::ProjectionConnectionServiceCloseError::SchedulerShutdown)
    ));
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

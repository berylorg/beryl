use super::*;

use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};

#[test]
fn service_shutdown_after_reservation_waits_for_exact_promotion() {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider(172, move |assets| {
        Box::new(ready_provider(provider_slot, assets))
    });
    let parent = fixture.submit_text("phase62 shutdown parent");
    fixture.complete_with_assistant(parent, "phase62 shutdown answer");
    let thread_id = fixture.thread;
    let execution = syndic::execution_binding();
    let server = NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            execution.runtime_id(),
            CasProcessGeneration::new(62_009).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    slot.replace(session);
    server.wait_for_admission();

    let barrier = install_scheduled_promotion_barrier(thread_id);
    let ids = admit_runtime_next_input(&mut fixture, 172);
    assert!(
        barrier.wait_until_paused(TIMEOUT),
        "scheduler did not reserve promotion before shutdown"
    );
    let (directory, service) = fixture.into_service();
    let close_worker = thread::spawn(move || service.close());
    wait_until("shutdown retirement fence", || {
        retirement.is_retired().then_some(())
    });
    assert!(
        !close_worker.is_finished(),
        "shutdown overtook a winning promotion reservation"
    );

    barrier.release();
    drop(retirement);
    close_worker
        .join()
        .expect("service shutdown worker did not panic")
        .expect("winning reserved promotion must not make shutdown fail");
    server.join();
    assert!(!slot.is_ready());

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        accepted_route_state(&reopened, reopened_storage, &ids),
        AcceptedRouteEffectiveState::Promoted,
        "shutdown after reservation must preserve the winning promotion"
    );
    reopened.close().unwrap();
    drop(directory);
}

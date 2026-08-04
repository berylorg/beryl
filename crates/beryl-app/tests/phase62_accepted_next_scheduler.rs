#![cfg(feature = "test-faults")]

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[path = "phase10_projection/syndic.rs"]
mod syndic;

#[path = "phase62_accepted_next_scheduler/promotion_collision.rs"]
mod promotion_collision;
#[path = "phase62_accepted_next_scheduler/promotion_faults.rs"]
mod promotion_faults;
#[path = "phase62_accepted_next_scheduler/shutdown.rs"]
mod shutdown;
#[path = "phase62_accepted_next_scheduler/support.rs"]
mod support;

use std::{
    path::Path,
    sync::mpsc::{TryRecvError, sync_channel},
    thread,
};

use beryl_app::cas_projection::{
    ProjectionConnectionService, ProjectionServiceConfig,
    test_faults::{
        install_scheduled_generation_invalidation_barrier, install_scheduled_promotion_barrier,
        install_scheduled_promotion_reconciliation_barrier,
        install_scheduled_promotion_reservation_barrier,
    },
};
use beryl_backend::ManagedBackendClientConnector;
use beryl_home_store::test_faults::FaultController;
use beryl_model::{CasProcessGeneration, RuntimeId};
use syndic_storage::{AcceptedRouteEffectiveState, SyndicReadError, SyndicStorage, TurnLifecycle};

use support::{
    NormalTerminalServer, SessionSlot, TIMEOUT, UnavailableProvider, accepted_route_state,
    admit_runtime_next_input, current_cas_thread_id, execution_binding, install_next_records,
    open_registered_home, ready_provider, recover_home_generation_before_promotion,
    try_accepted_route_state, wait_until,
};

#[test]
fn same_process_next_turn_promotes_projects_and_dispatches_once() {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider(162, move |assets| {
        Box::new(ready_provider(provider_slot, assets))
    });
    let parent = fixture.submit_text("phase62 completed parent");
    fixture.complete_with_assistant(parent, "phase62 completed answer");
    let storage = fixture.storage;
    let thread = fixture.thread;
    let cas_thread_id = current_cas_thread_id(&fixture.store, storage, thread);
    let execution = syndic::execution_binding();
    let runtime_id = execution.runtime_id();
    assert!(
        fixture
            .store
            .accepted_input_scheduler_diagnostics()
            .recovery_handed_off()
    );

    let server = NormalTerminalServer::spawn_resume_terminal(cas_thread_id);
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = fixture
        .store
        .admit(
            &connector,
            runtime_id,
            CasProcessGeneration::new(62_001).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);

    let ids = admit_runtime_next_input(&mut fixture, 162);
    let service = &fixture.store;

    let promoted = wait_until("accepted-input promotion", || {
        let state = accepted_route_state(&service, storage, &ids);
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        if state == AcceptedRouteEffectiveState::Promoted {
            Some(Ok(()))
        } else if diagnostics.fatal() || diagnostics.next_execution_unavailable() > 0 {
            Some(Err((state, diagnostics)))
        } else {
            None
        }
    });
    assert!(
        promoted.is_ok(),
        "scheduler stopped before promotion: {promoted:?}"
    );
    server.wait_for_projection();

    let successor = wait_until("scheduled ordinary terminal", || {
        let thread = storage
            .thread(&service, ids.thread, support::point_limit())
            .ok()
            .flatten()?;
        let successor = thread.committed_tail()?;
        let state = storage
            .turn_state(&service, successor, support::point_limit())
            .ok()
            .flatten()?;
        (state.lifecycle() == TurnLifecycle::Complete).then_some(successor)
    });
    assert_ne!(successor, ids.parent);
    assert_eq!(
        accepted_route_state(&service, storage, &ids),
        AcceptedRouteEffectiveState::Promoted
    );
    wait_until("scheduled session return", || slot.is_ready().then_some(()));
    wait_until("next-turn scan release", || {
        let diagnostics = service.accepted_input_scheduler_diagnostics();
        (diagnostics.workers_active() == 0
            && !diagnostics.next_retained_source_cursor()
            && !diagnostics.next_retained_candidate_cursor())
        .then_some(())
    });

    let diagnostics = service.accepted_input_scheduler_diagnostics();
    assert!(diagnostics.next_source_page_reads() >= 1);
    assert!(diagnostics.next_candidate_page_reads() >= 1);
    assert!(!diagnostics.next_retained_source_cursor());
    assert!(!diagnostics.next_retained_candidate_cursor());
    assert!(!diagnostics.fatal());

    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

#[test]
fn unavailable_execution_authority_leaves_the_durable_candidate_unpromoted() {
    let (directory, home, storage, _state) = open_registered_home();
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        Box::new(UnavailableProvider),
    )
    .unwrap();
    let ids = install_next_records(
        &service,
        storage,
        163,
        execution_binding(RuntimeId::from_bytes([163; 16])),
    );
    service.notify_scheduled_ordinary_execution_ready();
    wait_until("execution-unavailable scheduler observation", || {
        (service
            .accepted_input_scheduler_diagnostics()
            .next_execution_unavailable()
            >= 1)
            .then_some(())
    });
    assert_eq!(
        accepted_route_state(&service, storage, &ids),
        AcceptedRouteEffectiveState::NextTurn(syndic_storage::NextTurnReason::PendingTurn)
    );
    assert!(
        storage
            .turn(&service, ids.parent, support::point_limit())
            .unwrap()
            .is_some()
    );

    service.close().unwrap();
    drop(directory);
}

#[test]
fn connection_retirement_cannot_overtake_reserved_promotion() {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider(165, move |assets| {
        Box::new(ready_provider(provider_slot, assets))
    });
    let parent = fixture.submit_text("phase62 retirement parent");
    fixture.complete_with_assistant(parent, "phase62 retirement answer");
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
            CasProcessGeneration::new(62_002).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    slot.replace(session);
    server.wait_for_admission();

    let barrier = install_scheduled_promotion_barrier(thread_id);
    let ids = admit_runtime_next_input(&mut fixture, 165);
    assert!(
        barrier.wait_until_paused(TIMEOUT),
        "scheduler did not reserve promotion authority"
    );
    assert!(
        matches!(
            accepted_route_state(&fixture.store, storage, &ids),
            AcceptedRouteEffectiveState::NextTurn(_)
        ),
        "reservation must not publish before the test releases its exact cut"
    );

    let (retired_sender, retired) = sync_channel(1);
    let retirement_worker = {
        let retirement = retirement.clone();
        thread::spawn(move || {
            retirement.retire();
            retired_sender.send(()).unwrap();
        })
    };
    wait_until("connection retirement fence", || {
        retirement.is_retired().then_some(())
    });
    assert_eq!(retired.try_recv(), Err(TryRecvError::Empty));

    barrier.release();
    retired
        .recv_timeout(TIMEOUT)
        .expect("promotion reservation must release connection retirement");
    retirement_worker.join().unwrap();
    wait_until("reserved promotion publication", || {
        (accepted_route_state(&fixture.store, storage, &ids)
            == AcceptedRouteEffectiveState::Promoted)
            .then_some(())
    });
    wait_until("retired scheduled session return", || {
        slot.is_ready().then_some(())
    });
    assert!(!fixture.store.accepted_input_scheduler_diagnostics().fatal());
    drop(retirement);

    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

#[test]
fn connection_retirement_before_reservation_leaves_the_candidate_queued() {
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider(166, move |assets| {
        Box::new(ready_provider(provider_slot, assets))
    });
    let parent = fixture.submit_text("phase62 retirement-first parent");
    fixture.complete_with_assistant(parent, "phase62 retirement-first answer");
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
            CasProcessGeneration::new(62_003).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let retirement = session.connection_retirement_handle_for_test();
    slot.replace(session);
    server.wait_for_admission();

    let barrier = install_scheduled_promotion_reservation_barrier(thread_id);
    let ids = admit_runtime_next_input(&mut fixture, 166);
    assert!(
        barrier.wait_until_paused(TIMEOUT),
        "scheduler did not reach the pre-reservation cut"
    );
    retirement.retire();
    assert!(retirement.is_retired());
    barrier.release();

    wait_until("retirement-first worker completion", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (diagnostics.workers_joined() >= 1).then_some(diagnostics)
    });
    wait_until("retirement-first scheduled session return", || {
        slot.is_ready().then_some(())
    });
    let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
    assert_eq!(diagnostics.workers_started(), 1);
    assert!(!diagnostics.fatal());
    assert!(matches!(
        accepted_route_state(&fixture.store, storage, &ids),
        AcceptedRouteEffectiveState::NextTurn(_)
    ));

    drop(retirement);
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

#[test]
fn home_generation_recovery_before_reservation_fails_the_obsolete_service_closed() {
    let faults = FaultController::new();
    let slot = SessionSlot::default();
    let provider_slot = slot.clone();
    let mut fixture = syndic::Fixture::new_with_scheduled_provider_and_faults(
        171,
        faults.clone(),
        move |assets| Box::new(ready_provider(provider_slot, assets)),
    );
    let parent = fixture.submit_text("phase62 home-generation parent");
    fixture.complete_with_assistant(parent, "phase62 home-generation answer");
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
            CasProcessGeneration::new(62_008).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    server.wait_for_admission();

    let initial_generation = fixture.store.home_generation();
    let barrier = install_scheduled_promotion_reservation_barrier(thread_id);
    let invalidation = install_scheduled_generation_invalidation_barrier(thread_id);
    let ids = admit_runtime_next_input(&mut fixture, 171);
    assert!(
        barrier.wait_until_paused(TIMEOUT),
        "promotion did not reach its pre-reservation cut"
    );
    recover_home_generation_before_promotion(&fixture.store, storage, &faults, &ids);
    assert!(fixture.store.health().generation().unwrap() > initial_generation);
    let recovered_storage = SyndicStorage::reacquire(&fixture.store).unwrap();
    barrier.release();
    assert!(
        invalidation.wait_until_paused(TIMEOUT),
        "scheduled worker did not classify the newer healthy generation"
    );
    invalidation.release();

    wait_until("generation-drift worker completion", || {
        let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
        (diagnostics.workers_joined() >= 1).then_some(())
    });
    wait_until("generation-drift session return", || {
        slot.is_ready().then_some(())
    });
    assert!(matches!(
        accepted_route_state(&fixture.store, recovered_storage, &ids),
        AcceptedRouteEffectiveState::NextTurn(_)
    ));
    let diagnostics = fixture.store.accepted_input_scheduler_diagnostics();
    assert_eq!(diagnostics.workers_started(), 1);
    assert!(diagnostics.fatal());

    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close(),
        Err(beryl_app::cas_projection::ProjectionConnectionServiceCloseError::SchedulerShutdown)
    ));
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

#[test]
fn preexisting_next_turn_work_is_released_by_phase63_restart_handoff() {
    let (directory, home, storage, _state) = open_registered_home();
    let ids = install_next_records(
        &home,
        storage,
        164,
        execution_binding(RuntimeId::from_bytes([164; 16])),
    );
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        Box::new(UnavailableProvider),
    )
    .unwrap();
    let before = service.accepted_input_scheduler_diagnostics();
    assert!(before.recovery_handed_off());

    service.notify_scheduled_ordinary_execution_ready();
    wait_until("restart-handoff scheduler scan", || {
        (service
            .accepted_input_scheduler_diagnostics()
            .next_source_page_reads()
            >= 1)
            .then_some(())
    });
    let after = service.accepted_input_scheduler_diagnostics();
    assert!(after.next_source_page_reads() >= 1);
    assert_eq!(
        accepted_route_state(&service, storage, &ids),
        AcceptedRouteEffectiveState::NextTurn(syndic_storage::NextTurnReason::PendingTurn)
    );

    service.close().unwrap();
    drop(directory);
}

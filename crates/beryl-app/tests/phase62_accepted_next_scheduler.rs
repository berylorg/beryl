#![cfg(feature = "test-faults")]

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[path = "phase10_projection/syndic.rs"]
mod syndic;

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
    ProjectionConnectionService, ProjectionServiceConfig, RunningSessionRecoverySupervisor,
    test_faults::{
        install_scheduled_promotion_barrier, install_scheduled_promotion_reservation_barrier,
    },
};
use beryl_backend::ManagedBackendClientConnector;
use beryl_home_store::{
    HomeOpenOptions, HomeSchemaVersion, HomeStore, test_faults::FaultController,
};
use beryl_model::{CasProcessGeneration, RuntimeId};
use syndic_storage::{AcceptedRouteEffectiveState, SyndicReadError, SyndicStorage, TurnLifecycle};

use support::{
    NormalTerminalServer, ReadyProviderFactory, SessionSlot, TIMEOUT, UnavailableProvider,
    accepted_route_state, admit_runtime_next_input, current_cas_thread_id, execution_binding,
    fail_home_generation_before_promotion, install_next_records, open_registered_home,
    ready_provider, seed_runtime_next_input_without_wake, try_accepted_route_state, wait_until,
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
    let cas_thread_id = {
        let command_home = fixture.store.live_home_command().unwrap();
        current_cas_thread_id(command_home.home(), storage, thread)
    };
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
        .admit_lifecycle_test_candidate(
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
        let command_home = service.live_home_command().unwrap();
        let state = accepted_route_state(command_home.home(), storage, &ids);
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
        let command_home = service.live_home_command().ok()?;
        let home = command_home.home();
        let thread = storage
            .thread(home, ids.thread, support::point_limit())
            .ok()
            .flatten()?;
        let successor = thread.committed_tail()?;
        let state = storage
            .turn_state(home, successor, support::point_limit())
            .ok()
            .flatten()?;
        (state.lifecycle() == TurnLifecycle::Complete).then_some(successor)
    });
    assert_ne!(successor, ids.parent);
    {
        let command_home = service.live_home_command().unwrap();
        assert_eq!(
            accepted_route_state(command_home.home(), storage, &ids),
            AcceptedRouteEffectiveState::Promoted
        );
    }
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
    let ids = install_next_records(
        &home,
        storage,
        163,
        execution_binding(RuntimeId::from_bytes([163; 16])),
    );
    let service = ProjectionConnectionService::new(
        home,
        storage,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        Box::new(UnavailableProvider),
    )
    .unwrap();
    service.notify_scheduled_ordinary_execution_ready();
    wait_until("execution-unavailable scheduler observation", || {
        (service
            .accepted_input_scheduler_diagnostics()
            .next_execution_unavailable()
            >= 1)
            .then_some(())
    });
    {
        let command_home = service.live_home_command().unwrap();
        let home = command_home.home();
        assert_eq!(
            accepted_route_state(home, storage, &ids),
            AcceptedRouteEffectiveState::NextTurn(syndic_storage::NextTurnReason::PendingTurn)
        );
        assert!(
            storage
                .turn(home, ids.parent, support::point_limit())
                .unwrap()
                .is_some()
        );
    }

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
        .admit_lifecycle_test_candidate(
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
            {
                let command_home = fixture.store.live_home_command().unwrap();
                accepted_route_state(command_home.home(), storage, &ids)
            },
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
        let command_home = fixture.store.live_home_command().unwrap();
        (accepted_route_state(command_home.home(), storage, &ids)
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
        .admit_lifecycle_test_candidate(
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
        {
            let command_home = fixture.store.live_home_command().unwrap();
            accepted_route_state(command_home.home(), storage, &ids)
        },
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
fn home_generation_failure_before_reservation_makes_supervisor_terminally_unavailable() {
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
    let execution = execution_binding(RuntimeId::from_bytes([171; 16]));
    let ids = install_next_records(&home, storage, 171, execution.clone());
    let supervisor = RunningSessionRecoverySupervisor::start(
        home,
        ProjectionServiceConfig::try_new(128, 8).unwrap(),
        Box::new(ReadyProviderFactory::first_epoch_only(slot.clone())),
    )
    .unwrap();
    let service = supervisor.acquire().unwrap();
    let thread_id = ids.thread;
    let server = NormalTerminalServer::spawn_admission_only();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(
        server.endpoint(),
        support::AUTHORIZATION,
    );
    let session = service
        .admit_lifecycle_test_candidate(
            &connector,
            execution.runtime_id(),
            CasProcessGeneration::new(62_008).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    slot.replace(session);
    server.wait_for_admission();

    let barrier = install_scheduled_promotion_reservation_barrier(thread_id);
    service.notify_scheduled_ordinary_execution_ready();
    assert!(
        barrier.wait_until_paused(TIMEOUT),
        "promotion did not reach its pre-reservation cut"
    );
    fail_home_generation_before_promotion(&supervisor, storage, &faults, &ids);
    barrier.release();
    drop(service);

    wait_until("terminal service disposal", || {
        supervisor.diagnostics().terminal_settled().then_some(())
    });
    assert!(matches!(
        supervisor.acquire(),
        Err(beryl_app::cas_projection::RunningServiceAvailability::Unavailable)
    ));
    assert!(matches!(
        supervisor.shutdown(),
        Err(beryl_app::cas_projection::RunningSessionRecoveryShutdownError::TerminalRecovery)
    ));
    server.join();
    assert!(!slot.is_ready());
    drop(directory);
}

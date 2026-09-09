#![cfg(feature = "test-faults")]

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[path = "process_scheduled_sessions/server.rs"]
mod server;
#[path = "accepted_next_scheduler/support.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;

use std::{path::Path, thread};

use beryl_app::cas_projection::{
    ProcessScheduledExecutionProvider, ScheduledExecutionSessions,
    test_faults::install_scheduled_promotion_barrier,
};
use beryl_backend::{BackendWebSocketEndpoint, ManagedBackendClientConnector};
use beryl_model::{CasProcessGeneration, SyndicThreadId};
use server::ProcessSessionServer;
use syndic_storage::{AcceptedRouteEffectiveState, TurnLifecycle};

use support::{
    AUTHORIZATION, NextRecordIds, NormalTerminalServer, TIMEOUT, accepted_route_state,
    current_cas_thread_id, point_limit, request_policy, seed_runtime_next_input_without_wake,
    tool_authority, wait_until,
};

fn fixture(seed: u8) -> (syndic::Fixture, ScheduledExecutionSessions) {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let fixture = syndic::Fixture::new_with_scheduled_provider(seed, move |_| Box::new(provider));
    (fixture, sessions)
}

fn install_session(
    fixture: &syndic::Fixture,
    sessions: &ScheduledExecutionSessions,
    thread_id: SyndicThreadId,
    endpoint: BackendWebSocketEndpoint,
    generation: u64,
) {
    let binding = syndic::execution_binding();
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            binding.runtime_id(),
            CasProcessGeneration::new(generation).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    sessions
        .register(
            thread_id,
            binding,
            session,
            request_policy(),
            fixture.state.assets(),
            tool_authority(),
        )
        .unwrap();
}

fn resume_server(fixture: &syndic::Fixture, thread: SyndicThreadId) -> NormalTerminalServer {
    let home = fixture.store.live_home_command().unwrap();
    NormalTerminalServer::spawn_resume_terminal(current_cas_thread_id(
        home.home(),
        &fixture.storage,
        thread,
    ))
}

fn controlled_server(
    fixture: &syndic::Fixture,
    thread: SyndicThreadId,
    inputs: &[&str],
) -> ProcessSessionServer {
    let home = fixture.store.live_home_command().unwrap();
    ProcessSessionServer::spawn(
        current_cas_thread_id(home.home(), &fixture.storage, thread),
        inputs.iter().map(|input| (*input).to_owned()).collect(),
    )
}

fn await_terminal(fixture: &syndic::Fixture, ids: NextRecordIds) {
    wait_until("production session terminal capture", || {
        let home = fixture.store.live_home_command().ok()?;
        let thread = fixture
            .storage
            .thread(home.home(), ids.thread, point_limit())
            .ok()
            .flatten()?;
        let turn = thread.committed_tail()?;
        let state = fixture
            .storage
            .turn_state(home.home(), turn, point_limit())
            .ok()
            .flatten()?;
        (turn != ids.parent && state.lifecycle() == TurnLifecycle::Complete).then_some(())
    });
    let home = fixture.store.live_home_command().unwrap();
    assert_eq!(
        accepted_route_state(home.home(), &fixture.storage, &ids),
        AcceptedRouteEffectiveState::Promoted,
    );
}

fn seed_second_thread(fixture: &mut syndic::Fixture) -> NextRecordIds {
    let active = fixture.submit_text(" independent predecessor");
    let source = fixture.activate_without_terminal(active);
    fixture.mark_active_unknown_terminal(active, &source);
    let ids = NextRecordIds {
        thread: fixture.thread,
        parent: active.turn,
        accepted_input: fixture.accept_text(" independent scheduled input"),
    };
    fixture.advance_clock_to(62_102);
    fixture.complete_active_without_assistant(active, &source);
    ids
}

#[test]
fn process_sessions_dispatch_and_capture_independent_threads_without_views() {
    let (mut fixture, sessions) = fixture(181);
    let parent = fixture.submit_text("first completed parent");
    fixture.complete_with_assistant(parent, "first completed answer");
    let second_thread = fixture.create_ordinary(183);
    let parent = fixture.submit_text_on(second_thread, "second completed parent");
    fixture.complete_with_assistant_on(second_thread, parent, "second completed answer");
    let first = seed_runtime_next_input_without_wake(&mut fixture, 181);
    fixture.thread = second_thread;
    let second = seed_second_thread(&mut fixture);
    let first_server = controlled_server(&fixture, first.thread, &[support::SUBMITTED_TEXT]);
    let second_server =
        controlled_server(&fixture, second.thread, &[" independent scheduled input"]);
    install_session(
        &fixture,
        &sessions,
        first.thread,
        first_server.endpoint(),
        71_001,
    );
    assert!(first_server.wait_for_turn_start(0));
    install_session(
        &fixture,
        &sessions,
        second.thread,
        second_server.endpoint(),
        71_002,
    );
    assert!(
        second_server.wait_for_turn_start(0),
        "provider {:?}, scheduler {:?}, failure {:?}",
        sessions.diagnostics(),
        fixture.store.accepted_input_scheduler_diagnostics(),
        fixture.store.persistent_failure_cut_snapshot()
    );
    second_server.finish_turn();
    await_terminal(&fixture, second);
    wait_until("independent second checkout returned", || {
        (sessions.diagnostics().checked_out == 1 && sessions.diagnostics().available == 1)
            .then_some(())
    });
    first_server.finish_turn();
    await_terminal(&fixture, first);
    wait_until("both production checkouts returned", || {
        (sessions.diagnostics().available == 2).then_some(())
    });
    let diagnostic = sessions.diagnostics();
    assert_eq!(diagnostic.retained, 2);
    assert_eq!(diagnostic.high_water, 2);
    assert_eq!(diagnostic.checked_out, 0);
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    first_server.join();
    second_server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    drop(directory);
}

#[test]
fn returned_process_session_wakes_and_completes_its_queued_successor() {
    let (mut fixture, sessions) = fixture(189);
    let active = fixture.submit_text(" queued successor predecessor");
    let source = fixture.activate_without_terminal(active);
    fixture.mark_active_unknown_terminal(active, &source);
    let first_input = fixture.accept_text(" first queued process input");
    let second_input = fixture.accept_text(" second queued process input");
    fixture.advance_clock_to(62_102);
    fixture.complete_active_without_assistant(active, &source);
    let server = controlled_server(
        &fixture,
        fixture.thread,
        &[
            " first queued process input",
            " second queued process input",
        ],
    );
    install_session(
        &fixture,
        &sessions,
        fixture.thread,
        server.endpoint(),
        71_005,
    );
    assert!(server.wait_for_turn_start(0));
    let first_turn = {
        let home = fixture.store.live_home_command().unwrap();
        fixture
            .storage
            .thread(home.home(), fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .committed_tail()
            .unwrap()
    };
    assert_ne!(first_turn, active.turn);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    server.finish_turn();
    assert!(server.wait_for_turn_start(1));
    {
        let home = fixture.store.live_home_command().unwrap();
        assert_eq!(
            fixture
                .storage
                .turn_state(home.home(), first_turn, point_limit())
                .unwrap()
                .unwrap()
                .lifecycle(),
            TurnLifecycle::Complete
        );
        assert_eq!(
            accepted_route_state(
                home.home(),
                &fixture.storage,
                &NextRecordIds {
                    thread: fixture.thread,
                    parent: active.turn,
                    accepted_input: first_input,
                }
            ),
            AcceptedRouteEffectiveState::Promoted
        );
    }
    assert_eq!(sessions.diagnostics().retained, 1);
    assert_eq!(sessions.diagnostics().checked_out, 1);
    server.finish_turn();
    await_terminal(
        &fixture,
        NextRecordIds {
            thread: fixture.thread,
            parent: first_turn,
            accepted_input: second_input,
        },
    );
    wait_until("successor returns the same retained session", || {
        (sessions.diagnostics().available == 1).then_some(())
    });
    assert_eq!(sessions.diagnostics().high_water, 1);
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    drop(directory);
}

#[test]
fn provider_owner_fence_keeps_winning_work_and_discards_its_late_return() {
    let (mut fixture, sessions) = fixture(185);
    let ids = seed_runtime_next_input_without_wake(&mut fixture, 185);
    let server = resume_server(&fixture, ids.thread);
    let barrier = install_scheduled_promotion_barrier(ids.thread);
    install_session(&fixture, &sessions, ids.thread, server.endpoint(), 71_003);
    assert!(barrier.wait_until_paused(TIMEOUT));
    assert_eq!(sessions.diagnostics().checked_out, 1);
    sessions.close();
    let fenced = sessions.diagnostics();
    assert!(fenced.closed);
    assert_eq!(fenced.retained, 1);
    assert_eq!(fenced.retiring, 1);
    assert_eq!(fenced.checked_out, 1);
    barrier.release();
    server.wait_for_projection();
    await_terminal(&fixture, ids);
    wait_until("closed owner settles its exact late return", || {
        (sessions.diagnostics().retained == 0).then_some(())
    });
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert_eq!(sessions.diagnostics().available, 0);
    drop(directory);
}

#[test]
fn service_shutdown_joins_a_production_checkout_at_the_promotion_cut() {
    let (mut fixture, sessions) = fixture(187);
    let ids = seed_runtime_next_input_without_wake(&mut fixture, 187);
    let server = NormalTerminalServer::spawn_admission_only();
    let barrier = install_scheduled_promotion_barrier(ids.thread);
    install_session(&fixture, &sessions, ids.thread, server.endpoint(), 71_004);
    server.wait_for_admission();
    assert!(barrier.wait_until_paused(TIMEOUT));
    assert_eq!(sessions.diagnostics().checked_out, 1);
    let (directory, service) = fixture.into_service();
    let close = thread::spawn(move || service.close());
    wait_until("service closure fences provider issuance", || {
        sessions.diagnostics().closed.then_some(())
    });
    assert!(!close.is_finished());
    assert_eq!(sessions.diagnostics().checked_out, 1);
    barrier.release();
    assert!(matches!(
        close.join().unwrap().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    drop(directory);
}

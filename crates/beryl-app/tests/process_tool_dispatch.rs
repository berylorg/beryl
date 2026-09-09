#![cfg(feature = "test-faults")]

#[path = "accepted_next_scheduler/support.rs"]
mod scheduler_support;
#[path = "lifecycle_yield/server.rs"]
mod server;
#[path = "lifecycle_yield/support.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;

use std::{path::Path, sync::Arc, thread};

use beryl_app::{
    cas_projection::{
        OrdinaryTurnExecutionOutcome, ProcessScheduledExecutionProvider,
        ProjectionConnectionServiceCloseOutcome, ScheduledOrdinaryRequestPolicy,
    },
    lifecycle_attention::{LifecycleAttentionKind, ProcessLifecycleAttentionPool},
};
use beryl_backend::ManagedBackendClientConnector;
use beryl_model::CasProcessGeneration;
use syndic_storage::TurnLifecycle;

use server::{AUTHORIZATION, SUBMITTED_TEXT, TIMEOUT, YieldServer};
use syndic::{Fixture, point_limit};

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[test]
fn owned_tool_dispatch_accepts_lifecycle_and_refuses_deferred_branch_without_mutation() {
    let mut fixture = Fixture::new(233);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let mut tools = fixture.store.ordinary_dynamic_tool_authority(&pool);
    assert_eq!(Arc::strong_count(&pool), 1);
    let result = thread::scope(|scope| {
        let worker =
            scope.spawn(|| support::execute_with_authority(&fixture, projection, &mut tools));
        server.wait_started();
        assert_eq!(server.call("phase_needs_review")["result"]["success"], true);
        let revision = fixture.home().home_revision().unwrap();
        let response = server.resolve_branch();
        assert_eq!(response["result"]["success"], false);
        let serialized = response.to_string();
        assert!(serialized.contains("Branch discussion resolution is unavailable."));
        assert!(!serialized.contains("private resolution"));
        assert!(serialized.len() < 512);
        assert_eq!(fixture.home().home_revision().unwrap(), revision);
        assert_eq!(server.call("plan_complete")["result"]["success"], false);
        assert!(pool.snapshot().is_empty());
        server.finish(false);
        worker.join().unwrap().unwrap()
    });
    assert!(matches!(
        result,
        OrdinaryTurnExecutionOutcome::Terminal { .. }
    ));
    let records = pool.snapshot();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].thread_id(), fixture.thread);
    assert_eq!(records[0].turn_id(), submitted.turn);
    assert_eq!(records[0].kind(), LifecycleAttentionKind::ReviewReady);
    drop(result);
    session.invalidate_connection();
    drop(session);
    server.join();
    close_fixture(fixture);
    assert!(pool.acknowledge(records[0].token()));
    assert!(pool.snapshot().is_empty());
    drop(tools);
}

#[test]
fn foreign_and_retired_tool_owners_cannot_accept_matching_thread_and_turn_ids() {
    for retire in [false, true] {
        let mut fixture = Fixture::new(234);
        let submitted = fixture.submit_text(SUBMITTED_TEXT);
        let mut foreign = Fixture::new(234);
        let foreign_submitted = foreign.submit_text(SUBMITTED_TEXT);
        assert_eq!(fixture.thread, foreign.thread);
        assert_eq!(submitted.turn, foreign_submitted.turn);
        let pool = Arc::new(ProcessLifecycleAttentionPool::new());
        let mut tools = foreign.store.ordinary_dynamic_tool_authority(&pool);
        let mut foreign = Some(foreign);
        if retire {
            close_fixture(foreign.take().unwrap());
        }
        let server = YieldServer::spawn();
        let (session, projection) = support::obtain(&fixture, &server);
        let result = thread::scope(|scope| {
            let worker =
                scope.spawn(|| support::execute_with_authority(&fixture, projection, &mut tools));
            server.wait_started();
            assert_eq!(server.call("plan_complete")["result"]["success"], false);
            assert_eq!(server.resolve_branch()["result"]["success"], false);
            server.finish(false);
            worker.join().unwrap().unwrap()
        });
        assert!(pool.snapshot().is_empty());
        if let Some(foreign) = foreign {
            assert_eq!(
                foreign
                    .store
                    .take_terminal_lifecycle_yield_outcome(foreign.thread, foreign_submitted.turn)
                    .unwrap(),
                None
            );
            close_fixture(foreign);
        }
        drop(result);
        session.invalidate_connection();
        drop(session);
        server.join();
        close_fixture(fixture);
        drop(tools);
    }
}

#[test]
fn scheduled_checkout_lends_production_tool_authority_and_releases_it_on_retirement() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let mut fixture = Fixture::new_with_scheduled_provider(235, move |_| Box::new(provider));
    let ids = scheduler_support::seed_runtime_next_input_without_wake(&mut fixture, 235);
    let cas_thread =
        scheduler_support::current_cas_thread_id(&fixture.home(), &fixture.storage, ids.thread);
    let server = YieldServer::spawn_resume(cas_thread);
    let connector =
        ManagedBackendClientConnector::for_lifecycle_test(server.endpoint(), AUTHORIZATION);
    let session = fixture
        .store
        .admit_lifecycle_test_candidate(
            &connector,
            syndic::execution_binding().runtime_id(),
            CasProcessGeneration::new(980_001).unwrap(),
            Path::new(EXECUTION_ROOT),
            TIMEOUT,
        )
        .unwrap();
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    sessions
        .register(
            ids.thread,
            syndic::execution_binding(),
            session,
            ScheduledOrdinaryRequestPolicy::backend_defaults(
                fixture.state.settings(),
                Some(1_000_000),
                TIMEOUT,
                TIMEOUT,
            ),
            fixture.state.assets(),
            Box::new(fixture.store.ordinary_dynamic_tool_authority(&pool)),
        )
        .unwrap();
    server.wait_started();
    assert_eq!(sessions.diagnostics().checked_out, 1);
    assert_eq!(server.resolve_branch()["result"]["success"], false);
    assert_eq!(server.call("plan_complete")["result"]["success"], true);
    assert!(pool.snapshot().is_empty());
    server.finish(false);
    scheduler_support::wait_until("owned tools return from scheduled capture", || {
        let home = fixture.home();
        let turn = fixture
            .storage
            .thread(&home, ids.thread, point_limit())
            .ok()??
            .committed_tail()?;
        let state = fixture
            .storage
            .turn_state(&home, turn, point_limit())
            .ok()??;
        (turn != ids.parent
            && state.lifecycle() == TurnLifecycle::Complete
            && sessions.diagnostics().available == 1
            && pool.snapshot().len() == 1)
            .then_some(())
    });
    let records = pool.snapshot();
    assert_eq!(records[0].kind(), LifecycleAttentionKind::PlanComplete);
    assert_eq!(records[0].thread_id(), ids.thread);
    close_fixture(fixture);
    server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    assert_eq!(Arc::strong_count(&pool), 1);
    assert!(pool.acknowledge(records[0].token()));
}

fn close_fixture(fixture: Fixture) {
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}

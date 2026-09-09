#![cfg(feature = "test-faults")]

#[path = "lifecycle_yield/continuation_attention.rs"]
mod continuation_attention;
#[path = "lifecycle_yield/custody.rs"]
mod custody;
#[path = "lifecycle_yield/server.rs"]
mod server;
#[path = "lifecycle_yield/support.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;
#[path = "lifecycle_yield/work_facts.rs"]
mod work_facts;

use std::{sync::Arc, thread};

use beryl_app::{
    LifecycleYieldOutcome,
    cas_projection::OrdinaryTurnExecutionOutcome,
    lifecycle_attention::{LifecycleAttentionKind, ProcessLifecycleAttentionPool},
    main_window::NOTICE_RECORD_CAPACITY,
};
use beryl_model::SyndicTurnId;
use syndic_storage::TurnLifecycle;

use server::{SUBMITTED_TEXT, YieldServer};
use syndic::{Fixture, point_limit};

pub(crate) const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[test]
fn accepted_terminal_outcomes_report_once_after_exact_turn_convergence() {
    for (tag, outcome, kind) in [
        (
            221,
            LifecycleYieldOutcome::PhaseNeedsReview,
            LifecycleAttentionKind::ReviewReady,
        ),
        (
            222,
            LifecycleYieldOutcome::BlockedNeedsOperator,
            LifecycleAttentionKind::OperatorAttention,
        ),
        (
            223,
            LifecycleYieldOutcome::PlanComplete,
            LifecycleAttentionKind::PlanComplete,
        ),
    ] {
        run_terminal_case(tag, outcome, kind, false);
    }
}

#[test]
fn durably_incomplete_turn_reports_the_accepted_attention_without_claiming_success() {
    run_terminal_case(
        224,
        LifecycleYieldOutcome::PhaseNeedsReview,
        LifecycleAttentionKind::ReviewReady,
        true,
    );
}

fn run_terminal_case(
    tag: u8,
    outcome: LifecycleYieldOutcome,
    kind: LifecycleAttentionKind,
    incomplete: bool,
) {
    let mut fixture = Fixture::new(tag);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
    let result = thread::scope(|scope| {
        let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
        server.wait_started();
        let response = server.call(outcome.as_str());
        assert_eq!(response["result"]["success"], true);
        assert!(pool.snapshot().is_empty());
        let duplicate = server.call("phase_continue");
        assert_eq!(duplicate["result"]["success"], false);
        assert!(pool.snapshot().is_empty());
        server.finish(incomplete);
        worker.join().unwrap().unwrap()
    });
    assert_eq!(
        matches!(result, OrdinaryTurnExecutionOutcome::Incomplete { .. }),
        incomplete
    );
    let state = fixture
        .storage
        .turn_state(&fixture.home(), submitted.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        state.lifecycle(),
        if incomplete {
            TurnLifecycle::Incomplete
        } else {
            TurnLifecycle::Complete
        }
    );
    let records = pool.snapshot();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].home_id(), fixture.home().home_id());
    assert_eq!(records[0].thread_id(), fixture.thread);
    assert_eq!(records[0].turn_id(), submitted.turn);
    assert_eq!(records[0].outcome(), outcome);
    assert_eq!(records[0].kind(), kind);
    assert_eq!(
        fixture
            .store
            .take_terminal_lifecycle_yield_outcome(fixture.thread, submitted.turn)
            .unwrap(),
        None
    );
    assert!(pool.acknowledge(records[0].token()));
    assert!(pool.snapshot().is_empty());
    drop(result);
    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}

#[test]
fn cancellation_preserves_first_winner_and_prevents_later_outcome_replacement() {
    let mut fixture = Fixture::new(225);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
    let result = thread::scope(|scope| {
        let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
        server.wait_started();
        assert_eq!(server.call("phase_continue")["result"]["success"], true);
        fixture
            .store
            .cancel_selected_continuation_for_window_close(fixture.thread)
            .unwrap();
        assert_eq!(server.call("plan_complete")["result"]["success"], false);
        server.finish(false);
        worker.join().unwrap().unwrap()
    });
    assert!(matches!(
        result,
        OrdinaryTurnExecutionOutcome::Terminal { .. }
    ));
    assert!(pool.snapshot().is_empty());
    assert_eq!(
        fixture
            .store
            .take_terminal_lifecycle_yield_outcome(fixture.thread, submitted.turn)
            .unwrap(),
        None
    );
    drop(result);
    session.invalidate_connection();
    drop(session);
    server.join();
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}

#[test]
fn attention_capacity_and_disposal_do_not_change_yield_acceptance() {
    for closed in [false, true] {
        let mut fixture = Fixture::new(if closed { 227 } else { 226 });
        fixture.submit_text(SUBMITTED_TEXT);
        let server = YieldServer::spawn();
        let (session, projection) = support::obtain(&fixture, &server);
        let pool = Arc::new(ProcessLifecycleAttentionPool::new());
        if closed {
            pool.close();
        } else {
            for index in 0..NOTICE_RECORD_CAPACITY {
                let attempt = pool
                    .track_accepted_yield(
                        fixture.home().home_id(),
                        fixture.thread,
                        SyndicTurnId::from_bytes([index as u8; 16]),
                        LifecycleYieldOutcome::PlanComplete,
                    )
                    .unwrap();
                pool.report_terminal(&attempt);
            }
        }
        let mut lifecycle = fixture.store.lifecycle_yield_handler(&pool);
        let result = thread::scope(|scope| {
            let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
            server.wait_started();
            assert_eq!(server.call("plan_complete")["result"]["success"], true);
            server.finish(false);
            worker.join().unwrap().unwrap()
        });
        assert!(matches!(
            result,
            OrdinaryTurnExecutionOutcome::Terminal { .. }
        ));
        assert_eq!(
            pool.snapshot().len(),
            if closed { 0 } else { NOTICE_RECORD_CAPACITY }
        );
        assert_eq!(pool.diagnostics().omitted, if closed { 0 } else { 1 });
        drop(result);
        session.invalidate_connection();
        drop(session);
        server.join();
        let (directory, service) = fixture.into_service();
        assert!(matches!(
            service.close().unwrap(),
            beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
        ));
        drop(directory);
    }
}

#[test]
fn foreign_home_handler_rejects_matching_eligible_thread_and_turn_ids() {
    let mut fixture = Fixture::new(228);
    let submitted = fixture.submit_text(SUBMITTED_TEXT);
    let mut foreign = Fixture::new(228);
    let foreign_submitted = foreign.submit_text(SUBMITTED_TEXT);
    assert_eq!(fixture.thread, foreign.thread);
    assert_eq!(submitted.turn, foreign_submitted.turn);
    assert_ne!(fixture.home().home_id(), foreign.home().home_id());
    assert_eq!(
        foreign
            .storage
            .turn_state(&foreign.home(), foreign_submitted.turn, point_limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        TurnLifecycle::Pending,
    );
    let server = YieldServer::spawn();
    let (session, projection) = support::obtain(&fixture, &server);
    let pool = Arc::new(ProcessLifecycleAttentionPool::new());
    let mut lifecycle = foreign.store.lifecycle_yield_handler(&pool);
    let result = thread::scope(|scope| {
        let worker = scope.spawn(|| support::execute(&fixture, projection, &mut lifecycle));
        server.wait_started();
        assert_eq!(server.call("plan_complete")["result"]["success"], false);
        server.finish(false);
        worker.join().unwrap().unwrap()
    });
    assert!(pool.snapshot().is_empty());
    assert_eq!(
        foreign
            .store
            .take_terminal_lifecycle_yield_outcome(foreign.thread, foreign_submitted.turn)
            .unwrap(),
        None
    );
    drop(result);
    session.invalidate_connection();
    drop(session);
    server.join();
    for fixture in [fixture, foreign] {
        let (directory, service) = fixture.into_service();
        assert!(matches!(
            service.close().unwrap(),
            beryl_app::cas_projection::ProjectionConnectionServiceCloseOutcome::Closed
        ));
        drop(directory);
    }
}

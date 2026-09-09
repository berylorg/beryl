#![cfg(feature = "test-faults")]

#[allow(dead_code)]
#[path = "accepted_next_scheduler/support.rs"]
mod scheduler_support;
#[path = "attempt_policy/server.rs"]
mod server;
#[path = "attempt_policy/support.rs"]
mod support;
#[path = "projection/syndic.rs"]
mod syndic;

use beryl_app::cas_projection::{
    OrdinaryTurnExecutionError, OrdinaryTurnExecutionFailure, OrdinaryTurnExecutionRequest,
    ProcessScheduledExecutionProvider, ScheduledOrdinaryRequestPolicy,
    test_faults::install_scheduled_promotion_barrier,
};
use beryl_backend::TurnStartOptions;
use serde_json::json;
use syndic_storage::TurnLifecycle;

use server::{AttemptServer, SUBMITTED_TEXT, TIMEOUT, hidden_context};
use support::{admit, apply_instructions, execute, obtain, rejected_projection};
use syndic::Fixture;

const EXECUTION_ROOT: &str = r"C:\work\beryl";

#[test]
fn each_retry_reads_latest_instructions_and_blank_explicitly_clears_hidden_context() {
    let mut fixture = Fixture::new(191);
    fixture.submit_text(SUBMITTED_TEXT);
    apply_instructions(&fixture, "old construction-time instructions");
    let request = OrdinaryTurnExecutionRequest::backend_defaults(fixture.state.settings(), TIMEOUT);
    let server = AttemptServer::spawn(
        "thread/start",
        "policy-retries".into(),
        "thread-model",
        Some("medium"),
        vec![
            hidden_context(
                "thread-model",
                Some("medium"),
                Some("  Latest Ω instructions.\n"),
            ),
            hidden_context("thread-model", Some("medium"), Some("Changed for retry")),
            hidden_context("thread-model", Some("medium"), None),
        ],
        false,
    );
    let mut session = admit(&fixture, &server, 950_191);
    let mut projection = obtain(&fixture, &mut session, fixture.thread);
    let generation = projection.loaded_session_generation();
    for instructions in ["  Latest Ω instructions.\n", "Changed for retry", " \t\n"] {
        apply_instructions(&fixture, instructions);
        projection = rejected_projection(execute(&fixture, projection, &request).unwrap());
        assert_eq!(projection.loaded_session_generation(), generation);
        assert!(projection.is_live().unwrap());
    }
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
}

#[test]
fn absent_instructions_clear_hidden_context_and_unknown_reasoning_stays_omitted() {
    let mut fixture = Fixture::new(192);
    fixture.submit_text(SUBMITTED_TEXT);
    let request = OrdinaryTurnExecutionRequest::backend_defaults(fixture.state.settings(), TIMEOUT);
    let server = AttemptServer::spawn(
        "thread/start",
        "policy-absent".into(),
        "unknown-effort-model",
        None,
        vec![hidden_context("unknown-effort-model", None, None)],
        false,
    );
    let mut session = admit(&fixture, &server, 950_192);
    let projection = obtain(&fixture, &mut session, fixture.thread);
    let projection = rejected_projection(execute(&fixture, projection, &request).unwrap());
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
}

#[test]
fn foreign_settings_fail_before_activation_and_preserve_projection_for_corrected_retry() {
    let mut fixture = Fixture::new(193);
    let foreign = Fixture::new(194);
    fixture.submit_text(SUBMITTED_TEXT);
    apply_instructions(&foreign, "foreign instructions");
    apply_instructions(&fixture, "correct home");
    let request = OrdinaryTurnExecutionRequest::backend_defaults(foreign.state.settings(), TIMEOUT);
    let server = AttemptServer::spawn(
        "thread/start",
        "policy-home".into(),
        "home-model",
        Some("high"),
        vec![hidden_context(
            "home-model",
            Some("high"),
            Some("correct home"),
        )],
        false,
    );
    let mut session = admit(&fixture, &server, 950_193);
    let projection = obtain(&fixture, &mut session, fixture.thread);
    let revision = projection.binding_revision();
    let OrdinaryTurnExecutionFailure::PreActivation {
        projection,
        source: OrdinaryTurnExecutionError::HomeRead(_),
    } = execute(&fixture, projection, &request).unwrap_err()
    else {
        panic!("expected typed settings read failure")
    };
    assert_eq!(projection.binding_revision(), revision);
    assert!(projection.is_live().unwrap());
    let corrected =
        OrdinaryTurnExecutionRequest::backend_defaults(fixture.state.settings(), TIMEOUT);
    let projection = rejected_projection(execute(&fixture, *projection, &corrected).unwrap());
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
}

fn fixed_override_invalidates_shared_defaults(
    seed: u8,
    options: TurnStartOptions,
    expected: serde_json::Value,
) {
    let mut fixture = Fixture::new(seed);
    fixture.submit_text(SUBMITTED_TEXT);
    let server = AttemptServer::spawn(
        "thread/start",
        "policy-fixed".into(),
        "original-model",
        Some("high"),
        vec![expected],
        false,
    );
    let mut session = admit(&fixture, &server, 960_000 + u64::from(seed));
    let projection = obtain(&fixture, &mut session, fixture.thread);
    let observer = obtain(&fixture, &mut session, fixture.thread);
    let fixed = OrdinaryTurnExecutionRequest::new(options, TIMEOUT);
    let projection = rejected_projection(execute(&fixture, projection, &fixed).unwrap());
    assert!(
        observer
            .observed_thread_metadata()
            .unwrap()
            .unwrap()
            .model
            .is_none()
    );
    let revision = projection.binding_revision();
    let defaults =
        OrdinaryTurnExecutionRequest::backend_defaults(fixture.state.settings(), TIMEOUT);
    let OrdinaryTurnExecutionFailure::PreActivation {
        projection,
        source: OrdinaryTurnExecutionError::BackendDefaultPolicyUnavailable,
    } = execute(&fixture, projection, &defaults).unwrap_err()
    else {
        panic!("expected unavailable backend defaults")
    };
    assert_eq!(projection.binding_revision(), revision);
    assert!(projection.is_live().unwrap());
    session.invalidate_connection();
    drop(observer);
    drop(projection);
    drop(session);
    server.join();
}

#[test]
fn fixed_model_override_invalidates_shared_default_observation() {
    fixed_override_invalidates_shared_defaults(
        195,
        TurnStartOptions::default().with_model("override-model"),
        json!({"model":"override-model"}),
    );
}

#[test]
fn fixed_reasoning_override_invalidates_shared_default_observation() {
    fixed_override_invalidates_shared_defaults(
        196,
        TurnStartOptions::default().with_reasoning_effort("low"),
        json!({"effort":"low"}),
    );
}

#[test]
fn fixed_collaboration_context_invalidates_shared_default_observation() {
    fixed_override_invalidates_shared_defaults(
        197,
        TurnStartOptions::default().with_developer_instructions_context(
            Some("fixed context".into()),
            "context-model",
            Some("medium".into()),
        ),
        hidden_context("context-model", Some("medium"), Some("fixed context")),
    );
}

#[test]
fn replacement_start_reuses_settings_capability_but_reads_new_values_and_backend_defaults() {
    let mut fixture = Fixture::new(198);
    fixture.submit_text(SUBMITTED_TEXT);
    let request = OrdinaryTurnExecutionRequest::backend_defaults(fixture.state.settings(), TIMEOUT);
    let server = AttemptServer::spawn(
        "thread/start",
        "policy-replacement".into(),
        "before-model",
        Some("low"),
        vec![hidden_context("before-model", Some("low"), Some("before"))],
        false,
    );
    apply_instructions(&fixture, "before");
    let mut session = admit(&fixture, &server, 950_198);
    let projection = obtain(&fixture, &mut session, fixture.thread);
    let projection = rejected_projection(execute(&fixture, projection, &request).unwrap());
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();

    apply_instructions(&fixture, "after");
    let server = AttemptServer::spawn(
        "thread/resume",
        "policy-replacement".into(),
        "after-model",
        None,
        vec![hidden_context("after-model", None, Some("after"))],
        false,
    );
    let mut session = admit(&fixture, &server, 950_199);
    let projection = obtain(&fixture, &mut session, fixture.thread);
    let projection = rejected_projection(execute(&fixture, projection, &request).unwrap());
    session.invalidate_connection();
    drop(projection);
    drop(session);
    server.join();
}

#[test]
fn production_scheduled_checkout_uses_instructions_applied_after_registration() {
    let (provider, sessions) = ProcessScheduledExecutionProvider::new();
    let mut fixture = Fixture::new_with_scheduled_provider(199, move |_| Box::new(provider));
    let ids = scheduler_support::seed_runtime_next_input_without_wake(&mut fixture, 199);
    apply_instructions(&fixture, "registered value");
    let policy = ScheduledOrdinaryRequestPolicy::backend_defaults(
        fixture.state.settings(),
        Some(1_000_000),
        TIMEOUT,
        TIMEOUT,
    );
    let cas_thread =
        scheduler_support::current_cas_thread_id(&*fixture.home(), &fixture.storage, ids.thread);
    let server = AttemptServer::spawn(
        "thread/resume",
        cas_thread,
        "scheduled-model",
        Some("high"),
        vec![hidden_context(
            "scheduled-model",
            Some("high"),
            Some("latest scheduled value"),
        )],
        true,
    );
    let session = admit(&fixture, &server, 950_200);
    let barrier = install_scheduled_promotion_barrier(ids.thread);
    sessions
        .register(
            ids.thread,
            syndic::execution_binding(),
            session,
            policy,
            fixture.state.assets(),
            scheduler_support::tool_authority(),
        )
        .unwrap();
    assert!(barrier.wait_until_paused(TIMEOUT));
    assert_eq!(sessions.diagnostics().checked_out, 1);
    apply_instructions(&fixture, "latest scheduled value");
    barrier.release();
    scheduler_support::wait_until("scheduled default policy completes", || {
        let home = fixture.home();
        let thread = fixture
            .storage
            .thread(&home, ids.thread, scheduler_support::point_limit())
            .ok()??;
        let turn = thread.committed_tail()?;
        let state = fixture
            .storage
            .turn_state(&home, turn, scheduler_support::point_limit())
            .ok()??;
        (turn != ids.parent
            && state.lifecycle() == TurnLifecycle::Complete
            && sessions.diagnostics().available == 1)
            .then_some(())
    });
    let (directory, service) = fixture.into_service();
    service.close().unwrap();
    server.join();
    assert_eq!(sessions.diagnostics().retained, 0);
    drop(directory);
}

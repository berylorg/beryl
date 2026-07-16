use beryl_backend::{
    ManagedBackendError, NonIdempotentRequestOutcome, ThreadLoadOptions, UserInput,
};
use beryl_model::{CasThreadId, CasTurnId};
use serde_json::json;

#[path = "support/cas_lineage.rs"]
#[allow(
    dead_code,
    reason = "the shared fixture also serves the sibling injection integration binary"
)]
mod cas_lineage_support;

use cas_lineage_support::*;

#[test]
fn lineage_mutations_reject_wrong_or_reused_thread_identities() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);

        let resume = read_json(&mut socket);
        assert_eq!(resume["method"], json!("thread/resume"));
        send_result(
            &mut socket,
            2,
            lineage_result("thread_wrong", json!({ "type": "idle" })),
        );

        let fork = read_json(&mut socket);
        assert_eq!(fork["method"], json!("thread/fork"));
        send_result(
            &mut socket,
            3,
            lineage_result("thread_source", json!({ "type": "idle" })),
        );

        let rollback = read_json(&mut socket);
        assert_eq!(rollback["method"], json!("thread/rollback"));
        send_result(
            &mut socket,
            4,
            lineage_result("thread_wrong", json!({ "type": "idle" })),
        );
    });

    let mut client = connect(endpoint);
    let source = CasThreadId::new("thread_source").unwrap();
    let options = ThreadLoadOptions::for_root(EXECUTION_ROOT);

    let resume_error = client
        .resume_thread(&source, &options, REQUEST_TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        resume_error,
        ManagedBackendError::ThreadResponseIdentityMismatch {
            ref method,
            ref expected,
            ref actual,
        } if method == "thread/resume"
            && expected.as_str() == "thread_source"
            && actual.as_str() == "thread_wrong"
    ));

    let fork_error = client
        .fork_thread(&source, &options, REQUEST_TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        fork_error,
        ManagedBackendError::ForkResponseReusedSource {
            ref method,
            ref source_thread,
        } if method == "thread/fork" && source_thread.as_str() == "thread_source"
    ));

    let rollback_error = client
        .rollback_thread(&source, 1, REQUEST_TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        rollback_error,
        ManagedBackendError::ThreadResponseIdentityMismatch {
            ref method,
            ref expected,
            ref actual,
        } if method == "thread/rollback"
            && expected.as_str() == "thread_source"
            && actual.as_str() == "thread_wrong"
    ));

    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn steering_rejects_a_returned_turn_other_than_the_expected_active_turn() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);

        let steer = read_json(&mut socket);
        assert_eq!(steer["method"], json!("turn/steer"));
        assert_eq!(steer["params"]["expectedTurnId"], json!("turn_expected"));
        send_result(&mut socket, 2, json!({ "turnId": "turn_wrong" }));
    });

    let mut client = connect(endpoint);
    let thread_id = CasThreadId::new("thread_source").unwrap();
    let expected_turn_id = CasTurnId::new("turn_expected").unwrap();
    let outcome = client.steer_turn_with_user_input(
        &thread_id,
        &expected_turn_id,
        vec![UserInput::text("steer exactly")],
        REQUEST_TIMEOUT,
    );

    assert!(matches!(
        outcome,
        NonIdempotentRequestOutcome::CompletionUnknown { error }
            if matches!(
                *error,
                ManagedBackendError::TurnResponseIdentityMismatch {
                    ref method,
                    ref expected,
                    ref actual,
                } if method == "turn/steer"
            && expected.as_str() == "turn_expected"
            && actual.as_str() == "turn_wrong"
            )
    ));

    client.shutdown().unwrap();
    server.join().unwrap();
}

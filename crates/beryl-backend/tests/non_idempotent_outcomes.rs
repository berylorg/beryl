use std::{thread, time::Duration};

use beryl_backend::{
    ManagedBackendError, NonIdempotentRequestOutcome, NonSteerableTurnKind, UserInput,
    active_turn_not_steerable_error,
};
use beryl_model::{CasThreadId, CasTurnId};
use serde_json::json;

#[path = "support/cas_lineage.rs"]
#[allow(
    dead_code,
    reason = "the shared fixture also serves the sibling lineage integration binaries"
)]
mod cas_lineage_support;

use cas_lineage_support::*;

fn thread_id() -> CasThreadId {
    CasThreadId::new("thread_outcome").unwrap()
}

fn turn_id() -> CasTurnId {
    CasTurnId::new("turn_active").unwrap()
}

#[test]
fn closed_transport_proves_turn_start_was_not_dispatched() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);
    });
    let mut client = connect(endpoint);
    client.shutdown().unwrap();
    server.join().unwrap();

    let outcome = client.start_turn(&thread_id(), "must remain undispatched", REQUEST_TIMEOUT);

    assert!(matches!(
        outcome,
        NonIdempotentRequestOutcome::ProvenNotDispatched { error }
            if matches!(*error, ManagedBackendError::TransportClosed { ref method }
                if method == "turn/start")
    ));
}

#[test]
fn withheld_turn_start_response_is_completion_unknown() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("turn/start"));
        thread::sleep(Duration::from_millis(100));
    });
    let mut client = connect(endpoint);

    let outcome = client.start_turn(
        &thread_id(),
        "response will be withheld",
        Duration::from_millis(25),
    );

    assert!(matches!(
        outcome,
        NonIdempotentRequestOutcome::CompletionUnknown { error }
            if matches!(*error, ManagedBackendError::RequestTimeout { ref method, .. }
                if method == "turn/start")
    ));
    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn transport_loss_after_turn_steer_write_is_completion_unknown() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("turn/steer"));
        assert_eq!(request["params"]["expectedTurnId"], json!("turn_active"));
    });
    let mut client = connect(endpoint);

    let outcome = client.steer_turn_with_user_input(
        &thread_id(),
        &turn_id(),
        vec![UserInput::text("written before connection loss")],
        REQUEST_TIMEOUT,
    );

    let NonIdempotentRequestOutcome::CompletionUnknown { error } = outcome else {
        panic!("post-write transport loss must be completion-unknown");
    };
    assert!(error.invalidates_connection_authority());
    client.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn exact_non_steerable_rejection_preserves_machine_readable_detail() {
    let (endpoint, server) = spawn_fake_app_server(|mut socket| {
        expect_initialize(&mut socket);
        let request = read_json(&mut socket);
        assert_eq!(request["method"], json!("turn/steer"));
        send_error(
            &mut socket,
            2,
            -32_000,
            "wording supplies no classification authority",
            Some(json!({
                "codexErrorInfo": {
                    "activeTurnNotSteerable": {
                        "turnKind": "compact"
                    }
                }
            })),
        );
    });
    let mut client = connect(endpoint);

    let outcome = client.steer_turn_with_user_input(
        &thread_id(),
        &turn_id(),
        vec![UserInput::text("exactly rejected steering")],
        REQUEST_TIMEOUT,
    );

    let NonIdempotentRequestOutcome::ExactRejection { error } = outcome else {
        panic!("matching JSON-RPC error must remain an exact rejection");
    };
    assert_eq!(error.code, -32_000);
    assert_eq!(
        active_turn_not_steerable_error(&error).unwrap().turn_kind,
        NonSteerableTurnKind::Compact
    );
    client.shutdown().unwrap();
    server.join().unwrap();
}

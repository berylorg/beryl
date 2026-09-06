use std::{path::Path, time::Duration};

use beryl_backend::{ForegroundIngressError, ManagedBackendError, ThreadLoadOptions};
use beryl_model::{CasThreadId, CasTurnId};

use super::{
    send_lineage_response,
    support::{
        CONFIG_CWD, TIMEOUT, assert_initialize, assert_initialized, connector, expect_close,
        foreground_config, read_json, send_initialize_response, send_json, spawn_server,
    },
};

#[test]
fn structured_rejection_is_distinct_and_leaves_the_session_reusable() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let rejected = read_json(socket).unwrap();
        assert_eq!(rejected["id"], 2);
        assert_eq!(rejected["method"], "thread/start");
        send_json(
            socket,
            r#"{"error":{"code":-32600,"message":"request rejected"},"id":2}"#,
        );
        let retry = read_json(socket).unwrap();
        assert_eq!(retry["id"], 3);
        assert_eq!(retry["method"], "thread/start");
        send_lineage_response(
            socket,
            3,
            "thread-after-rejection",
            r#"{"type":"idle"}"#,
            false,
        );
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();

    let error = session
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestFailed { method, .. } if method == "thread/start"
    ));
    assert!(!session.transport_is_closed_for_lifecycle_test());
    let started = session
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap();
    assert_eq!(started.thread_id().as_str(), "thread-after-rejection");
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn resume_identity_mismatch_retires_without_publishing_a_loaded_thread() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["method"], "thread/resume");
        send_lineage_response(socket, 2, "thread-wrong", r#"{"type":"idle"}"#, true);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let expected = CasThreadId::new("thread-expected").unwrap();
    let options = ThreadLoadOptions::for_root(CONFIG_CWD);
    let error = session
        .resume_thread(&expected, &options, TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ThreadResponseIdentityMismatch {
            method,
            expected: returned_expected,
            actual,
        } if method == "thread/resume"
            && returned_expected == expected
            && actual.as_str() == "thread-wrong"
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn fork_through_turn_source_reuse_retires_without_publishing_a_fresh_thread() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["method"], "thread/fork");
        assert_eq!(request["params"]["lastTurnId"], "turn-boundary");
        send_lineage_response(socket, 2, "thread-source", r#"{"type":"idle"}"#, false);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let source = CasThreadId::new("thread-source").unwrap();
    let turn = CasTurnId::new("turn-boundary").unwrap();
    let options = ThreadLoadOptions::for_root(CONFIG_CWD);
    let error = session
        .fork_thread_through_turn(&source, &turn, &options, TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ForkResponseReusedSource {
            method,
            source_thread,
        } if method == "thread/fork" && source_thread == source
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn uninitialized_and_request_only_sessions_reject_lineage_before_bytes() {
    let (foreground_endpoint, foreground_server) = spawn_server(|socket| {
        let initialize = read_json(socket).unwrap();
        assert_initialize(&initialize, false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let foreground_connector = connector(foreground_endpoint);
    let mut foreground = foreground_connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    let before = foreground.predispatch_state_for_lifecycle_test();
    let error = foreground
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(error, ManagedBackendError::ClientNotInitialized));
    assert_eq!(foreground.predispatch_state_for_lifecycle_test(), before);
    foreground.initialize_foreground(TIMEOUT).unwrap();
    foreground.shutdown().unwrap();
    foreground_server.join().unwrap();

    let (request_endpoint, request_server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let request_connector = connector(request_endpoint);
    let mut request_only = request_connector.connect_request_client(TIMEOUT).unwrap();
    let before = request_only.predispatch_state_for_lifecycle_test();
    let error = request_only
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestProfileMismatch {
            method: "thread/start",
            required_profile: "foreground",
        }
    ));
    assert_eq!(request_only.predispatch_state_for_lifecycle_test(), before);
    request_only.shutdown().unwrap();
    request_server.join().unwrap();
}

#[test]
fn proven_predispatch_write_failure_cancels_expectation_and_preserves_request_id() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let retry = read_json(socket).unwrap();
        assert_eq!(retry["id"], 2);
        assert_eq!(retry["method"], "thread/start");
        send_lineage_response(
            socket,
            2,
            "thread-after-write-failure",
            r#"{"type":"idle"}"#,
            false,
        );
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let before = session.predispatch_state_for_lifecycle_test();
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    let error = session
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::WriteRequest { method, .. } if method == "thread/start"
    ));
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
    assert!(!session.transport_is_closed_for_lifecycle_test());
    let retry = session
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap();
    assert_eq!(retry.thread_id().as_str(), "thread-after-write-failure");
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn exhausted_ids_and_occupied_expectations_fail_before_lineage_bytes() {
    let (exhausted_endpoint, exhausted_server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let exhausted_connector = connector(exhausted_endpoint);
    let mut exhausted = exhausted_connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    exhausted.initialize_foreground(TIMEOUT).unwrap();
    exhausted.exhaust_request_ids_for_lifecycle_test();
    let before = exhausted.predispatch_state_for_lifecycle_test();
    let error = exhausted
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestIdExhausted {
            method: "thread/start"
        }
    ));
    assert_eq!(exhausted.predispatch_state_for_lifecycle_test(), before);
    assert!(!exhausted.transport_is_closed_for_lifecycle_test());
    exhausted.shutdown().unwrap();
    exhausted_server.join().unwrap();

    let (occupied_endpoint, occupied_server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let occupied_connector = connector(occupied_endpoint);
    let mut occupied = occupied_connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    occupied.initialize_foreground(TIMEOUT).unwrap();
    occupied.prepare_pre_bind_response_wait_for_lifecycle_test(99);
    let before = occupied.predispatch_state_for_lifecycle_test();
    let error = occupied
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ResponseExpectationUnavailable {
            method: "thread/start"
        }
    ));
    assert_eq!(occupied.predispatch_state_for_lifecycle_test(), before);
    assert!(!occupied.transport_is_closed_for_lifecycle_test());
    occupied.shutdown().unwrap();
    occupied_server.join().unwrap();
}

#[test]
fn lineage_timeout_is_completion_unknown_and_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "thread/start");
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let request_timeout = Duration::from_millis(40);
    let error = session
        .start_thread(Path::new(CONFIG_CWD), request_timeout)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestTimeout { method, timeout }
            if method == "thread/start" && timeout == request_timeout
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn malformed_matching_lineage_response_is_consumed_and_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "thread/start");
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let error = session
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ForegroundIngress {
            method,
            source: ForegroundIngressError::MalformedResponse,
        } if method == "thread/start"
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn transport_loss_after_lineage_write_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "thread/start");
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let error = session
        .start_thread(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(error.invalidates_connection_authority());
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn required_lineage_identities_are_bounded_before_session_use() {
    assert!(CasThreadId::new("x".repeat(257)).is_err());
    assert!(CasTurnId::new("y".repeat(257)).is_err());
}

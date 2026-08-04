use std::time::Duration;

use beryl_backend::{ForegroundIngressError, ManagedBackendError};
use beryl_model::CasThreadId;

use super::{
    send_thread_read_response,
    support::{
        TIMEOUT, assert_initialize, assert_initialized, connector, expect_close, read_json,
        send_initialize_response, send_json, send_recognized_rejection, spawn_server,
    },
};

#[test]
fn structured_rejection_leaves_the_request_only_session_reusable() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let rejected = read_json(socket).unwrap();
        assert_eq!(rejected["id"], 2);
        assert_eq!(rejected["method"], "thread/read");
        send_recognized_rejection(socket, 2);
        let retry = read_json(socket).unwrap();
        assert_eq!(retry["id"], 3);
        assert_eq!(retry["method"], "thread/read");
        send_thread_read_response(
            socket,
            3,
            "thread-target",
            r#"{"type":"idle"}"#,
            None,
            "discarded",
        );
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    let target = CasThreadId::new("thread-target").unwrap();
    let error = session.read_thread_metadata(&target, TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestFailed { method, .. } if method == "thread/read"
    ));
    assert!(!session.transport_is_closed_for_lifecycle_test());
    let metadata = session.read_thread_metadata(&target, TIMEOUT).unwrap();
    assert_eq!(metadata.thread_id(), &target);
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn returned_identity_mismatch_retires_without_publishing_metadata() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["method"], "thread/read");
        send_thread_read_response(
            socket,
            2,
            "thread-wrong",
            r#"{"type":"idle"}"#,
            None,
            "discarded",
        );
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    let target = CasThreadId::new("thread-expected").unwrap();
    let error = session.read_thread_metadata(&target, TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ThreadResponseIdentityMismatch {
            method,
            expected,
            actual,
        } if method == "thread/read" && expected == target && actual.as_str() == "thread-wrong"
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn proven_predispatch_write_failure_preserves_id_and_expectation_for_retry() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let retry = read_json(socket).unwrap();
        assert_eq!(retry["id"], 2);
        assert_eq!(retry["method"], "thread/read");
        send_thread_read_response(
            socket,
            2,
            "thread-target",
            r#"{"type":"idle"}"#,
            None,
            "discarded",
        );
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    let target = CasThreadId::new("thread-target").unwrap();
    let before = session.predispatch_state_for_lifecycle_test();
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    let error = session.read_thread_metadata(&target, TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::WriteRequest { method, .. } if method == "thread/read"
    ));
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
    assert!(!session.transport_is_closed_for_lifecycle_test());
    let metadata = session.read_thread_metadata(&target, TIMEOUT).unwrap();
    assert_eq!(metadata.thread_id(), &target);
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn exhausted_occupied_and_poisoned_expectations_fail_before_request_bytes() {
    assert_predispatch_slot_failure(PredispatchFault::Exhausted);
    assert_predispatch_slot_failure(PredispatchFault::Occupied);
    assert_predispatch_slot_failure(PredispatchFault::Poisoned);
}

#[derive(Clone, Copy)]
enum PredispatchFault {
    Exhausted,
    Occupied,
    Poisoned,
}

fn assert_predispatch_slot_failure(fault: PredispatchFault) {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    match fault {
        PredispatchFault::Exhausted => session.exhaust_request_ids_for_lifecycle_test(),
        PredispatchFault::Occupied => {
            session.occupy_response_expectation_for_lifecycle_test(99);
        }
        PredispatchFault::Poisoned => session.poison_response_expectation_for_lifecycle_test(),
    }
    let before = session.predispatch_state_for_lifecycle_test();
    let target = CasThreadId::new("thread-target").unwrap();
    let error = session.read_thread_metadata(&target, TIMEOUT).unwrap_err();
    match fault {
        PredispatchFault::Exhausted => assert!(matches!(
            error,
            ManagedBackendError::RequestIdExhausted {
                method: "thread/read"
            }
        )),
        PredispatchFault::Occupied | PredispatchFault::Poisoned => assert!(matches!(
            error,
            ManagedBackendError::ResponseExpectationUnavailable {
                method: "thread/read"
            }
        )),
    }
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
    assert!(!session.transport_is_closed_for_lifecycle_test());
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn timeout_after_thread_read_write_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "thread/read");
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    let target = CasThreadId::new("thread-target").unwrap();
    let timeout = Duration::from_millis(40);
    let error = session.read_thread_metadata(&target, timeout).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestTimeout {
            method,
            timeout: actual,
        } if method == "thread/read" && actual == timeout
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn malformed_matching_thread_read_response_is_consumed_and_retires() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["method"], "thread/read");
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    let target = CasThreadId::new("thread-target").unwrap();
    let error = session.read_thread_metadata(&target, TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ForegroundIngress {
            method,
            source: ForegroundIngressError::MalformedResponse,
        } if method == "thread/read"
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn transport_loss_after_thread_read_write_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["method"], "thread/read");
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    let target = CasThreadId::new("thread-target").unwrap();
    let error = session.read_thread_metadata(&target, TIMEOUT).unwrap_err();
    assert!(error.invalidates_connection_authority());
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

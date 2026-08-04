use beryl_backend::ManagedBackendError;
use beryl_model::CasThreadId;

use super::{
    send_thread_read_response,
    support::{
        TIMEOUT, assert_initialize, assert_initialized, connector, expect_close, foreground_config,
        read_json, read_text, send_initialize_response, spawn_server,
    },
};

#[test]
fn request_only_thread_read_writes_exact_params_and_publishes_compact_metadata() {
    let incidental = "h".repeat(96 * 1_024);
    let (endpoint, server) = spawn_server(move |socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        assert_eq!(
            read_text(socket).unwrap(),
            r#"{"jsonrpc":"2.0","id":2,"method":"thread/read","params":{"threadId":"thread-target","includeTurns":false}}"#,
        );
        super::support::send_json(
            socket,
            r#"{"method":"unknown/progress","params":{"discard":[1,2,3]}}"#,
        );
        send_thread_read_response(
            socket,
            2,
            "thread-target",
            r#"{"type":"active","activeFlags":["waitingOnUserInput"]}"#,
            Some("Ada"),
            &incidental,
        );
        expect_close(socket);
    });

    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    let target = CasThreadId::new("thread-target").unwrap();
    let metadata = session.read_thread_metadata(&target, TIMEOUT).unwrap();
    assert_eq!(metadata.thread_id(), &target);
    assert!(metadata.status().waiting_on_user_input());
    assert_eq!(metadata.model_provider(), "openai");
    assert_eq!(metadata.agent_nickname(), Some("Ada"));
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn wrong_profile_and_uninitialized_request_only_use_fail_before_request_bytes() {
    let (foreground_endpoint, foreground_server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let foreground_connector = connector(foreground_endpoint);
    let mut foreground = foreground_connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    foreground.initialize_foreground(TIMEOUT).unwrap();
    let target = CasThreadId::new("thread-target").unwrap();
    let before = foreground.predispatch_state_for_lifecycle_test();
    let error = foreground
        .read_thread_metadata(&target, TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestProfileMismatch {
            method: "thread/read",
            required_profile: "request-only",
        }
    ));
    assert_eq!(foreground.predispatch_state_for_lifecycle_test(), before);
    foreground.shutdown().unwrap();
    foreground_server.join().unwrap();

    let (request_endpoint, request_server) = spawn_server(expect_close);
    let request_connector = connector(request_endpoint);
    let mut uninitialized = request_connector
        .connect_request_candidate_for_lifecycle_test(TIMEOUT)
        .unwrap();
    let before = uninitialized.predispatch_state_for_lifecycle_test();
    let error = uninitialized
        .read_thread_metadata(&target, TIMEOUT)
        .unwrap_err();
    assert!(matches!(error, ManagedBackendError::ClientNotInitialized));
    assert_eq!(uninitialized.predispatch_state_for_lifecycle_test(), before);
    uninitialized.shutdown().unwrap();
    request_server.join().unwrap();
}

#[test]
fn requested_thread_identity_is_bounded_before_session_use() {
    assert!(CasThreadId::new("x".repeat(257)).is_err());
}

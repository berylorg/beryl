use std::{path::Path, time::Duration};

use beryl_backend::{ManagedBackendError, ModelListOptions, ModelPage, ThreadUnsubscribeStatus};
use beryl_model::CasThreadId;

use super::support::{
    CONFIG_CWD, TIMEOUT, assert_initialize, assert_initialized, connector, expect_close,
    foreground_config, read_json, send_config_response, send_empty_model_page,
    send_initialize_response, send_json, spawn_server,
};

#[test]
fn foreground_model_pages_are_ordinary_bounded_values_and_write_cancellation_is_exact() {
    let (endpoint, server) = spawn_server(|socket| {
        let initialize = read_json(socket).unwrap();
        assert_initialize(&initialize, false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let first_page = read_json(socket).unwrap();
        assert_model_request(&first_page, 2);
        send_json(
            socket,
            r#"{"method":"unknown/incidental","params":{"ignored":[1,2,3]}}"#,
        );
        send_full_model_page(socket, 2, "page-2");

        let second_page = read_json(socket).unwrap();
        assert_eq!(second_page["jsonrpc"], "2.0");
        assert_eq!(second_page["id"], 3);
        assert_eq!(second_page["method"], "model/list");
        assert_eq!(second_page["params"]["cursor"], "page-2");
        assert_eq!(second_page["params"]["limit"], 1);
        assert_eq!(second_page["params"]["includeHidden"], false);
        send_empty_model_page(socket, 3);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    assert!(session.has_full_turn_stream());

    let before_repeat = session.predispatch_state_for_lifecycle_test();
    let error = session.initialize_foreground(TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ClientAlreadyInitialized
    ));
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        before_repeat
    );

    let before_failure = session.predispatch_state_for_lifecycle_test();
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    let error = session.list_models(TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::WriteRequest { method, .. } if method == "model/list"
    ));
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        before_failure
    );

    let mut first: Box<ModelPage> = session.list_models(TIMEOUT).unwrap();
    assert_eq!(first.len(), 64);
    assert_eq!(first.records().next().unwrap().id(), "id-0");
    assert_eq!(first.records().next_back().unwrap().id(), "id-63");
    assert_eq!(first.next_cursor(), Some("page-2"));

    let cursor = first.take_next_cursor().unwrap();
    let options = ModelListOptions::page(1).unwrap().with_cursor(cursor);
    let second: Box<ModelPage> = session.list_model_page(&options, TIMEOUT).unwrap();
    assert!(second.is_empty());
    assert_eq!(second.next_cursor(), None);
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn exact_model_rejection_leaves_session_reusable() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        assert_model_request(&read_json(socket).unwrap(), 2);
        send_json(
            socket,
            r#"{"error":{"code":-32602,"message":"invalid cursor"},"id":2}"#,
        );
        let config = read_json(socket).unwrap();
        assert_eq!(config["id"], 3);
        assert_eq!(config["method"], "config/read");
        assert_eq!(config["params"]["cwd"], CONFIG_CWD);
        assert_eq!(config["params"]["includeLayers"], false);
        send_config_response(socket, 3);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();

    let error = session.list_models(TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestFailed { method, .. } if method == "model/list"
    ));
    assert!(!session.transport_is_closed_for_lifecycle_test());

    let config = session.read_config(Path::new(CONFIG_CWD), TIMEOUT).unwrap();
    assert_eq!(config.defaults().model(), Some("gpt-5.6"));
    assert_eq!(config.defaults().model_reasoning_effort(), Some("high"));
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn foreground_thread_unsubscribe_uses_one_exact_bounded_request_per_closed_status() {
    const THREAD_ID: &str = "thread-unsubscribe-target";
    const STATUSES: [(&str, ThreadUnsubscribeStatus); 3] = [
        ("notLoaded", ThreadUnsubscribeStatus::NotLoaded),
        ("notSubscribed", ThreadUnsubscribeStatus::NotSubscribed),
        ("unsubscribed", ThreadUnsubscribeStatus::Unsubscribed),
    ];

    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        for (offset, (wire_status, _)) in STATUSES.into_iter().enumerate() {
            let id = u64::try_from(offset).unwrap() + 2;
            let request = read_json(socket).unwrap();
            assert_eq!(request["jsonrpc"], "2.0");
            assert_eq!(request["id"], id);
            assert_eq!(request["method"], "thread/unsubscribe");
            assert_eq!(request["params"]["threadId"], THREAD_ID);
            assert_eq!(request["params"].as_object().unwrap().len(), 1);
            if id == 2 {
                send_json(
                    socket,
                    r#"{"method":"unknown/progress","params":{"discard":[1,2,3]}}"#,
                );
            }
            send_json(
                socket,
                &format!(r#"{{"id":{id},"result":{{"status":"{wire_status}"}}}}"#),
            );
        }
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let thread_id = CasThreadId::new(THREAD_ID).unwrap();

    for (_, expected) in STATUSES {
        let response = session.unsubscribe_thread(&thread_id, TIMEOUT).unwrap();
        assert_eq!(response.status, expected);
    }

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn thread_unsubscribe_predispatch_failure_and_rejection_leave_session_reusable() {
    const THREAD_ID: &str = "thread-unsubscribe-retry";
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let rejected = read_json(socket).unwrap();
        assert_eq!(rejected["id"], 2);
        assert_eq!(rejected["method"], "thread/unsubscribe");
        assert_eq!(rejected["params"]["threadId"], THREAD_ID);
        send_json(
            socket,
            r#"{"error":{"code":-32602,"message":"not subscribed"},"id":2}"#,
        );

        let succeeded = read_json(socket).unwrap();
        assert_eq!(succeeded["id"], 3);
        assert_eq!(succeeded["method"], "thread/unsubscribe");
        assert_eq!(succeeded["params"]["threadId"], THREAD_ID);
        send_json(socket, r#"{"id":3,"result":{"status":"unsubscribed"}}"#);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let thread_id = CasThreadId::new(THREAD_ID).unwrap();

    let before_write_failure = session.predispatch_state_for_lifecycle_test();
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    let error = session.unsubscribe_thread(&thread_id, TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::WriteRequest { method, .. } if method == "thread/unsubscribe"
    ));
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        before_write_failure
    );
    assert!(!session.transport_is_closed_for_lifecycle_test());

    let error = session.unsubscribe_thread(&thread_id, TIMEOUT).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestFailed { method, .. } if method == "thread/unsubscribe"
    ));
    assert!(!session.transport_is_closed_for_lifecycle_test());

    let response = session.unsubscribe_thread(&thread_id, TIMEOUT).unwrap();
    assert_eq!(response.status, ThreadUnsubscribeStatus::Unsubscribed);
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn thread_unsubscribe_timeout_after_dispatch_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "thread/unsubscribe");
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    let thread_id = CasThreadId::new("thread-unsubscribe-timeout").unwrap();
    let request_timeout = Duration::from_millis(40);

    let error = session
        .unsubscribe_thread(&thread_id, request_timeout)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestTimeout { method, timeout }
            if method == "thread/unsubscribe" && timeout == request_timeout
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn model_timeout_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        assert_model_request(&read_json(socket).unwrap(), 2);
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();

    let request_timeout = Duration::from_millis(40);
    let error = session.list_models(request_timeout).unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestTimeout { method, timeout }
            if method == "model/list" && timeout == request_timeout
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn incompatible_initialize_response_publishes_no_profile_and_retires_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_json(
            socket,
            r#"{"id":1,"result":{"userAgent":"beryl/0.143.0","codexHome":"C:\\codex","platformFamily":"windows","platformOs":"windows"}}"#,
        );
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();

    let error = session.initialize_foreground(TIMEOUT).unwrap_err();
    assert!(matches!(error, ManagedBackendError::Compatibility(_)));
    assert!(!session.has_full_turn_stream());
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn invalid_model_page_limits_are_rejected_without_a_session() {
    let zero = ModelListOptions::page(0).unwrap_err();
    assert_eq!(zero.requested, 0);
    assert_eq!(zero.maximum, 64);

    let oversized = ModelListOptions::page(65).unwrap_err();
    assert_eq!(oversized.requested, 65);
    assert_eq!(oversized.maximum, 64);

    assert_eq!(ModelListOptions::page(1).unwrap().limit().get(), 1);
    assert_eq!(ModelListOptions::page(64).unwrap().limit().get(), 64);
}

#[test]
fn exhausted_request_ids_are_rejected_before_bytes_and_leave_transport_reusable() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    session.exhaust_request_ids_for_lifecycle_test();
    let before = session.predispatch_state_for_lifecycle_test();

    let error = session
        .read_config(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::RequestIdExhausted {
            method: "config/read"
        }
    ));
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
    assert!(!session.transport_is_closed_for_lifecycle_test());

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn occupied_expectation_is_rejected_before_bytes_and_leaves_transport_reusable() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    session.prepare_pre_bind_response_wait_for_lifecycle_test(99);
    let before = session.predispatch_state_for_lifecycle_test();

    let error = session
        .read_config(Path::new(CONFIG_CWD), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        error,
        ManagedBackendError::ResponseExpectationUnavailable {
            method: "config/read"
        }
    ));
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
    assert!(!session.transport_is_closed_for_lifecycle_test());

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn poisoned_expectation_never_recovers_or_writes_request_bytes() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector
        .connect_foreground_candidate(foreground_config(1), TIMEOUT)
        .unwrap();
    session.initialize_foreground(TIMEOUT).unwrap();
    session.poison_response_expectation_for_lifecycle_test();
    let before = session.predispatch_state_for_lifecycle_test();

    for _ in 0..2 {
        let error = session
            .read_config(Path::new(CONFIG_CWD), TIMEOUT)
            .unwrap_err();
        assert!(matches!(
            error,
            ManagedBackendError::ResponseExpectationUnavailable {
                method: "config/read"
            }
        ));
        assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
        assert!(!session.transport_is_closed_for_lifecycle_test());
    }

    session.shutdown().unwrap();
    server.join().unwrap();
}

fn assert_model_request(request: &serde_json::Value, id: u64) {
    assert_eq!(request["jsonrpc"], "2.0");
    assert_eq!(request["id"], id);
    assert_eq!(request["method"], "model/list");
    assert!(request["params"].get("cursor").is_none());
    assert_eq!(request["params"]["limit"], 64);
    assert_eq!(request["params"]["includeHidden"], false);
}

fn send_full_model_page(
    socket: &mut tungstenite::WebSocket<std::net::TcpStream>,
    id: u64,
    next_cursor: &str,
) {
    let records = (0..64)
        .map(|index| {
            format!(
                r#"{{"id":"id-{index}","model":"model-{index}","displayName":"Model {index}","hidden":false,"supportedReasoningEfforts":["low","high"],"defaultReasoningEffort":"high","isDefault":{}}}"#,
                index == 0,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    send_json(
        socket,
        &format!(r#"{{"id":{id},"result":{{"data":[{records}],"nextCursor":"{next_cursor}"}}}}"#,),
    );
}

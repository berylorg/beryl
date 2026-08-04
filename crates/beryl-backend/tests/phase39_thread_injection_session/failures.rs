use std::{net::Shutdown, time::Duration};

use beryl_backend::{
    ManagedBackendError, ThreadInjectionOutcome, ThreadInjectionRole, ThreadInjectionSourceError,
    lifecycle_test_support::fresh_idle_thread,
};
use beryl_model::CasThreadId;
use tungstenite::Message;

use super::{
    fixtures::{
        USER_TEXT, assert_injection_request, connect_initialized_foreground, initialize_server,
        one_item_fixture, source_failure_after_full_page,
    },
    support::{TIMEOUT, expect_close, read_json, send_json, spawn_server},
};

#[test]
fn source_failure_after_transport_bytes_is_completion_unknown_and_retires_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        loop {
            match socket.read() {
                Ok(Message::Text(text)) => {
                    panic!("source failure unexpectedly completed a JSON message: {text}")
                }
                Ok(Message::Close(_)) => break,
                Ok(
                    Message::Binary(_) | Message::Ping(_) | Message::Pong(_) | Message::Frame(_),
                ) => {}
                Err(_) => break,
            }
        }
    });
    let mut session = connect_initialized_foreground(endpoint);
    let mut fixture = source_failure_after_full_page(10);
    let thread_id = CasThreadId::new("thread-injection-source-after-bytes").unwrap();

    let outcome = session.inject_thread_items(
        fresh_idle_thread(thread_id.clone()),
        &fixture.preflight,
        &mut fixture.source,
        TIMEOUT,
    );
    let ThreadInjectionOutcome::CompletionUnknown {
        thread_id: actual,
        error,
    } = outcome
    else {
        panic!("post-dispatch source failure must be completion unknown");
    };
    assert_eq!(actual, thread_id);
    assert!(matches!(
        error.as_ref(),
        ManagedBackendError::ThreadInjectionSource {
            source: ThreadInjectionSourceError::ReadFailed,
            transport_bytes_written: true,
            ..
        }
    ));
    assert!(error.invalidates_connection_authority());
    assert_eq!(fixture.source.calls(), 2);
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn concrete_socket_loss_after_dispatch_remains_transport_lost() {
    let thread_id = CasThreadId::new("thread-injection-transport-loss").unwrap();
    let expected = thread_id.clone();
    let (endpoint, server) = spawn_server(move |socket| {
        initialize_server(socket);
        let request = read_json(socket).unwrap();
        assert_injection_request(
            &request,
            2,
            expected.as_str(),
            ThreadInjectionRole::UserInputText,
            USER_TEXT,
        );
        socket.get_mut().shutdown(Shutdown::Both).unwrap();
    });
    let mut session = connect_initialized_foreground(endpoint);
    let mut fixture = one_item_fixture(11, ThreadInjectionRole::UserInputText, USER_TEXT);

    let outcome = session.inject_thread_items(
        fresh_idle_thread(thread_id.clone()),
        &fixture.preflight,
        &mut fixture.source,
        TIMEOUT,
    );
    let ThreadInjectionOutcome::TransportLost {
        thread_id: actual,
        error,
    } = outcome
    else {
        panic!("concrete socket loss must remain transport lost");
    };
    assert_eq!(actual, thread_id);
    assert!(matches!(
        error.as_ref(),
        ManagedBackendError::WebSocketTransport { .. }
            | ManagedBackendError::TransportClosed { .. }
    ));
    assert!(error.invalidates_connection_authority());
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn response_timeout_is_completion_unknown_not_transport_lost() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "thread/inject_items");
        expect_close(socket);
    });
    let mut session = connect_initialized_foreground(endpoint);
    let mut fixture = one_item_fixture(12, ThreadInjectionRole::UserInputText, USER_TEXT);
    let thread_id = CasThreadId::new("thread-injection-timeout").unwrap();
    let timeout = Duration::from_millis(40);

    let outcome = session.inject_thread_items(
        fresh_idle_thread(thread_id.clone()),
        &fixture.preflight,
        &mut fixture.source,
        timeout,
    );
    let ThreadInjectionOutcome::CompletionUnknown {
        thread_id: actual,
        error,
    } = outcome
    else {
        panic!("response timeout must be completion unknown");
    };
    assert_eq!(actual, thread_id);
    assert!(matches!(
        error.as_ref(),
        ManagedBackendError::RequestTimeout {
            method,
            timeout: actual_timeout,
        } if method == "thread/inject_items" && *actual_timeout == timeout
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn malformed_matching_response_is_completion_unknown_and_poisons_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        send_json(socket, r#"{"id":2,"result":"#);
        expect_close(socket);
    });
    let mut session = connect_initialized_foreground(endpoint);
    let mut fixture = one_item_fixture(13, ThreadInjectionRole::UserInputText, USER_TEXT);
    let thread_id = CasThreadId::new("thread-injection-malformed-response").unwrap();

    let outcome = session.inject_thread_items(
        fresh_idle_thread(thread_id.clone()),
        &fixture.preflight,
        &mut fixture.source,
        TIMEOUT,
    );
    let ThreadInjectionOutcome::CompletionUnknown {
        thread_id: actual,
        error,
    } = outcome
    else {
        panic!("malformed response after dispatch must be completion unknown");
    };
    assert_eq!(actual, thread_id);
    assert!(error.invalidates_connection_authority());
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(!session.predispatch_state_for_lifecycle_test().1);
    server.join().unwrap();
}

#[test]
fn response_id_mismatch_is_completion_unknown_and_poisons_the_connection() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        send_json(socket, r#"{"id":99,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized_foreground(endpoint);
    let mut fixture = one_item_fixture(14, ThreadInjectionRole::UserInputText, USER_TEXT);
    let thread_id = CasThreadId::new("thread-injection-response-id-mismatch").unwrap();

    let outcome = session.inject_thread_items(
        fresh_idle_thread(thread_id.clone()),
        &fixture.preflight,
        &mut fixture.source,
        TIMEOUT,
    );
    let ThreadInjectionOutcome::CompletionUnknown {
        thread_id: actual,
        error,
    } = outcome
    else {
        panic!("response identity mismatch after dispatch must be completion unknown");
    };
    assert_eq!(actual, thread_id);
    assert!(error.invalidates_connection_authority());
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(!session.predispatch_state_for_lifecycle_test().1);
    server.join().unwrap();
}

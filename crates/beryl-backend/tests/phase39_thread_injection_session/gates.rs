use beryl_backend::{
    ManagedBackendError, ThreadInjectionOutcome, ThreadInjectionRole,
    lifecycle_test_support::fresh_idle_thread,
};
use beryl_model::CasThreadId;

use super::{
    fixtures::{USER_TEXT, connect_initialized_foreground, initialize_server, one_item_fixture},
    support::{
        TIMEOUT, assert_initialize, assert_initialized, connector, expect_close, read_json,
        send_initialize_response, spawn_server,
    },
};

#[test]
fn request_only_websocket_is_rejected_before_source_read_or_dispatch() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), true);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let connector = connector(endpoint);
    let mut session = connector.connect_request_client(TIMEOUT).unwrap();
    let before = session.predispatch_state_for_lifecycle_test();
    let mut fixture = one_item_fixture(20, ThreadInjectionRole::UserInputText, USER_TEXT);
    let thread_id = CasThreadId::new("thread-injection-request-only").unwrap();

    let outcome = session.inject_thread_items(
        fresh_idle_thread(thread_id.clone()),
        &fixture.preflight,
        &mut fixture.source,
        TIMEOUT,
    );
    let ThreadInjectionOutcome::ProvenNotDispatched {
        thread_id: actual,
        error,
    } = outcome
    else {
        panic!("request-only injection must be proven not dispatched");
    };
    assert_eq!(actual, thread_id);
    assert!(matches!(
        error.as_ref(),
        ManagedBackendError::ThreadInjectionTransportUnsupported {
            method,
            transport: "request-only websocket",
        } if method == "thread/inject_items"
    ));
    assert_eq!(fixture.source.calls(), 0);
    assert_eq!(session.predispatch_state_for_lifecycle_test(), before);
    assert!(!session.transport_is_closed_for_lifecycle_test());
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn poisoned_expectation_rejects_injection_before_source_read_or_dispatch() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        expect_close(socket);
    });
    let mut session = connect_initialized_foreground(endpoint);
    session.poison_response_expectation_for_lifecycle_test();
    let mut fixture = one_item_fixture(21, ThreadInjectionRole::UserInputText, USER_TEXT);
    let thread_id = CasThreadId::new("thread-injection-poisoned-expectation").unwrap();

    let outcome = session.inject_thread_items(
        fresh_idle_thread(thread_id.clone()),
        &fixture.preflight,
        &mut fixture.source,
        TIMEOUT,
    );
    assert!(matches!(
        outcome,
        ThreadInjectionOutcome::ProvenNotDispatched {
            thread_id: actual,
            error,
        } if actual == thread_id && matches!(
            error.as_ref(),
            ManagedBackendError::ResponseExpectationUnavailable {
                method: "thread/inject_items"
            }
        )
    ));
    assert_eq!(fixture.source.calls(), 0);
    assert!(!session.predispatch_state_for_lifecycle_test().1);
    assert!(!session.transport_is_closed_for_lifecycle_test());
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn exhausted_request_identity_rejects_injection_before_source_read_or_dispatch() {
    let (endpoint, server) = spawn_server(|socket| {
        initialize_server(socket);
        expect_close(socket);
    });
    let mut session = connect_initialized_foreground(endpoint);
    session.exhaust_request_ids_for_lifecycle_test();
    let mut fixture = one_item_fixture(22, ThreadInjectionRole::UserInputText, USER_TEXT);
    let thread_id = CasThreadId::new("thread-injection-exhausted-id").unwrap();

    let outcome = session.inject_thread_items(
        fresh_idle_thread(thread_id.clone()),
        &fixture.preflight,
        &mut fixture.source,
        TIMEOUT,
    );
    assert!(matches!(
        outcome,
        ThreadInjectionOutcome::ProvenNotDispatched {
            thread_id: actual,
            error,
        } if actual == thread_id && matches!(
            error.as_ref(),
            ManagedBackendError::RequestIdExhausted {
                method: "thread/inject_items"
            }
        )
    ));
    assert_eq!(fixture.source.calls(), 0);
    assert!(!session.transport_is_closed_for_lifecycle_test());
    session.shutdown().unwrap();
    server.join().unwrap();
}

use std::time::Duration;

use beryl_backend::{
    ManagedBackendError, ThreadInjectionOutcome, ThreadInjectionRole, ThreadInjectionSourceError,
    lifecycle_test_support::fresh_idle_thread,
};
use beryl_model::CasThreadId;

use super::{
    fixtures::{
        ASSISTANT_TEXT, USER_TEXT, assert_injection_request, connect_initialized_foreground,
        immediate_source_failure, initialize_server, one_item_fixture,
    },
    support::{TIMEOUT, expect_close, read_json, send_json, spawn_server},
};

#[test]
fn exact_success_promotes_the_consumed_target_and_reports_bounded_transport_usage() {
    let thread_id = CasThreadId::new("thread-injection-success").unwrap();
    let expected_thread_id = thread_id.clone();
    let (endpoint, server) = spawn_server(move |socket| {
        initialize_server(socket);
        let request = read_json(socket).unwrap();
        assert_injection_request(
            &request,
            2,
            expected_thread_id.as_str(),
            ThreadInjectionRole::UserInputText,
            USER_TEXT,
        );
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized_foreground(endpoint);
    let diagnostics = session.websocket_diagnostics_for_lifecycle_test().unwrap();
    let mut fixture = one_item_fixture(1, ThreadInjectionRole::UserInputText, USER_TEXT);

    let outcome = session.inject_thread_items(
        fresh_idle_thread(thread_id.clone()),
        &fixture.preflight,
        &mut fixture.source,
        TIMEOUT,
    );
    let ThreadInjectionOutcome::Succeeded { thread } = outcome else {
        panic!("exact empty acknowledgement must succeed");
    };
    assert_eq!(thread.thread_id(), &thread_id);
    assert_eq!(fixture.source.calls(), 2);
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        (3, true, false)
    );
    let snapshot = diagnostics.snapshot();
    assert_eq!(snapshot.outbound_buffer_capacity_bytes(), 64 * 1_024);
    assert!(snapshot.maximum_outbound_buffered_bytes() > 0);
    assert!(
        snapshot.maximum_outbound_buffered_bytes() <= snapshot.outbound_buffer_capacity_bytes()
    );
    assert!(snapshot.outbound_frames() > 0);
    assert!(snapshot.inbound_frames() > 0);
    assert!(snapshot.decoded_messages() > 0);
    assert!(snapshot.outbound_logical_bytes() > 0);
    assert!(snapshot.inbound_logical_bytes() > 0);
    assert!(!session.transport_is_closed_for_lifecycle_test());

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn structured_rejection_is_bounded_and_the_connection_accepts_the_next_injection() {
    let rejected_id = CasThreadId::new("thread-injection-rejected").unwrap();
    let succeeded_id = CasThreadId::new("thread-injection-after-rejection").unwrap();
    let expected_rejected = rejected_id.clone();
    let expected_succeeded = succeeded_id.clone();
    let (endpoint, server) = spawn_server(move |socket| {
        initialize_server(socket);
        let rejected = read_json(socket).unwrap();
        assert_injection_request(
            &rejected,
            2,
            expected_rejected.as_str(),
            ThreadInjectionRole::AssistantOutputText,
            ASSISTANT_TEXT,
        );
        send_json(
            socket,
            r#"{"error":{"code":-32091,"data":{"private":true},"message":"recovery rejected"},"id":2}"#,
        );
        let succeeded = read_json(socket).unwrap();
        assert_injection_request(
            &succeeded,
            3,
            expected_succeeded.as_str(),
            ThreadInjectionRole::UserInputText,
            USER_TEXT,
        );
        send_json(socket, r#"{"id":3,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized_foreground(endpoint);
    let mut rejected_fixture =
        one_item_fixture(2, ThreadInjectionRole::AssistantOutputText, ASSISTANT_TEXT);
    let rejected = session.inject_thread_items(
        fresh_idle_thread(rejected_id.clone()),
        &rejected_fixture.preflight,
        &mut rejected_fixture.source,
        TIMEOUT,
    );
    let ThreadInjectionOutcome::Rejected {
        thread_id,
        rejection,
    } = rejected
    else {
        panic!("matching JSON-RPC rejection must remain distinct");
    };
    assert_eq!(thread_id, rejected_id);
    assert_eq!(rejection.code(), -32091);
    assert_eq!(rejection.message(), "recovery rejected");
    assert!(!rejection.message_was_truncated());
    assert!(rejection.data_was_present());
    assert!(!session.transport_is_closed_for_lifecycle_test());
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        (3, true, false)
    );

    let mut success_fixture = one_item_fixture(3, ThreadInjectionRole::UserInputText, USER_TEXT);
    let succeeded = session.inject_thread_items(
        fresh_idle_thread(succeeded_id.clone()),
        &success_fixture.preflight,
        &mut success_fixture.source,
        TIMEOUT,
    );
    assert!(matches!(
        succeeded,
        ThreadInjectionOutcome::Succeeded { ref thread }
            if thread.thread_id() == &succeeded_id
    ));
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn prebyte_write_and_source_failures_preserve_request_identity_and_connection_authority() {
    let final_id = CasThreadId::new("thread-injection-after-predispatch-failures").unwrap();
    let expected_final = final_id.clone();
    let (endpoint, server) = spawn_server(move |socket| {
        initialize_server(socket);
        let request = read_json(socket).unwrap();
        assert_injection_request(
            &request,
            2,
            expected_final.as_str(),
            ThreadInjectionRole::UserInputText,
            USER_TEXT,
        );
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized_foreground(endpoint);
    let predispatch_state = session.predispatch_state_for_lifecycle_test();

    let mut write_fixture = one_item_fixture(4, ThreadInjectionRole::UserInputText, USER_TEXT);
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    let write_failure = session.inject_thread_items(
        fresh_idle_thread(CasThreadId::new("thread-injection-write-failure").unwrap()),
        &write_fixture.preflight,
        &mut write_fixture.source,
        Duration::from_millis(100),
    );
    assert!(matches!(
        write_failure,
        ThreadInjectionOutcome::ProvenNotDispatched { error, .. }
            if matches!(*error, ManagedBackendError::WriteRequest { ref method, .. }
                if method == "thread/inject_items")
    ));
    assert_eq!(write_fixture.source.calls(), 0);
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        predispatch_state
    );
    assert!(!session.transport_is_closed_for_lifecycle_test());

    let mut source_fixture = immediate_source_failure(5);
    let source_failure = session.inject_thread_items(
        fresh_idle_thread(CasThreadId::new("thread-injection-source-failure").unwrap()),
        &source_fixture.preflight,
        &mut source_fixture.source,
        Duration::from_millis(100),
    );
    assert!(matches!(
        source_failure,
        ThreadInjectionOutcome::ProvenNotDispatched { error, .. }
            if matches!(*error, ManagedBackendError::ThreadInjectionSource {
                source: ThreadInjectionSourceError::ReadFailed,
                transport_bytes_written: false,
                ..
            })
    ));
    assert_eq!(source_fixture.source.calls(), 1);
    assert_eq!(
        session.predispatch_state_for_lifecycle_test(),
        predispatch_state
    );
    assert!(!session.transport_is_closed_for_lifecycle_test());

    let mut final_fixture = one_item_fixture(6, ThreadInjectionRole::UserInputText, USER_TEXT);
    let final_outcome = session.inject_thread_items(
        fresh_idle_thread(final_id),
        &final_fixture.preflight,
        &mut final_fixture.source,
        TIMEOUT,
    );
    assert!(matches!(
        final_outcome,
        ThreadInjectionOutcome::Succeeded { .. }
    ));
    session.shutdown().unwrap();
    server.join().unwrap();
}

use beryl_backend::{
    CoarseThreadCleanupDisposition, ManagedBackendError, TurnInterruptDisposition,
};

use crate::{
    support::{authorize, connect_foreground, connect_initialized},
    websocket::{
        TIMEOUT, assert_initialize, assert_initialized, expect_close, read_json, read_text,
        send_initialize_response, send_json, spawn_server,
    },
};

#[test]
fn coarse_cleanup_admission_is_read_only_and_follows_exact_foreground_initialization() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let mut session = connect_foreground(endpoint);

    assert!(!session.admits_exact_thread_background_terminals_cleanup());
    session.initialize_foreground(TIMEOUT).unwrap();
    assert!(session.admits_exact_thread_background_terminals_cleanup());

    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn accepted_cleanup_is_only_a_same_session_ordering_fact() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let wire = read_text(socket).unwrap();
        assert_eq!(
            wire,
            r#"{"method":"thread/backgroundTerminals/clean","id":2,"params":{"threadId":"thread-phase67"}}"#
        );
        let cleanup: serde_json::Value = serde_json::from_str(&wire).unwrap();
        assert_eq!(cleanup["id"], 2);
        assert_eq!(cleanup["method"], "thread/backgroundTerminals/clean");
        assert_eq!(cleanup["params"]["threadId"], "thread-phase67");
        assert_eq!(cleanup["params"].as_object().unwrap().len(), 1);
        send_json(socket, r#"{"id":2,"result":{}}"#);

        let later = read_json(socket).unwrap();
        assert_eq!(later["id"], 3);
        assert_eq!(later["method"], "turn/interrupt");
        send_json(socket, r#"{"id":3,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let cleanup_authorization = authorize(&mut session);
    let cleanup = session.clean_exact_thread_background_terminals(cleanup_authorization, TIMEOUT);
    let ordering = match cleanup.disposition() {
        CoarseThreadCleanupDisposition::RequestAccepted { ordering } => ordering,
        other => panic!("unexpected cleanup disposition: {other:?}"),
    };
    assert_ne!(ordering.session_token(), 0);

    let interrupt_authorization = authorize(&mut session);
    let interrupt = session.interrupt_exact_foreground_turn(interrupt_authorization, TIMEOUT);
    assert!(matches!(
        interrupt.disposition(),
        TurnInterruptDisposition::RequestAccepted
    ));
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn any_cleanup_json_rpc_error_retires_before_reuse() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        assert_eq!(
            read_json(socket).unwrap()["method"],
            "thread/backgroundTerminals/clean"
        );
        send_json(
            socket,
            r#"{"error":{"code":-32600,"message":"diagnostic is not classified"},"id":2}"#,
        );
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let authorization = authorize(&mut session);
    let outcome = session.clean_exact_thread_background_terminals(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        CoarseThreadCleanupDisposition::SessionAuthorityInvalidated { error }
            if matches!(
                **error,
                ManagedBackendError::RequestFailed {
                    ref method,
                    ..
                } if method == "thread/backgroundTerminals/clean"
            )
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

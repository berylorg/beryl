use beryl_backend::{ManagedBackendError, TurnInterruptDisposition};

use crate::{
    support::{authorize, connect_initialized},
    websocket::{
        TIMEOUT, assert_initialize, assert_initialized, expect_close, read_json,
        send_initialize_response, spawn_server,
    },
};

#[test]
fn exhausted_request_identity_and_occupied_response_state_are_prebyte() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let authorization = authorize(&mut session);
    session.exhaust_request_ids_for_lifecycle_test();
    let outcome = session.interrupt_exact_foreground_turn(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::ProvenNotDispatched { error }
            if matches!(**error, ManagedBackendError::RequestIdExhausted { .. })
    ));
    session.shutdown().unwrap();
    server.join().unwrap();

    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let authorization = authorize(&mut session);
    session.occupy_response_expectation_for_lifecycle_test(99);
    let outcome = session.interrupt_exact_foreground_turn(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::ProvenNotDispatched { error }
            if matches!(
                **error,
                ManagedBackendError::ResponseExpectationUnavailable {
                    method: "turn/interrupt"
                }
            )
    ));
    session.shutdown().unwrap();
    server.join().unwrap();
}

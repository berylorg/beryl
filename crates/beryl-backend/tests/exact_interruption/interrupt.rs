use beryl_backend::{
    CallerNoSuccessorFence, ExactHardStopLimitation, ManagedBackendError, StopAttemptCorrelation,
    StopOperationCorrelation, TurnInterruptDisposition,
};

use crate::{
    support::{authorize, connect_initialized, connect_initialized_unbound, target},
    websocket::{
        TIMEOUT, assert_initialize, assert_initialized, expect_close, read_json, read_text,
        send_initialize_response, send_json, spawn_server,
    },
};

#[test]
fn exact_wire_omits_local_correlations_and_preserves_ordered_ingress() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let wire = read_text(socket).unwrap();
        assert_eq!(
            wire,
            r#"{"method":"turn/interrupt","id":2,"params":{"threadId":"thread-phase67","turnId":"turn-phase67"}}"#
        );
        let request: serde_json::Value = serde_json::from_str(&wire).unwrap();
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "turn/interrupt");
        assert_eq!(request["params"]["threadId"], "thread-phase67");
        assert_eq!(request["params"]["turnId"], "turn-phase67");
        assert_eq!(request["params"].as_object().unwrap().len(), 2);
        assert!(!wire.contains("a5a5"));
        assert!(!wire.contains("5a5a"));

        send_json(
            socket,
            r#"{"method":"thread/name/updated","params":{"threadId":"thread-phase67","threadName":"discarded"}}"#,
        );
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let authorization = authorize(&mut session);
    let outcome = session.interrupt_exact_foreground_turn(authorization, TIMEOUT);

    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::RequestAccepted
    ));
    assert_eq!(outcome.request().target(), &target());
    assert_eq!(
        outcome.request().operation_correlation().as_bytes(),
        &[0xA5; 16]
    );
    assert_eq!(
        outcome.request().attempt_correlation().as_bytes(),
        &[0x5A; 16]
    );
    assert!(outcome.request().had_no_successor_fence());
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn pinned_rejections_are_closed_without_parsing_diagnostic_text() {
    for code in [-32_600, -32_603] {
        let (endpoint, server) = spawn_server(move |socket| {
            assert_initialize(&read_json(socket).unwrap(), false);
            send_initialize_response(socket, 1);
            assert_initialized(&read_json(socket).unwrap());
            assert_eq!(read_json(socket).unwrap()["method"], "turn/interrupt");
            send_json(
                socket,
                &format!(
                    r#"{{"error":{{"code":{code},"message":"different text each time"}},"id":2}}"#
                ),
            );
            expect_close(socket);
        });
        let mut session = connect_initialized(endpoint);
        let authorization = authorize(&mut session);
        let outcome = session.interrupt_exact_foreground_turn(authorization, TIMEOUT);
        assert!(matches!(
            outcome.disposition(),
            TurnInterruptDisposition::RejectedBeforeCoreInterrupt
        ));
        session.shutdown().unwrap();
        server.join().unwrap();
    }
}

#[test]
fn unsupported_exact_target_families_accept_no_handles() {
    assert_eq!(
        ExactHardStopLimitation::pinned(),
        [
            ExactHardStopLimitation::ChildOrSubagentInterruptionUnsupported,
            ExactHardStopLimitation::IndividualTurnProcessTerminationIdentityUnsafe,
        ]
    );
}

#[test]
fn local_prebyte_failure_is_reusable_but_possible_dispatch_retires() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    let first_authorization = authorize(&mut session);
    let first = session.interrupt_exact_foreground_turn(first_authorization, TIMEOUT);
    assert!(matches!(
        first.disposition(),
        TurnInterruptDisposition::ProvenNotDispatched { .. }
    ));
    let second_authorization = authorize(&mut session);
    let second = session.interrupt_exact_foreground_turn(second_authorization, TIMEOUT);
    assert!(matches!(
        second.disposition(),
        TurnInterruptDisposition::RequestAccepted
    ));
    session.shutdown().unwrap();
    server.join().unwrap();

    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        assert_eq!(read_json(socket).unwrap()["method"], "turn/interrupt");
    });
    let mut session = connect_initialized(endpoint);
    let authorization = authorize(&mut session);
    let outcome = session.interrupt_exact_foreground_turn(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::CompletionUnknown { .. }
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(matches!(
        session.authorize_exact_foreground_turn(
            target(),
            StopOperationCorrelation::from_bytes([1; 16]),
            StopAttemptCorrelation::from_bytes([2; 16]),
            CallerNoSuccessorFence::issue(),
        ),
        Err(ManagedBackendError::TransportClosed { .. })
    ));
    server.join().unwrap();
}

#[test]
fn stale_authorization_and_unbound_driver_refuse_before_bytes() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let stale = authorize(&mut session);
    let current = authorize(&mut session);
    let stale_outcome = session.interrupt_exact_foreground_turn(stale, TIMEOUT);
    assert!(matches!(
        stale_outcome.disposition(),
        TurnInterruptDisposition::ProvenNotDispatched { error }
            if matches!(**error, ManagedBackendError::ExactForegroundAuthorizationStale)
    ));
    let current_outcome = session.interrupt_exact_foreground_turn(current, TIMEOUT);
    assert!(matches!(
        current_outcome.disposition(),
        TurnInterruptDisposition::RequestAccepted
    ));
    session.shutdown().unwrap();
    server.join().unwrap();

    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let mut unbound = connect_initialized_unbound(endpoint);
    assert!(matches!(
        unbound.authorize_exact_foreground_turn(
            target(),
            StopOperationCorrelation::from_bytes([3; 16]),
            StopAttemptCorrelation::from_bytes([4; 16]),
            CallerNoSuccessorFence::issue(),
        ),
        Err(ManagedBackendError::ExactForegroundTurnUnbound)
    ));
    unbound.shutdown().unwrap();
    server.join().unwrap();
}

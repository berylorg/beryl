use std::time::Duration;

use beryl_backend::{
    CallerNoSuccessorFence, ExactForegroundTurn, ExactForegroundTurnAuthorization,
    ManagedBackendError, ManagedBackendSession, PersistentFailureInterruptAuthorization,
    PersistentFailureInterruptCorrelation, PersistentFailureInterruptOutcome,
    StopAttemptCorrelation, StopOperationCorrelation, TurnInterruptDisposition,
    TurnInterruptOutcome,
};

use crate::{
    support::{
        authorize_persistent_failure, connect_initialized, connect_initialized_unbound, target,
    },
    websocket::{
        TIMEOUT, assert_initialize, assert_initialized, expect_close, read_json, read_text,
        send_initialize_response, send_json, spawn_server,
    },
};

#[test]
fn volatile_authority_is_nominally_separate_from_durable_stop() {
    let _: fn(
        &mut ManagedBackendSession,
        ExactForegroundTurn,
        StopOperationCorrelation,
        StopAttemptCorrelation,
        CallerNoSuccessorFence,
    ) -> Result<ExactForegroundTurnAuthorization, ManagedBackendError> =
        ManagedBackendSession::authorize_exact_foreground_turn;
    let _: fn(
        &mut ManagedBackendSession,
        ExactForegroundTurn,
        PersistentFailureInterruptCorrelation,
        CallerNoSuccessorFence,
    ) -> Result<PersistentFailureInterruptAuthorization, ManagedBackendError> =
        ManagedBackendSession::authorize_persistent_failure_interrupt;
    let _: fn(
        &mut ManagedBackendSession,
        ExactForegroundTurnAuthorization,
        Duration,
    ) -> TurnInterruptOutcome = ManagedBackendSession::interrupt_exact_foreground_turn;
    let _: fn(
        &mut ManagedBackendSession,
        PersistentFailureInterruptAuthorization,
        Duration,
    ) -> PersistentFailureInterruptOutcome =
        ManagedBackendSession::interrupt_for_persistent_failure;

    let source = include_str!("../../src/persistent_failure_interrupt.rs");
    for forbidden in [
        "StopOperationCorrelation",
        "StopAttemptCorrelation",
        "HomeStore",
        "FailureGeneration",
        "syndic",
        "gpui",
    ] {
        assert!(
            !source.contains(forbidden),
            "volatile backend authority crossed forbidden boundary with {forbidden}"
        );
    }
}

#[test]
fn volatile_wire_matches_pinned_interrupt_and_omits_correlation() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let wire = read_text(socket).unwrap();
        assert_eq!(
            wire,
            r#"{"method":"turn/interrupt","id":2,"params":{"threadId":"thread-phase67","turnId":"turn-phase67"}}"#
        );
        assert!(!wire.contains("c3c3"));
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let authorization = authorize_persistent_failure(&mut session);
    let outcome = session.interrupt_for_persistent_failure(authorization, TIMEOUT);

    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::RequestAccepted
    ));
    assert_eq!(outcome.request().target(), &target());
    assert_eq!(outcome.request().correlation().as_bytes(), &[0xC3; 16]);
    assert!(outcome.request().had_no_successor_fence());
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn volatile_interrupt_preserves_closed_rejection_and_prebyte_outcomes() {
    for code in [-32_600, -32_603] {
        let (endpoint, server) = spawn_server(move |socket| {
            assert_initialize(&read_json(socket).unwrap(), false);
            send_initialize_response(socket, 1);
            assert_initialized(&read_json(socket).unwrap());
            assert_eq!(read_json(socket).unwrap()["method"], "turn/interrupt");
            send_json(
                socket,
                &format!(r#"{{"error":{{"code":{code},"message":"opaque"}},"id":2}}"#),
            );
            expect_close(socket);
        });
        let mut session = connect_initialized(endpoint);
        let authorization = authorize_persistent_failure(&mut session);
        let outcome = session.interrupt_for_persistent_failure(authorization, TIMEOUT);
        assert!(matches!(
            outcome.disposition(),
            TurnInterruptDisposition::RejectedBeforeCoreInterrupt
        ));
        session.shutdown().unwrap();
        server.join().unwrap();
    }

    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    session.fail_next_write_before_dispatch_for_lifecycle_test();
    let authorization = authorize_persistent_failure(&mut session);
    let outcome = session.interrupt_for_persistent_failure(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::ProvenNotDispatched { .. }
    ));
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn volatile_possible_dispatch_retires_and_revocation_refuses_before_bytes() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        assert_eq!(read_json(socket).unwrap()["method"], "turn/interrupt");
    });
    let mut session = connect_initialized(endpoint);
    let authorization = authorize_persistent_failure(&mut session);
    let outcome = session.interrupt_for_persistent_failure(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::CompletionUnknown { .. }
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    assert!(matches!(
        session.authorize_persistent_failure_interrupt(
            target(),
            PersistentFailureInterruptCorrelation::from_bytes([1; 16]),
            CallerNoSuccessorFence::issue(),
        ),
        Err(ManagedBackendError::TransportClosed { .. })
    ));
    server.join().unwrap();

    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let authorization = authorize_persistent_failure(&mut session);
    session
        .revoke_exact_foreground_turn_authorizations()
        .unwrap();
    let outcome = session.interrupt_for_persistent_failure(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::ProvenNotDispatched { error }
            if matches!(**error, ManagedBackendError::ExactForegroundAuthorizationStale)
    ));
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn volatile_malformed_mismatched_and_timed_out_responses_are_completion_unknown() {
    for response in [
        r#"{"id":99,"result":{}}"#,
        r#"{"id":2,"result":{"unexpected":true}}"#,
    ] {
        let (endpoint, server) = spawn_server(move |socket| {
            assert_initialize(&read_json(socket).unwrap(), false);
            send_initialize_response(socket, 1);
            assert_initialized(&read_json(socket).unwrap());
            assert_eq!(read_json(socket).unwrap()["method"], "turn/interrupt");
            send_json(socket, response);
            expect_close(socket);
        });
        let mut session = connect_initialized(endpoint);
        let authorization = authorize_persistent_failure(&mut session);
        let outcome = session.interrupt_for_persistent_failure(authorization, TIMEOUT);
        assert!(matches!(
            outcome.disposition(),
            TurnInterruptDisposition::CompletionUnknown { .. }
        ));
        assert!(session.transport_is_closed_for_lifecycle_test());
        server.join().unwrap();
    }

    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        assert_eq!(read_json(socket).unwrap()["method"], "turn/interrupt");
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let authorization = authorize_persistent_failure(&mut session);
    let timeout = Duration::from_millis(25);
    let outcome = session.interrupt_for_persistent_failure(authorization, timeout);
    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::CompletionUnknown { error }
            if matches!(
                **error,
                ManagedBackendError::RequestTimeout {
                    method: ref actual,
                    timeout: actual_timeout,
                } if actual == "turn/interrupt" && actual_timeout == timeout
            )
    ));
    assert!(session.transport_is_closed_for_lifecycle_test());
    server.join().unwrap();
}

#[test]
fn volatile_authorization_rejects_unbound_and_foreign_targets() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());
        expect_close(socket);
    });
    let mut session = connect_initialized_unbound(endpoint);
    assert!(matches!(
        session.authorize_persistent_failure_interrupt(
            target(),
            PersistentFailureInterruptCorrelation::from_bytes([2; 16]),
            CallerNoSuccessorFence::issue(),
        ),
        Err(ManagedBackendError::ExactForegroundTurnUnbound)
    ));
    session.bind_exact_foreground_turn(target()).unwrap();
    let mut foreign = target();
    foreign = ExactForegroundTurn::new(
        foreign.runtime_id(),
        foreign.loaded_session_generation(),
        foreign.thread_id().clone(),
        beryl_model::CasTurnId::new("turn-foreign").unwrap(),
    );
    assert!(matches!(
        session.authorize_persistent_failure_interrupt(
            foreign,
            PersistentFailureInterruptCorrelation::from_bytes([3; 16]),
            CallerNoSuccessorFence::issue(),
        ),
        Err(ManagedBackendError::ExactForegroundTurnMismatch)
    ));
    session.shutdown().unwrap();
    server.join().unwrap();
}

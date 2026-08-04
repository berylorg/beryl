use beryl_backend::{
    CallerNoSuccessorFence, ExactForegroundTurn, ManagedBackendError, StopAttemptCorrelation,
    StopOperationCorrelation, TurnInterruptDisposition,
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    CasTurnId, RuntimeId,
};

use crate::{
    support::{authorize, connect_initialized, target},
    websocket::{
        TIMEOUT, assert_initialize, assert_initialized, expect_close, read_json,
        send_initialize_response, send_json, spawn_server,
    },
};

fn changed_target(
    runtime: u8,
    process: u64,
    loaded_thread: u64,
    thread: &str,
    turn: &str,
) -> ExactForegroundTurn {
    ExactForegroundTurn::new(
        RuntimeId::from_bytes([runtime; 16]),
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(process).unwrap(),
            CasLoadedThreadGeneration::new(loaded_thread).unwrap(),
        ),
        CasThreadId::new(thread).unwrap(),
        CasTurnId::new(turn).unwrap(),
    )
}

fn authorize_target(
    session: &mut beryl_backend::ManagedBackendSession,
    target: ExactForegroundTurn,
) -> Result<beryl_backend::ExactForegroundTurnAuthorization, ManagedBackendError> {
    session.authorize_exact_foreground_turn(
        target,
        StopOperationCorrelation::from_bytes([0x31; 16]),
        StopAttemptCorrelation::from_bytes([0x67; 16]),
        CallerNoSuccessorFence::issue(),
    )
}

#[test]
fn every_changed_exact_target_component_refuses_before_bytes() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let request = read_json(socket).unwrap();
        assert_eq!(request["id"], 2);
        assert_eq!(request["method"], "turn/interrupt");
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let mismatches = [
        changed_target(4, 7, 11, "thread-phase67", "turn-phase67"),
        changed_target(3, 8, 11, "thread-phase67", "turn-phase67"),
        changed_target(3, 7, 12, "thread-phase67", "turn-phase67"),
        changed_target(3, 7, 11, "thread-other", "turn-phase67"),
        changed_target(3, 7, 11, "thread-phase67", "turn-other"),
    ];
    for mismatch in mismatches {
        assert!(matches!(
            authorize_target(&mut session, mismatch),
            Err(ManagedBackendError::ExactForegroundTurnMismatch)
        ));
    }

    let authorization = authorize(&mut session);
    let outcome = session.interrupt_exact_foreground_turn(authorization, TIMEOUT);
    assert!(matches!(
        outcome.disposition(),
        TurnInterruptDisposition::RequestAccepted
    ));
    session.shutdown().unwrap();
    server.join().unwrap();
}

#[test]
fn target_replacement_requires_an_explicit_revoking_cut() {
    let (endpoint, server) = spawn_server(|socket| {
        assert_initialize(&read_json(socket).unwrap(), false);
        send_initialize_response(socket, 1);
        assert_initialized(&read_json(socket).unwrap());

        let request = read_json(socket).unwrap();
        assert_eq!(request["method"], "turn/interrupt");
        assert_eq!(request["params"]["turnId"], "turn-successor");
        send_json(socket, r#"{"id":2,"result":{}}"#);
        expect_close(socket);
    });
    let mut session = connect_initialized(endpoint);
    let stale = authorize(&mut session);
    let successor = changed_target(3, 8, 1, "thread-phase67", "turn-successor");

    assert!(matches!(
        session.bind_exact_foreground_turn(successor.clone()),
        Err(ManagedBackendError::ExactForegroundTurnAlreadyBound)
    ));
    assert_eq!(
        session.unbind_exact_foreground_turn().unwrap(),
        Some(target())
    );
    session
        .bind_exact_foreground_turn(successor.clone())
        .unwrap();

    let stale_outcome = session.interrupt_exact_foreground_turn(stale, TIMEOUT);
    assert!(matches!(
        stale_outcome.disposition(),
        TurnInterruptDisposition::ProvenNotDispatched { error }
            if matches!(**error, ManagedBackendError::ExactForegroundAuthorizationStale)
    ));
    let current = authorize_target(&mut session, successor).unwrap();
    let current_outcome = session.interrupt_exact_foreground_turn(current, TIMEOUT);
    assert!(matches!(
        current_outcome.disposition(),
        TurnInterruptDisposition::RequestAccepted
    ));
    session.shutdown().unwrap();
    server.join().unwrap();
}

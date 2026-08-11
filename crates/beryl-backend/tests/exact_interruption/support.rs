use beryl_backend::{
    CallerNoSuccessorFence, ExactForegroundTurn, ExactForegroundTurnAuthorization,
    ManagedBackendClientConnector, ManagedBackendSession, OrderedTurnStreamCompletion,
    OrderedTurnStreamOperation, OrderedTurnStreamSink, OrderedTurnStreamSubmitError,
    StopAttemptCorrelation, StopOperationCorrelation, VolatileInterruptAdmissionFailure,
    VolatileInterruptAuthorization, VolatileInterruptCorrelation,
};
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    CasTurnId, RuntimeId,
};

use crate::websocket::{AUTHORIZATION, TIMEOUT, foreground_config};

pub const THREAD: &str = "thread-phase67";
pub const TURN: &str = "turn-phase67";

pub struct InertSink;

impl OrderedTurnStreamSink for InertSink {
    fn submit(
        &mut self,
        operation: OrderedTurnStreamOperation,
    ) -> Result<OrderedTurnStreamCompletion, OrderedTurnStreamSubmitError> {
        panic!("unexpected ordered operation while awaiting exact response: {operation:?}")
    }
}

pub fn connect_foreground(
    endpoint: beryl_backend::BackendWebSocketEndpoint,
) -> ManagedBackendSession {
    let connector = ManagedBackendClientConnector::for_lifecycle_test(endpoint, AUTHORIZATION);
    connector
        .connect_foreground_candidate(foreground_config(8), TIMEOUT)
        .unwrap()
}

pub fn connect_initialized(
    endpoint: beryl_backend::BackendWebSocketEndpoint,
) -> ManagedBackendSession {
    let mut session = connect_initialized_unbound(endpoint);
    session.bind_exact_foreground_turn(target()).unwrap();
    session
}

pub fn connect_initialized_unbound(
    endpoint: beryl_backend::BackendWebSocketEndpoint,
) -> ManagedBackendSession {
    let mut session = connect_foreground(endpoint);
    session.initialize_foreground(TIMEOUT).unwrap();
    session
        .bind_ordered_turn_stream_sink(Box::new(InertSink))
        .unwrap();
    session
}

pub fn target() -> ExactForegroundTurn {
    ExactForegroundTurn::new(
        RuntimeId::from_bytes([3; 16]),
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(7).unwrap(),
            CasLoadedThreadGeneration::new(11).unwrap(),
        ),
        CasThreadId::new(THREAD).unwrap(),
        CasTurnId::new(TURN).unwrap(),
    )
}

pub fn authorize(session: &mut ManagedBackendSession) -> ExactForegroundTurnAuthorization {
    session
        .authorize_exact_foreground_turn(
            target(),
            StopOperationCorrelation::from_bytes([0xA5; 16]),
            StopAttemptCorrelation::from_bytes([0x5A; 16]),
            CallerNoSuccessorFence::issue(),
        )
        .unwrap()
}

pub fn authorize_volatile(session: &mut ManagedBackendSession) -> VolatileInterruptAuthorization {
    session
        .authorize_volatile_interrupt(
            target(),
            VolatileInterruptAdmissionFailure::WriterReturnedNotCommitted,
            VolatileInterruptCorrelation::from_bytes([0xC3; 16]),
            CallerNoSuccessorFence::issue(),
        )
        .unwrap()
}

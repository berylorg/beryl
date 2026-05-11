use std::{
    fmt,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use beryl_backend::{ManagedBackendClientConnector, ManagedBackendSession};

use super::status_line::CancellableActiveTurn;

pub(super) enum TurnStopUpdate {
    Finished(TurnStopOutcome),
}

pub(super) enum TurnStopOutcome {
    Accepted {
        target: CancellableActiveTurn,
    },
    Failed {
        target: CancellableActiveTurn,
        message: String,
    },
}

pub(crate) trait TurnStopBackend {
    type Error: fmt::Display;

    fn interrupt_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        timeout: Duration,
    ) -> Result<(), Self::Error>;
}

impl TurnStopBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn interrupt_turn(
        &mut self,
        thread_id: &str,
        turn_id: &str,
        timeout: Duration,
    ) -> Result<(), Self::Error> {
        ManagedBackendSession::interrupt_turn(self, thread_id, turn_id, timeout)
    }
}

pub(super) fn spawn_turn_stop_worker(
    connector: ManagedBackendClientConnector,
    target: CancellableActiveTurn,
    timeout: Duration,
) -> Receiver<TurnStopUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || run_turn_stop_worker(connector, target, timeout, sender));
    receiver
}

fn run_turn_stop_worker(
    connector: ManagedBackendClientConnector,
    target: CancellableActiveTurn,
    timeout: Duration,
    sender: Sender<TurnStopUpdate>,
) {
    let outcome = match connector.connect_client(timeout) {
        Ok(mut session) => match request_turn_stop(&mut session, &target, timeout) {
            Ok(()) => TurnStopOutcome::Accepted { target },
            Err(message) => TurnStopOutcome::Failed { target, message },
        },
        Err(error) => TurnStopOutcome::Failed {
            target,
            message: format!(
                "Beryl could not connect to the managed backend to stop the turn: {error}"
            ),
        },
    };

    let _ = sender.send(TurnStopUpdate::Finished(outcome));
}

pub(crate) fn request_turn_stop<B>(
    backend: &mut B,
    target: &CancellableActiveTurn,
    timeout: Duration,
) -> Result<(), String>
where
    B: TurnStopBackend,
{
    backend
        .interrupt_turn(&target.thread_id, &target.turn_id, timeout)
        .map_err(|error| format!("Beryl could not stop the active turn: {error}"))
}

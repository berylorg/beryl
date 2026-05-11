use std::{
    fmt,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use beryl_backend::{
    HardStopTarget, HardStopTargetOutcome, ManagedBackendClientConnector, ManagedBackendSession,
};

use super::status_line::{CancellableActiveTurn, SelectedTurnHardStopTargets};

pub(super) enum HardStopUpdate {
    Finished(HardStopOutcome),
}

pub(super) enum HardStopOutcome {
    Finished {
        selected_turn: CancellableActiveTurn,
        outcomes: Vec<HardStopTargetOutcome>,
    },
    Failed {
        selected_turn: CancellableActiveTurn,
        message: String,
    },
}

pub(crate) trait HardStopBackend {
    type Error: fmt::Display;

    fn request_hard_stop_target(
        &mut self,
        target: &HardStopTarget,
        timeout: Duration,
    ) -> Result<HardStopTargetOutcome, Self::Error>;
}

impl HardStopBackend for ManagedBackendSession {
    type Error = std::convert::Infallible;

    fn request_hard_stop_target(
        &mut self,
        target: &HardStopTarget,
        timeout: Duration,
    ) -> Result<HardStopTargetOutcome, Self::Error> {
        Ok(ManagedBackendSession::request_hard_stop_target(
            self, target, timeout,
        ))
    }
}

pub(super) fn spawn_hard_stop_worker(
    connector: ManagedBackendClientConnector,
    selected_targets: SelectedTurnHardStopTargets,
    timeout: Duration,
) -> Receiver<HardStopUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || run_hard_stop_worker(connector, selected_targets, timeout, sender));
    receiver
}

fn run_hard_stop_worker(
    connector: ManagedBackendClientConnector,
    selected_targets: SelectedTurnHardStopTargets,
    timeout: Duration,
    sender: Sender<HardStopUpdate>,
) {
    let selected_turn = selected_targets.selected_turn.clone();
    let outcome = match connector.connect_client(timeout) {
        Ok(mut session) => {
            let outcomes = request_hard_stop(&mut session, &selected_targets, timeout)
                .unwrap_or_else(|error| {
                    vec![HardStopTargetOutcome::Failed {
                        target: HardStopTarget::turn(
                            selected_turn.thread_id.clone(),
                            selected_turn.turn_id.clone(),
                        ),
                        method: "turn/interrupt",
                        message: format!("Beryl could not request hard stop: {error}"),
                    }]
                });
            HardStopOutcome::Finished {
                selected_turn,
                outcomes,
            }
        }
        Err(error) => HardStopOutcome::Failed {
            selected_turn,
            message: format!(
                "Beryl could not connect to the managed backend to hard stop the turn: {error}"
            ),
        },
    };

    let _ = sender.send(HardStopUpdate::Finished(outcome));
}

pub(crate) fn request_hard_stop<B>(
    backend: &mut B,
    selected_targets: &SelectedTurnHardStopTargets,
    timeout: Duration,
) -> Result<Vec<HardStopTargetOutcome>, String>
where
    B: HardStopBackend,
{
    let targets = ordered_hard_stop_targets(selected_targets);
    let mut outcomes = Vec::with_capacity(targets.len());
    for target in targets {
        let outcome = backend
            .request_hard_stop_target(&target, timeout)
            .map_err(|error| error.to_string())?;
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

fn ordered_hard_stop_targets(
    selected_targets: &SelectedTurnHardStopTargets,
) -> Vec<HardStopTarget> {
    let mut targets = Vec::with_capacity(selected_targets.targets.len().saturating_add(1));
    push_unique_target(
        &mut targets,
        HardStopTarget::turn(
            selected_targets.selected_turn.thread_id.clone(),
            selected_targets.selected_turn.turn_id.clone(),
        ),
    );
    for target in &selected_targets.targets {
        push_unique_target(&mut targets, target.clone());
    }
    targets
}

fn push_unique_target(targets: &mut Vec<HardStopTarget>, target: HardStopTarget) {
    if !targets.contains(&target) {
        targets.push(target);
    }
}

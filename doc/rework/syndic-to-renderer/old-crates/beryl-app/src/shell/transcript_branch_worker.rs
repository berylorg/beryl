use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_backend::ManagedBackendClientConnector;
use beryl_model::workspace::WorkspaceId;

use super::{
    transcript_branch_core::{TranscriptBranchOutcome, run_transcript_branch},
    transcript_branch_menu_state::TranscriptBranchRequest,
    turn_worker::TurnWorkerUpdate,
};

mod foreground;
mod handlers;

pub(super) enum TranscriptBranchUpdate {
    Finished(TranscriptBranchOutcome),
}

pub(super) fn spawn_transcript_branch_worker(
    connector: ManagedBackendClientConnector,
    request: TranscriptBranchRequest,
    timeout: Duration,
) -> Receiver<TranscriptBranchUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let outcome = run_transcript_branch_worker(connector, request, timeout);
        let _ = sender.send(TranscriptBranchUpdate::Finished(outcome));
    });
    receiver
}

pub(super) fn spawn_foreground_transcript_branch_worker(
    connector: ManagedBackendClientConnector,
    request: TranscriptBranchRequest,
    execution_target: WorkspaceId,
    timeout: Duration,
) -> Receiver<TurnWorkerUpdate> {
    foreground::spawn_foreground_transcript_branch_worker(
        connector,
        request,
        execution_target,
        timeout,
    )
}

fn run_transcript_branch_worker(
    connector: ManagedBackendClientConnector,
    request: TranscriptBranchRequest,
    timeout: Duration,
) -> TranscriptBranchOutcome {
    let action = request.action();
    let source_thread_id = request.target().source_thread_id().to_string();
    let source_turn_id = request.target().source_turn_id().to_string();

    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            return TranscriptBranchOutcome::Failed {
                action,
                source_thread_id,
                source_turn_id,
                message: format!("Beryl could not connect to the managed backend: {error}"),
            };
        }
    };

    run_transcript_branch(&mut session, request, timeout)
}

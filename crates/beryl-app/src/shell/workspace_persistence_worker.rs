use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError},
    },
    thread,
    time::Duration,
};

use beryl_model::{
    conversation::{
        ConversationThreadId, ConversationThreadTokenUsageSnapshot, WorkspaceConversationState,
    },
    workspace::BerylWorkspaceId,
};
use tracing::warn;

use crate::{BerylWorkspacePersistence, WorkspaceUiState};

#[derive(Debug)]
pub(super) struct WorkspacePersistenceQueue {
    sender: Sender<WorkspacePersistenceCommand>,
    pending_work: Arc<AtomicUsize>,
}

#[derive(Debug)]
pub(super) struct WorkspacePersistenceFlush {
    receiver: Receiver<Result<(), String>>,
}

#[derive(Debug)]
enum WorkspacePersistenceCommand {
    SaveWorkspaceState {
        workspace_id: BerylWorkspaceId,
        state: WorkspaceConversationState,
        touch_manifest: bool,
    },
    SaveWorkspaceUiState {
        workspace_id: BerylWorkspaceId,
        state: WorkspaceUiState,
    },
    RecordTokenUsageSnapshot {
        workspace_id: BerylWorkspaceId,
        thread_id: ConversationThreadId,
        turn_id: String,
        snapshot: ConversationThreadTokenUsageSnapshot,
    },
    MarkImageAssetsReferenced {
        workspace_id: BerylWorkspaceId,
        asset_ids: Vec<String>,
    },
    MarkImageAssetsRetained {
        workspace_id: BerylWorkspaceId,
        asset_ids: Vec<String>,
    },
    MarkImageAssetsUnreferenced {
        workspace_id: BerylWorkspaceId,
        asset_ids: Vec<String>,
    },
    Flush {
        responder: Sender<Result<(), String>>,
    },
}

struct PendingTokenUsageSnapshot {
    workspace_id: BerylWorkspaceId,
    thread_id: ConversationThreadId,
    turn_id: String,
    snapshot: ConversationThreadTokenUsageSnapshot,
}

pub(super) fn spawn_workspace_persistence_worker(
    persistence: Result<BerylWorkspacePersistence, String>,
) -> WorkspacePersistenceQueue {
    let (sender, receiver) = mpsc::channel();
    let pending_work = Arc::new(AtomicUsize::new(0));
    thread::spawn({
        let pending_work = pending_work.clone();
        move || run_workspace_persistence_worker(receiver, pending_work, persistence)
    });
    WorkspacePersistenceQueue {
        sender,
        pending_work,
    }
}

impl WorkspacePersistenceQueue {
    fn send_command(&self, command: WorkspacePersistenceCommand) -> Result<(), ()> {
        self.pending_work.fetch_add(1, Ordering::AcqRel);
        if self.sender.send(command).is_err() {
            self.pending_work.fetch_sub(1, Ordering::AcqRel);
            return Err(());
        }
        Ok(())
    }

    pub(super) fn has_pending_work(&self) -> bool {
        self.pending_work.load(Ordering::Acquire) > 0
    }

    pub(super) fn save_workspace_state(
        &self,
        workspace_id: BerylWorkspaceId,
        state: WorkspaceConversationState,
        touch_manifest: bool,
    ) {
        if self
            .send_command(WorkspacePersistenceCommand::SaveWorkspaceState {
                workspace_id: workspace_id.clone(),
                state,
                touch_manifest,
            })
            .is_err()
        {
            warn!(
                workspace_id = workspace_id.as_str(),
                "workspace persistence worker stopped before saving workspace state"
            );
        }
    }

    pub(super) fn save_workspace_ui_state(
        &self,
        workspace_id: BerylWorkspaceId,
        state: WorkspaceUiState,
    ) {
        if self
            .send_command(WorkspacePersistenceCommand::SaveWorkspaceUiState {
                workspace_id: workspace_id.clone(),
                state,
            })
            .is_err()
        {
            warn!(
                workspace_id = workspace_id.as_str(),
                "workspace persistence worker stopped before saving workspace UI state"
            );
        }
    }

    pub(super) fn record_token_usage_snapshot(
        &self,
        workspace_id: BerylWorkspaceId,
        thread_id: ConversationThreadId,
        turn_id: String,
        snapshot: ConversationThreadTokenUsageSnapshot,
    ) {
        if self
            .send_command(WorkspacePersistenceCommand::RecordTokenUsageSnapshot {
                workspace_id: workspace_id.clone(),
                thread_id,
                turn_id,
                snapshot,
            })
            .is_err()
        {
            warn!(
                workspace_id = workspace_id.as_str(),
                "workspace persistence worker stopped before saving token usage snapshot"
            );
        }
    }

    pub(super) fn mark_image_assets_referenced(
        &self,
        workspace_id: BerylWorkspaceId,
        asset_ids: Vec<String>,
    ) {
        if asset_ids.is_empty() {
            return;
        }
        if self
            .send_command(WorkspacePersistenceCommand::MarkImageAssetsReferenced {
                workspace_id: workspace_id.clone(),
                asset_ids,
            })
            .is_err()
        {
            warn!(
                workspace_id = workspace_id.as_str(),
                "workspace persistence worker stopped before marking image assets referenced"
            );
        }
    }

    pub(super) fn mark_image_assets_retained(
        &self,
        workspace_id: BerylWorkspaceId,
        asset_ids: Vec<String>,
    ) {
        if asset_ids.is_empty() {
            return;
        }
        if self
            .send_command(WorkspacePersistenceCommand::MarkImageAssetsRetained {
                workspace_id: workspace_id.clone(),
                asset_ids,
            })
            .is_err()
        {
            warn!(
                workspace_id = workspace_id.as_str(),
                "workspace persistence worker stopped before marking image assets retained"
            );
        }
    }

    pub(super) fn mark_image_assets_unreferenced(
        &self,
        workspace_id: BerylWorkspaceId,
        asset_ids: Vec<String>,
    ) {
        if asset_ids.is_empty() {
            return;
        }
        if self
            .send_command(WorkspacePersistenceCommand::MarkImageAssetsUnreferenced {
                workspace_id: workspace_id.clone(),
                asset_ids,
            })
            .is_err()
        {
            warn!(
                workspace_id = workspace_id.as_str(),
                "workspace persistence worker stopped before marking image assets unreferenced"
            );
        }
    }

    pub(super) fn flush(&self) -> WorkspacePersistenceFlush {
        let (responder, receiver) = mpsc::channel();
        if self
            .send_command(WorkspacePersistenceCommand::Flush {
                responder: responder.clone(),
            })
            .is_err()
        {
            let _ = responder.send(Err(
                "workspace persistence worker stopped before confirming pending writes".to_string(),
            ));
        }
        WorkspacePersistenceFlush { receiver }
    }
}

impl WorkspacePersistenceFlush {
    pub(super) fn wait(self, timeout: Duration) -> Result<(), String> {
        match self.receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(format!(
                "timed out after {timeout:?} waiting for workspace persistence to flush"
            )),
            Err(RecvTimeoutError::Disconnected) => Err(
                "workspace persistence worker stopped before confirming pending writes".to_string(),
            ),
        }
    }
}

fn run_workspace_persistence_worker(
    receiver: Receiver<WorkspacePersistenceCommand>,
    pending_work: Arc<AtomicUsize>,
    persistence: Result<BerylWorkspacePersistence, String>,
) {
    let persistence = match persistence {
        Ok(persistence) => persistence,
        Err(error) => {
            while let Ok(command) = receiver.recv() {
                log_persistence_unavailable(&command, &error);
                if let WorkspacePersistenceCommand::Flush { responder } = command {
                    let _ = responder.send(Err(error.clone()));
                }
                pending_work.fetch_sub(1, Ordering::AcqRel);
            }
            return;
        }
    };

    while let Ok(first_command) = receiver.recv() {
        let commands = collect_command_batch_until_flush(first_command, &receiver);
        let command_count = commands.len();
        persist_command_batch(&persistence, commands);
        pending_work.fetch_sub(command_count, Ordering::AcqRel);
    }
}

fn collect_command_batch_until_flush(
    first_command: WorkspacePersistenceCommand,
    receiver: &Receiver<WorkspacePersistenceCommand>,
) -> Vec<WorkspacePersistenceCommand> {
    let mut commands = vec![first_command];
    if matches!(
        commands.last(),
        Some(WorkspacePersistenceCommand::Flush { .. })
    ) {
        return commands;
    }

    loop {
        match receiver.try_recv() {
            Ok(command) => {
                let is_flush = matches!(command, WorkspacePersistenceCommand::Flush { .. });
                commands.push(command);
                if is_flush {
                    break;
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }

    commands
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspacePersistenceCommandKindForTest {
    Write,
    Flush,
}

#[cfg(test)]
pub(crate) fn collect_workspace_persistence_batch_kinds_for_test(
    first_command: WorkspacePersistenceCommandKindForTest,
    queued_commands: &[WorkspacePersistenceCommandKindForTest],
) -> Vec<WorkspacePersistenceCommandKindForTest> {
    let (sender, receiver) = mpsc::channel();
    for command in queued_commands {
        sender
            .send(workspace_persistence_command_for_test(*command))
            .unwrap();
    }
    drop(sender);

    collect_command_batch_until_flush(
        workspace_persistence_command_for_test(first_command),
        &receiver,
    )
    .iter()
    .map(workspace_persistence_command_kind_for_test)
    .collect()
}

#[cfg(test)]
fn workspace_persistence_command_for_test(
    kind: WorkspacePersistenceCommandKindForTest,
) -> WorkspacePersistenceCommand {
    match kind {
        WorkspacePersistenceCommandKindForTest::Write => {
            WorkspacePersistenceCommand::MarkImageAssetsReferenced {
                workspace_id: BerylWorkspaceId::new("workspace").unwrap(),
                asset_ids: vec!["asset".to_string()],
            }
        }
        WorkspacePersistenceCommandKindForTest::Flush => {
            let (responder, _receiver) = mpsc::channel();
            WorkspacePersistenceCommand::Flush { responder }
        }
    }
}

#[cfg(test)]
fn workspace_persistence_command_kind_for_test(
    command: &WorkspacePersistenceCommand,
) -> WorkspacePersistenceCommandKindForTest {
    match command {
        WorkspacePersistenceCommand::Flush { .. } => WorkspacePersistenceCommandKindForTest::Flush,
        WorkspacePersistenceCommand::SaveWorkspaceState { .. }
        | WorkspacePersistenceCommand::SaveWorkspaceUiState { .. }
        | WorkspacePersistenceCommand::RecordTokenUsageSnapshot { .. }
        | WorkspacePersistenceCommand::MarkImageAssetsReferenced { .. }
        | WorkspacePersistenceCommand::MarkImageAssetsRetained { .. }
        | WorkspacePersistenceCommand::MarkImageAssetsUnreferenced { .. } => {
            WorkspacePersistenceCommandKindForTest::Write
        }
    }
}

fn persist_command_batch(
    persistence: &BerylWorkspacePersistence,
    commands: Vec<WorkspacePersistenceCommand>,
) {
    let mut pending_token_snapshots: HashMap<(String, String), PendingTokenUsageSnapshot> =
        HashMap::new();

    for command in commands {
        match command {
            WorkspacePersistenceCommand::RecordTokenUsageSnapshot {
                workspace_id,
                thread_id,
                turn_id,
                snapshot,
            } => {
                pending_token_snapshots.insert(
                    (
                        workspace_id.as_str().to_string(),
                        thread_id.as_str().to_string(),
                    ),
                    PendingTokenUsageSnapshot {
                        workspace_id,
                        thread_id,
                        turn_id,
                        snapshot,
                    },
                );
            }
            WorkspacePersistenceCommand::Flush { responder } => {
                flush_token_snapshots(persistence, &mut pending_token_snapshots);
                let _ = responder.send(Ok(()));
            }
            WorkspacePersistenceCommand::SaveWorkspaceState {
                workspace_id,
                state,
                touch_manifest,
            } => {
                flush_token_snapshots(persistence, &mut pending_token_snapshots);
                persist_workspace_state(persistence, workspace_id, state, touch_manifest);
            }
            WorkspacePersistenceCommand::SaveWorkspaceUiState {
                workspace_id,
                state,
            } => {
                flush_token_snapshots(persistence, &mut pending_token_snapshots);
                persist_workspace_ui_state(persistence, workspace_id, state);
            }
            WorkspacePersistenceCommand::MarkImageAssetsReferenced {
                workspace_id,
                asset_ids,
            } => {
                flush_token_snapshots(persistence, &mut pending_token_snapshots);
                mark_image_assets_referenced(persistence, workspace_id, asset_ids);
            }
            WorkspacePersistenceCommand::MarkImageAssetsRetained {
                workspace_id,
                asset_ids,
            } => {
                flush_token_snapshots(persistence, &mut pending_token_snapshots);
                mark_image_assets_retained(persistence, workspace_id, asset_ids);
            }
            WorkspacePersistenceCommand::MarkImageAssetsUnreferenced {
                workspace_id,
                asset_ids,
            } => {
                flush_token_snapshots(persistence, &mut pending_token_snapshots);
                mark_image_assets_unreferenced(persistence, workspace_id, asset_ids);
            }
        }
    }

    flush_token_snapshots(persistence, &mut pending_token_snapshots);
}

fn flush_token_snapshots(
    persistence: &BerylWorkspacePersistence,
    pending_token_snapshots: &mut HashMap<(String, String), PendingTokenUsageSnapshot>,
) {
    for pending in pending_token_snapshots.drain().map(|(_, pending)| pending) {
        if let Err(error) = persistence.record_thread_token_usage_snapshot(
            &pending.workspace_id,
            &pending.thread_id,
            pending.snapshot,
        ) {
            warn!(
                workspace_id = pending.workspace_id.as_str(),
                thread_id = pending.thread_id.as_str(),
                turn_id = pending.turn_id,
                error = %error,
                "failed to persist token usage snapshot"
            );
        }
    }
}

fn persist_workspace_state(
    persistence: &BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    state: WorkspaceConversationState,
    touch_manifest: bool,
) {
    if let Err(error) = persistence.save_workspace_state(&workspace_id, &state) {
        warn!(
            workspace_id = workspace_id.as_str(),
            error = %error,
            "failed to persist workspace execution-target state"
        );
        return;
    }

    if !touch_manifest {
        return;
    }

    if let Err(error) = persistence.touch_workspace_manifest(&workspace_id) {
        warn!(
            workspace_id = workspace_id.as_str(),
            error = %error,
            "failed to update workspace manifest timestamp after execution-target changes"
        );
    }
}

fn persist_workspace_ui_state(
    persistence: &BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    state: WorkspaceUiState,
) {
    if let Err(error) = persistence.save_workspace_ui_state(&workspace_id, &state) {
        warn!(
            workspace_id = workspace_id.as_str(),
            error = %error,
            "failed to persist workspace UI state"
        );
    }
}

fn mark_image_assets_referenced(
    persistence: &BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    asset_ids: Vec<String>,
) {
    for asset_id in asset_ids {
        if let Err(error) =
            persistence.mark_workspace_image_asset_referenced(&workspace_id, &asset_id)
        {
            warn!(
                workspace_id = workspace_id.as_str(),
                asset_id,
                error = %error,
                "failed to mark workspace image asset referenced"
            );
        }
    }
}

fn mark_image_assets_retained(
    persistence: &BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    asset_ids: Vec<String>,
) {
    for asset_id in asset_ids {
        if let Err(error) =
            persistence.mark_workspace_image_asset_retained(&workspace_id, &asset_id)
        {
            warn!(
                workspace_id = workspace_id.as_str(),
                asset_id,
                error = %error,
                "failed to mark workspace image asset retained"
            );
        }
    }
}

fn mark_image_assets_unreferenced(
    persistence: &BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    asset_ids: Vec<String>,
) {
    for asset_id in asset_ids {
        if let Err(error) =
            persistence.mark_workspace_image_asset_unreferenced(&workspace_id, &asset_id)
        {
            warn!(
                workspace_id = workspace_id.as_str(),
                asset_id,
                error = %error,
                "failed to mark workspace image asset unreferenced"
            );
        }
    }
}

fn log_persistence_unavailable(command: &WorkspacePersistenceCommand, error: &str) {
    match command {
        WorkspacePersistenceCommand::SaveWorkspaceState { workspace_id, .. } => {
            warn!(
                workspace_id = workspace_id.as_str(),
                error = %error,
                "failed to access workspace persistence while saving execution-target state"
            );
        }
        WorkspacePersistenceCommand::SaveWorkspaceUiState { workspace_id, .. } => {
            warn!(
                workspace_id = workspace_id.as_str(),
                error = %error,
                "failed to access workspace persistence while saving workspace UI state"
            );
        }
        WorkspacePersistenceCommand::RecordTokenUsageSnapshot {
            workspace_id,
            thread_id,
            turn_id,
            ..
        } => {
            warn!(
                workspace_id = workspace_id.as_str(),
                thread_id = thread_id.as_str(),
                turn_id,
                error = %error,
                "failed to access workspace persistence while saving token usage snapshot"
            );
        }
        WorkspacePersistenceCommand::MarkImageAssetsReferenced { workspace_id, .. }
        | WorkspacePersistenceCommand::MarkImageAssetsRetained { workspace_id, .. }
        | WorkspacePersistenceCommand::MarkImageAssetsUnreferenced { workspace_id, .. } => {
            warn!(
                workspace_id = workspace_id.as_str(),
                error = %error,
                "failed to access workspace persistence while updating image asset metadata"
            );
        }
        WorkspacePersistenceCommand::Flush { .. } => {
            warn!(
                error = %error,
                "failed to access workspace persistence while flushing pending workspace writes"
            );
        }
    }
}

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
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
    shared: Arc<WorkspacePersistenceShared>,
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

#[derive(Debug)]
struct WorkspacePersistenceShared {
    state: Mutex<WorkspacePersistenceQueueState>,
    available: Condvar,
    pending_work: AtomicUsize,
}

#[derive(Debug, Default)]
struct WorkspacePersistenceQueueState {
    commands: VecDeque<WorkspacePersistenceCommand>,
    closed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImageAssetMarkKind {
    Referenced,
    Retained,
    Unreferenced,
}

struct PendingTokenUsageSnapshot {
    workspace_id: BerylWorkspaceId,
    thread_id: ConversationThreadId,
    turn_id: String,
    snapshot: ConversationThreadTokenUsageSnapshot,
}

impl WorkspacePersistenceShared {
    fn new() -> Self {
        Self {
            state: Mutex::new(WorkspacePersistenceQueueState::default()),
            available: Condvar::new(),
            pending_work: AtomicUsize::new(0),
        }
    }
}

impl WorkspacePersistenceQueueState {
    fn push_or_coalesce(&mut self, command: WorkspacePersistenceCommand) -> bool {
        let tail_start = self.tail_start_after_last_flush();
        match command {
            WorkspacePersistenceCommand::SaveWorkspaceState {
                workspace_id,
                state,
                touch_manifest,
            } => {
                let mut touch_manifest = touch_manifest;
                if let Some(index) = self.workspace_state_command_index(tail_start, &workspace_id) {
                    if let Some(WorkspacePersistenceCommand::SaveWorkspaceState {
                        touch_manifest: previous_touch_manifest,
                        ..
                    }) = self.commands.remove(index)
                    {
                        touch_manifest |= previous_touch_manifest;
                    }
                    self.commands
                        .push_back(WorkspacePersistenceCommand::SaveWorkspaceState {
                            workspace_id,
                            state,
                            touch_manifest,
                        });
                    false
                } else {
                    self.commands
                        .push_back(WorkspacePersistenceCommand::SaveWorkspaceState {
                            workspace_id,
                            state,
                            touch_manifest,
                        });
                    true
                }
            }
            WorkspacePersistenceCommand::SaveWorkspaceUiState {
                workspace_id,
                state,
            } => {
                if let Some(index) =
                    self.workspace_ui_state_command_index(tail_start, &workspace_id)
                {
                    let _ = self.commands.remove(index);
                    self.commands
                        .push_back(WorkspacePersistenceCommand::SaveWorkspaceUiState {
                            workspace_id,
                            state,
                        });
                    false
                } else {
                    self.commands
                        .push_back(WorkspacePersistenceCommand::SaveWorkspaceUiState {
                            workspace_id,
                            state,
                        });
                    true
                }
            }
            WorkspacePersistenceCommand::RecordTokenUsageSnapshot {
                workspace_id,
                thread_id,
                turn_id,
                snapshot,
            } => {
                if let Some(index) =
                    self.token_snapshot_command_index(tail_start, &workspace_id, &thread_id)
                {
                    let _ = self.commands.remove(index);
                    self.commands.push_back(
                        WorkspacePersistenceCommand::RecordTokenUsageSnapshot {
                            workspace_id,
                            thread_id,
                            turn_id,
                            snapshot,
                        },
                    );
                    false
                } else {
                    self.commands.push_back(
                        WorkspacePersistenceCommand::RecordTokenUsageSnapshot {
                            workspace_id,
                            thread_id,
                            turn_id,
                            snapshot,
                        },
                    );
                    true
                }
            }
            WorkspacePersistenceCommand::MarkImageAssetsReferenced {
                workspace_id,
                asset_ids,
            } => self.push_image_asset_mark_or_coalesce(
                ImageAssetMarkKind::Referenced,
                workspace_id,
                asset_ids,
            ),
            WorkspacePersistenceCommand::MarkImageAssetsRetained {
                workspace_id,
                asset_ids,
            } => self.push_image_asset_mark_or_coalesce(
                ImageAssetMarkKind::Retained,
                workspace_id,
                asset_ids,
            ),
            WorkspacePersistenceCommand::MarkImageAssetsUnreferenced {
                workspace_id,
                asset_ids,
            } => self.push_image_asset_mark_or_coalesce(
                ImageAssetMarkKind::Unreferenced,
                workspace_id,
                asset_ids,
            ),
            WorkspacePersistenceCommand::Flush { responder } => {
                self.commands
                    .push_back(WorkspacePersistenceCommand::Flush { responder });
                true
            }
        }
    }

    fn pop_batch_until_flush(&mut self) -> Vec<WorkspacePersistenceCommand> {
        let mut commands = Vec::new();
        while let Some(command) = self.commands.pop_front() {
            let is_flush = matches!(command, WorkspacePersistenceCommand::Flush { .. });
            commands.push(command);
            if is_flush {
                break;
            }
        }
        commands
    }

    fn tail_start_after_last_flush(&self) -> usize {
        self.commands
            .iter()
            .rposition(|command| matches!(command, WorkspacePersistenceCommand::Flush { .. }))
            .map_or(0, |index| index + 1)
    }

    fn workspace_state_command_index(
        &self,
        tail_start: usize,
        workspace_id: &BerylWorkspaceId,
    ) -> Option<usize> {
        self.commands
            .iter()
            .enumerate()
            .skip(tail_start)
            .find_map(|(index, command)| match command {
                WorkspacePersistenceCommand::SaveWorkspaceState {
                    workspace_id: existing_workspace_id,
                    ..
                } if existing_workspace_id == workspace_id => Some(index),
                _ => None,
            })
    }

    fn workspace_ui_state_command_index(
        &self,
        tail_start: usize,
        workspace_id: &BerylWorkspaceId,
    ) -> Option<usize> {
        self.commands
            .iter()
            .enumerate()
            .skip(tail_start)
            .find_map(|(index, command)| match command {
                WorkspacePersistenceCommand::SaveWorkspaceUiState {
                    workspace_id: existing_workspace_id,
                    ..
                } if existing_workspace_id == workspace_id => Some(index),
                _ => None,
            })
    }

    fn token_snapshot_command_index(
        &self,
        tail_start: usize,
        workspace_id: &BerylWorkspaceId,
        thread_id: &ConversationThreadId,
    ) -> Option<usize> {
        self.commands
            .iter()
            .enumerate()
            .skip(tail_start)
            .find_map(|(index, command)| match command {
                WorkspacePersistenceCommand::RecordTokenUsageSnapshot {
                    workspace_id: existing_workspace_id,
                    thread_id: existing_thread_id,
                    ..
                } if existing_workspace_id == workspace_id && existing_thread_id == thread_id => {
                    Some(index)
                }
                _ => None,
            })
    }

    fn push_image_asset_mark_or_coalesce(
        &mut self,
        kind: ImageAssetMarkKind,
        workspace_id: BerylWorkspaceId,
        mut asset_ids: Vec<String>,
    ) -> bool {
        dedupe_asset_ids(&mut asset_ids);
        if asset_ids.is_empty() {
            return false;
        }

        if let Some(existing_asset_ids) = self
            .commands
            .back_mut()
            .and_then(|command| image_asset_mark_asset_ids_mut(command, kind, &workspace_id))
        {
            merge_asset_ids(existing_asset_ids, asset_ids);
            return false;
        }

        self.commands
            .push_back(image_asset_mark_command(kind, workspace_id, asset_ids));
        true
    }
}

pub(super) fn spawn_workspace_persistence_worker(
    persistence: Result<BerylWorkspacePersistence, String>,
) -> WorkspacePersistenceQueue {
    let shared = Arc::new(WorkspacePersistenceShared::new());
    thread::spawn({
        let shared = shared.clone();
        move || run_workspace_persistence_worker(shared, persistence)
    });
    WorkspacePersistenceQueue { shared }
}

impl WorkspacePersistenceQueue {
    fn send_command(&self, command: WorkspacePersistenceCommand) -> Result<(), ()> {
        let mut state = self.shared.state.lock().unwrap();
        if state.closed {
            return Err(());
        }

        if state.push_or_coalesce(command) {
            self.shared.pending_work.fetch_add(1, Ordering::AcqRel);
        }
        drop(state);
        self.shared.available.notify_one();
        Ok(())
    }

    pub(super) fn has_pending_work(&self) -> bool {
        self.shared.pending_work.load(Ordering::Acquire) > 0
    }

    pub(super) fn pending_work_count(&self) -> usize {
        self.shared.pending_work.load(Ordering::Acquire)
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

impl Drop for WorkspacePersistenceQueue {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.closed = true;
        drop(state);
        self.shared.available.notify_one();
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
    shared: Arc<WorkspacePersistenceShared>,
    persistence: Result<BerylWorkspacePersistence, String>,
) {
    let persistence = match persistence {
        Ok(persistence) => persistence,
        Err(error) => {
            run_workspace_persistence_unavailable(shared, error);
            return;
        }
    };

    while let Some(commands) = wait_for_command_batch(&shared) {
        let command_count = commands.len();
        persist_command_batch(&persistence, commands);
        shared
            .pending_work
            .fetch_sub(command_count, Ordering::AcqRel);
    }
}

fn run_workspace_persistence_unavailable(shared: Arc<WorkspacePersistenceShared>, error: String) {
    while let Some(commands) = wait_for_command_batch(&shared) {
        let command_count = commands.len();
        for command in commands {
            log_persistence_unavailable(&command, &error);
            if let WorkspacePersistenceCommand::Flush { responder } = command {
                let _ = responder.send(Err(error.clone()));
            }
        }
        shared
            .pending_work
            .fetch_sub(command_count, Ordering::AcqRel);
    }
}

fn wait_for_command_batch(
    shared: &WorkspacePersistenceShared,
) -> Option<Vec<WorkspacePersistenceCommand>> {
    let mut state = shared.state.lock().unwrap();
    loop {
        if !state.commands.is_empty() {
            return Some(state.pop_batch_until_flush());
        }
        if state.closed {
            return None;
        }
        state = shared.available.wait(state).unwrap();
    }
}

fn image_asset_mark_command(
    kind: ImageAssetMarkKind,
    workspace_id: BerylWorkspaceId,
    asset_ids: Vec<String>,
) -> WorkspacePersistenceCommand {
    match kind {
        ImageAssetMarkKind::Referenced => WorkspacePersistenceCommand::MarkImageAssetsReferenced {
            workspace_id,
            asset_ids,
        },
        ImageAssetMarkKind::Retained => WorkspacePersistenceCommand::MarkImageAssetsRetained {
            workspace_id,
            asset_ids,
        },
        ImageAssetMarkKind::Unreferenced => {
            WorkspacePersistenceCommand::MarkImageAssetsUnreferenced {
                workspace_id,
                asset_ids,
            }
        }
    }
}

fn image_asset_mark_asset_ids_mut<'a>(
    command: &'a mut WorkspacePersistenceCommand,
    kind: ImageAssetMarkKind,
    workspace_id: &BerylWorkspaceId,
) -> Option<&'a mut Vec<String>> {
    match command {
        WorkspacePersistenceCommand::MarkImageAssetsReferenced {
            workspace_id: existing_workspace_id,
            asset_ids,
        } if kind == ImageAssetMarkKind::Referenced && existing_workspace_id == workspace_id => {
            Some(asset_ids)
        }
        WorkspacePersistenceCommand::MarkImageAssetsRetained {
            workspace_id: existing_workspace_id,
            asset_ids,
        } if kind == ImageAssetMarkKind::Retained && existing_workspace_id == workspace_id => {
            Some(asset_ids)
        }
        WorkspacePersistenceCommand::MarkImageAssetsUnreferenced {
            workspace_id: existing_workspace_id,
            asset_ids,
        } if kind == ImageAssetMarkKind::Unreferenced && existing_workspace_id == workspace_id => {
            Some(asset_ids)
        }
        _ => None,
    }
}

fn dedupe_asset_ids(asset_ids: &mut Vec<String>) {
    let mut seen = HashSet::new();
    asset_ids.retain(|asset_id| seen.insert(asset_id.clone()));
}

fn merge_asset_ids(existing_asset_ids: &mut Vec<String>, new_asset_ids: Vec<String>) {
    let mut seen = existing_asset_ids.iter().cloned().collect::<HashSet<_>>();
    for asset_id in new_asset_ids {
        if seen.insert(asset_id.clone()) {
            existing_asset_ids.push(asset_id);
        }
    }
}

fn coalesce_image_asset_marks_for_batch(
    commands: Vec<WorkspacePersistenceCommand>,
) -> Vec<WorkspacePersistenceCommand> {
    let mut coalesced = Vec::with_capacity(commands.len());
    for command in commands {
        match command {
            WorkspacePersistenceCommand::MarkImageAssetsReferenced {
                workspace_id,
                asset_ids,
            } => push_batch_image_asset_mark(
                &mut coalesced,
                ImageAssetMarkKind::Referenced,
                workspace_id,
                asset_ids,
            ),
            WorkspacePersistenceCommand::MarkImageAssetsRetained {
                workspace_id,
                asset_ids,
            } => push_batch_image_asset_mark(
                &mut coalesced,
                ImageAssetMarkKind::Retained,
                workspace_id,
                asset_ids,
            ),
            WorkspacePersistenceCommand::MarkImageAssetsUnreferenced {
                workspace_id,
                asset_ids,
            } => push_batch_image_asset_mark(
                &mut coalesced,
                ImageAssetMarkKind::Unreferenced,
                workspace_id,
                asset_ids,
            ),
            command => coalesced.push(command),
        }
    }
    coalesced
}

fn push_batch_image_asset_mark(
    commands: &mut Vec<WorkspacePersistenceCommand>,
    kind: ImageAssetMarkKind,
    workspace_id: BerylWorkspaceId,
    mut asset_ids: Vec<String>,
) {
    dedupe_asset_ids(&mut asset_ids);
    if asset_ids.is_empty() {
        return;
    }

    if let Some(existing_asset_ids) = commands
        .last_mut()
        .and_then(|command| image_asset_mark_asset_ids_mut(command, kind, &workspace_id))
    {
        merge_asset_ids(existing_asset_ids, asset_ids);
    } else {
        commands.push(image_asset_mark_command(kind, workspace_id, asset_ids));
    }
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
    let mut state = WorkspacePersistenceQueueState::default();
    state.push_or_coalesce(workspace_persistence_command_for_test(first_command));
    for command in queued_commands {
        state.push_or_coalesce(workspace_persistence_command_for_test(*command));
    }

    state
        .pop_batch_until_flush()
        .iter()
        .map(workspace_persistence_command_kind_for_test)
        .collect()
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WorkspacePersistenceCommandForTest {
    WorkspaceState {
        workspace_id: String,
        touch_manifest: bool,
    },
    WorkspaceUiState {
        workspace_id: String,
        panel_height_px: f32,
    },
    TokenSnapshot {
        workspace_id: String,
        thread_id: String,
        turn_id: String,
    },
    MarkReferenced {
        workspace_id: String,
        asset_ids: Vec<String>,
    },
    MarkRetained {
        workspace_id: String,
        asset_ids: Vec<String>,
    },
    MarkUnreferenced {
        workspace_id: String,
        asset_ids: Vec<String>,
    },
    Flush,
}

#[cfg(test)]
pub(crate) fn collect_workspace_persistence_batches_for_test(
    commands: &[WorkspacePersistenceCommandForTest],
) -> Vec<Vec<WorkspacePersistenceCommandForTest>> {
    let mut state = WorkspacePersistenceQueueState::default();
    for command in commands {
        state.push_or_coalesce(workspace_persistence_command_detail_for_test(
            command.clone(),
        ));
    }

    let mut batches = Vec::new();
    while !state.commands.is_empty() {
        batches.push(
            state
                .pop_batch_until_flush()
                .iter()
                .map(workspace_persistence_command_detail_from_command_for_test)
                .collect(),
        );
    }
    batches
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
fn workspace_persistence_command_detail_for_test(
    command: WorkspacePersistenceCommandForTest,
) -> WorkspacePersistenceCommand {
    match command {
        WorkspacePersistenceCommandForTest::WorkspaceState {
            workspace_id,
            touch_manifest,
        } => WorkspacePersistenceCommand::SaveWorkspaceState {
            workspace_id: BerylWorkspaceId::new(workspace_id).unwrap(),
            state: WorkspaceConversationState::default(),
            touch_manifest,
        },
        WorkspacePersistenceCommandForTest::WorkspaceUiState {
            workspace_id,
            panel_height_px,
        } => {
            let mut state = WorkspaceUiState::default();
            state.set_tool_activity_panel_height_px(panel_height_px);
            WorkspacePersistenceCommand::SaveWorkspaceUiState {
                workspace_id: BerylWorkspaceId::new(workspace_id).unwrap(),
                state,
            }
        }
        WorkspacePersistenceCommandForTest::TokenSnapshot {
            workspace_id,
            thread_id,
            turn_id,
        } => WorkspacePersistenceCommand::RecordTokenUsageSnapshot {
            workspace_id: BerylWorkspaceId::new(workspace_id).unwrap(),
            thread_id: ConversationThreadId::new(thread_id),
            turn_id: turn_id.clone(),
            snapshot: token_usage_snapshot_for_test(&turn_id),
        },
        WorkspacePersistenceCommandForTest::MarkReferenced {
            workspace_id,
            asset_ids,
        } => WorkspacePersistenceCommand::MarkImageAssetsReferenced {
            workspace_id: BerylWorkspaceId::new(workspace_id).unwrap(),
            asset_ids,
        },
        WorkspacePersistenceCommandForTest::MarkRetained {
            workspace_id,
            asset_ids,
        } => WorkspacePersistenceCommand::MarkImageAssetsRetained {
            workspace_id: BerylWorkspaceId::new(workspace_id).unwrap(),
            asset_ids,
        },
        WorkspacePersistenceCommandForTest::MarkUnreferenced {
            workspace_id,
            asset_ids,
        } => WorkspacePersistenceCommand::MarkImageAssetsUnreferenced {
            workspace_id: BerylWorkspaceId::new(workspace_id).unwrap(),
            asset_ids,
        },
        WorkspacePersistenceCommandForTest::Flush => {
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

#[cfg(test)]
fn workspace_persistence_command_detail_from_command_for_test(
    command: &WorkspacePersistenceCommand,
) -> WorkspacePersistenceCommandForTest {
    match command {
        WorkspacePersistenceCommand::SaveWorkspaceState {
            workspace_id,
            touch_manifest,
            ..
        } => WorkspacePersistenceCommandForTest::WorkspaceState {
            workspace_id: workspace_id.as_str().to_string(),
            touch_manifest: *touch_manifest,
        },
        WorkspacePersistenceCommand::SaveWorkspaceUiState {
            workspace_id,
            state,
        } => WorkspacePersistenceCommandForTest::WorkspaceUiState {
            workspace_id: workspace_id.as_str().to_string(),
            panel_height_px: state.tool_activity_panel_height_px(),
        },
        WorkspacePersistenceCommand::RecordTokenUsageSnapshot {
            workspace_id,
            thread_id,
            snapshot,
            ..
        } => WorkspacePersistenceCommandForTest::TokenSnapshot {
            workspace_id: workspace_id.as_str().to_string(),
            thread_id: thread_id.as_str().to_string(),
            turn_id: snapshot.turn_id().as_str().to_string(),
        },
        WorkspacePersistenceCommand::MarkImageAssetsReferenced {
            workspace_id,
            asset_ids,
        } => WorkspacePersistenceCommandForTest::MarkReferenced {
            workspace_id: workspace_id.as_str().to_string(),
            asset_ids: asset_ids.clone(),
        },
        WorkspacePersistenceCommand::MarkImageAssetsRetained {
            workspace_id,
            asset_ids,
        } => WorkspacePersistenceCommandForTest::MarkRetained {
            workspace_id: workspace_id.as_str().to_string(),
            asset_ids: asset_ids.clone(),
        },
        WorkspacePersistenceCommand::MarkImageAssetsUnreferenced {
            workspace_id,
            asset_ids,
        } => WorkspacePersistenceCommandForTest::MarkUnreferenced {
            workspace_id: workspace_id.as_str().to_string(),
            asset_ids: asset_ids.clone(),
        },
        WorkspacePersistenceCommand::Flush { .. } => WorkspacePersistenceCommandForTest::Flush,
    }
}

#[cfg(test)]
fn token_usage_snapshot_for_test(turn_id: &str) -> ConversationThreadTokenUsageSnapshot {
    ConversationThreadTokenUsageSnapshot::new(
        beryl_model::conversation::ConversationTurnId::new(turn_id),
        beryl_model::conversation::ConversationTokenUsageBreakdown::new(0, 10, 2, 3, 15),
        beryl_model::conversation::ConversationTokenUsageBreakdown::new(0, 20, 4, 6, 30),
        Some(200_000),
        1,
    )
}

fn persist_command_batch(
    persistence: &BerylWorkspacePersistence,
    commands: Vec<WorkspacePersistenceCommand>,
) {
    let commands = coalesce_image_asset_marks_for_batch(commands);
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

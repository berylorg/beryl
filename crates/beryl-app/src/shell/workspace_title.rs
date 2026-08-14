use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest};

use crate::BerylWorkspacePersistence;
use crate::title_generation::derive_short_title_from_turn;

use super::workspace_persistence_worker::WorkspacePersistenceFlush;

pub(super) enum WorkspaceTitleUpdate {
    Generated {
        workspace_id: BerylWorkspaceId,
        result: WorkspaceTitleResult,
    },
    Manual {
        workspace_id: BerylWorkspaceId,
        result: WorkspaceTitleResult,
    },
}

pub(super) enum WorkspaceTitleResult {
    Updated(WorkspaceTitleChange),
    Unchanged,
    Failed(String),
}

pub(super) struct WorkspaceTitleChange {
    pub(super) old_workspace_id: BerylWorkspaceId,
    pub(super) new_workspace_id: BerylWorkspaceId,
    pub(super) manifest: BerylWorkspaceManifest,
}

impl WorkspaceTitleChange {
    fn new(old_workspace_id: BerylWorkspaceId, manifest: BerylWorkspaceManifest) -> Self {
        Self {
            old_workspace_id,
            new_workspace_id: manifest.id().clone(),
            manifest,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct WorkspaceTitleCandidate {
    workspace_id: BerylWorkspaceId,
    user_input: String,
    assistant_text: String,
}

impl WorkspaceTitleCandidate {
    pub(super) fn new(
        workspace_id: BerylWorkspaceId,
        user_input: impl Into<String>,
        assistant_text: impl Into<String>,
    ) -> Option<Self> {
        let assistant_text = assistant_text.into();
        if assistant_text.trim().is_empty() {
            return None;
        }

        Some(Self {
            workspace_id,
            user_input: user_input.into(),
            assistant_text,
        })
    }

    pub(super) fn workspace_id(&self) -> &BerylWorkspaceId {
        &self.workspace_id
    }
}

pub(super) fn spawn_workspace_title_worker(
    persistence: BerylWorkspacePersistence,
    candidate: WorkspaceTitleCandidate,
    workspace_persistence_flush: WorkspacePersistenceFlush,
    timeout: Duration,
) -> Receiver<WorkspaceTitleUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let workspace_id = candidate.workspace_id.clone();
        let result = wait_for_workspace_persistence_flush(workspace_persistence_flush, timeout)
            .unwrap_or_else(|| generate_and_persist_workspace_title(&persistence, candidate));
        let _ = sender.send(WorkspaceTitleUpdate::Generated {
            workspace_id,
            result,
        });
    });
    receiver
}

pub(super) fn spawn_workspace_manual_title_worker(
    persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    title: String,
    workspace_persistence_flush: WorkspacePersistenceFlush,
    timeout: Duration,
) -> Receiver<WorkspaceTitleUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = wait_for_workspace_persistence_flush(workspace_persistence_flush, timeout)
            .unwrap_or_else(|| persist_manual_workspace_title(&persistence, &workspace_id, title));
        let _ = sender.send(WorkspaceTitleUpdate::Manual {
            workspace_id,
            result,
        });
    });
    receiver
}

fn wait_for_workspace_persistence_flush(
    workspace_persistence_flush: WorkspacePersistenceFlush,
    timeout: Duration,
) -> Option<WorkspaceTitleResult> {
    match workspace_persistence_flush.wait(timeout) {
        Ok(()) => None,
        Err(error) => Some(WorkspaceTitleResult::Failed(error)),
    }
}

fn persist_manual_workspace_title(
    persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    title: String,
) -> WorkspaceTitleResult {
    match persistence.set_workspace_manual_title(workspace_id, title) {
        Ok(Some(manifest)) => {
            WorkspaceTitleResult::Updated(WorkspaceTitleChange::new(workspace_id.clone(), manifest))
        }
        Ok(None) => WorkspaceTitleResult::Unchanged,
        Err(error) => WorkspaceTitleResult::Failed(error.to_string()),
    }
}

fn generate_and_persist_workspace_title(
    persistence: &BerylWorkspacePersistence,
    candidate: WorkspaceTitleCandidate,
) -> WorkspaceTitleResult {
    let Some(title) =
        derive_short_title_from_turn(&candidate.user_input, &candidate.assistant_text)
    else {
        return WorkspaceTitleResult::Unchanged;
    };

    match persistence.set_workspace_generated_title_if_untitled(candidate.workspace_id(), title) {
        Ok(Some(manifest)) => WorkspaceTitleResult::Updated(WorkspaceTitleChange::new(
            candidate.workspace_id().clone(),
            manifest,
        )),
        Ok(None) => WorkspaceTitleResult::Unchanged,
        Err(error) => WorkspaceTitleResult::Failed(error.to_string()),
    }
}

use beryl_model::workspace::{BerylWorkspaceId, BerylWorkspaceManifest};
use thiserror::Error;

use crate::{
    BerylWorkspacePersistence, StartupMetadata, StartupPersistence, StartupPersistenceError,
    WorkspacePersistenceError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedStartupState {
    active_workspace: BerylWorkspaceManifest,
    known_workspaces: Vec<BerylWorkspaceManifest>,
    startup_warning: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceDeletionResolution {
    deleted: bool,
    replacement_workspace: Option<BerylWorkspaceManifest>,
    known_workspaces: Vec<BerylWorkspaceManifest>,
}

#[derive(Debug, Error)]
pub enum StartupStateError {
    #[error(transparent)]
    StartupPersistence(#[from] StartupPersistenceError),
    #[error(transparent)]
    WorkspacePersistence(#[from] WorkspacePersistenceError),
}

impl ResolvedStartupState {
    pub fn active_workspace(&self) -> &BerylWorkspaceManifest {
        &self.active_workspace
    }

    pub fn known_workspaces(&self) -> &[BerylWorkspaceManifest] {
        &self.known_workspaces
    }

    pub fn startup_warning(&self) -> Option<&str> {
        self.startup_warning.as_deref()
    }
}

impl WorkspaceDeletionResolution {
    pub fn deleted(&self) -> bool {
        self.deleted
    }

    pub fn replacement_workspace(&self) -> Option<&BerylWorkspaceManifest> {
        self.replacement_workspace.as_ref()
    }

    pub fn known_workspaces(&self) -> &[BerylWorkspaceManifest] {
        &self.known_workspaces
    }
}

pub fn resolve_startup_state(
    startup_persistence: &StartupPersistence,
    workspace_persistence: &BerylWorkspacePersistence,
) -> Result<ResolvedStartupState, StartupStateError> {
    workspace_persistence.recover_interrupted_workspace_rename(startup_persistence)?;
    let mut metadata = startup_persistence.load()?;
    let mut known_workspaces = workspace_persistence.list_workspace_manifests()?;
    sort_known_workspaces_for_picker(&mut known_workspaces, &metadata);

    let (active_workspace, startup_warning) = select_active_workspace(&metadata, &known_workspaces)
        .map(|workspace| (workspace, None))
        .map(Ok)
        .unwrap_or_else(|| {
            let warning = metadata.last_opened_workspace().map(|workspace_id| {
                format!(
                    "The previously active workspace '{}' is unavailable; Beryl opened a fresh untitled workspace instead.",
                    workspace_id.as_str()
                )
            });
            create_fresh_untitled_workspace_with_metadata(workspace_persistence, &mut metadata)
                .map(|workspace| (workspace, warning))
        })?;

    metadata.remember_workspace(active_workspace.id().clone());
    startup_persistence.save(&metadata)?;
    known_workspaces = workspace_persistence.list_workspace_manifests()?;
    sort_known_workspaces_for_picker(&mut known_workspaces, &metadata);

    Ok(ResolvedStartupState {
        active_workspace,
        known_workspaces,
        startup_warning,
    })
}

fn select_active_workspace(
    metadata: &StartupMetadata,
    known_workspaces: &[BerylWorkspaceManifest],
) -> Option<BerylWorkspaceManifest> {
    let last_opened = metadata.last_opened_workspace()?;
    known_workspaces
        .iter()
        .find(|workspace| workspace.id() == last_opened)
        .cloned()
}

pub(crate) fn resolve_known_workspaces(
    startup_persistence: &StartupPersistence,
    workspace_persistence: &BerylWorkspacePersistence,
) -> Result<Vec<BerylWorkspaceManifest>, StartupStateError> {
    workspace_persistence.recover_interrupted_workspace_rename(startup_persistence)?;
    let metadata = startup_persistence.load()?;
    let mut known_workspaces = workspace_persistence.list_workspace_manifests()?;
    sort_known_workspaces_for_picker(&mut known_workspaces, &metadata);
    Ok(known_workspaces)
}

pub(crate) fn sort_known_workspaces_for_picker(
    known_workspaces: &mut [BerylWorkspaceManifest],
    metadata: &StartupMetadata,
) {
    known_workspaces.sort_by(|left, right| {
        recent_workspace_rank(metadata, left.id())
            .cmp(&recent_workspace_rank(metadata, right.id()))
            .then_with(|| left.title().cmp(right.title()))
            .then_with(|| left.id().as_str().cmp(right.id().as_str()))
    });
}

pub(crate) fn create_fresh_untitled_workspace_with_metadata(
    workspace_persistence: &BerylWorkspacePersistence,
    metadata: &mut StartupMetadata,
) -> Result<BerylWorkspaceManifest, StartupStateError> {
    loop {
        let sequence = metadata.allocate_untitled_workspace_sequence();
        if let Some(workspace) = workspace_persistence.create_untitled_workspace(sequence)? {
            return Ok(workspace);
        }
    }
}

pub fn create_fresh_untitled_workspace(
    startup_persistence: &StartupPersistence,
    workspace_persistence: &BerylWorkspacePersistence,
) -> Result<BerylWorkspaceManifest, StartupStateError> {
    let mut metadata = startup_persistence.load()?;
    let workspace =
        create_fresh_untitled_workspace_with_metadata(workspace_persistence, &mut metadata)?;
    metadata.remember_workspace(workspace.id().clone());
    startup_persistence.save(&metadata)?;
    Ok(workspace)
}

pub fn delete_workspace_and_resolve_active_replacement(
    startup_persistence: &StartupPersistence,
    workspace_persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    active_workspace_id: &BerylWorkspaceId,
) -> Result<WorkspaceDeletionResolution, StartupStateError> {
    let deleted = workspace_persistence.delete_workspace(workspace_id)?;
    let mut metadata = startup_persistence.load()?;
    metadata.forget_workspace(workspace_id);

    let replacement_workspace = if workspace_id == active_workspace_id {
        let workspace =
            create_fresh_untitled_workspace_with_metadata(workspace_persistence, &mut metadata)?;
        metadata.remember_workspace(workspace.id().clone());
        Some(workspace)
    } else {
        metadata.remember_workspace(active_workspace_id.clone());
        None
    };
    startup_persistence.save(&metadata)?;

    let mut known_workspaces = workspace_persistence.list_workspace_manifests()?;
    sort_known_workspaces_for_picker(&mut known_workspaces, &metadata);

    Ok(WorkspaceDeletionResolution {
        deleted,
        replacement_workspace,
        known_workspaces,
    })
}

fn recent_workspace_rank(metadata: &StartupMetadata, workspace_id: &BerylWorkspaceId) -> usize {
    metadata
        .recent_workspaces()
        .iter()
        .position(|candidate| candidate == workspace_id)
        .unwrap_or(usize::MAX)
}

use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use beryl_model::{
    conversation::WorkspaceConversationState,
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest, WorkspaceId},
};

use crate::{BerylWorkspacePersistence, StartupPersistence, WorkspaceUiState};
use tracing::warn;

use super::{
    workspace_members::apply_primary_execution_target_selection,
    workspace_persistence_worker::WorkspacePersistenceFlush,
    workspace_picker::{
        WorkspacePickerMemberPaths, explicit_member_path_strings,
        workspace_picker_member_paths_from_states,
    },
};

pub(super) enum WorkspacePickerActionUpdate {
    Created(Result<WorkspacePickerOpenedWorkspace, String>),
    Switched(Result<WorkspacePickerOpenedWorkspace, String>),
    Deleted {
        workspace_id: BerylWorkspaceId,
        result: Result<WorkspacePickerDeletionOutcome, String>,
    },
}

pub(super) struct WorkspacePickerOpenedWorkspace {
    pub(super) workspace: BerylWorkspaceManifest,
    pub(super) known_workspaces: Vec<BerylWorkspaceManifest>,
    pub(super) workspace_picker_member_paths: WorkspacePickerMemberPaths,
    pub(super) workspace_state: WorkspaceConversationState,
    pub(super) workspace_ui_state: WorkspaceUiState,
}

pub(super) struct WorkspacePickerDeletionOutcome {
    pub(super) deleted: bool,
    pub(super) replacement_workspace: Option<BerylWorkspaceManifest>,
    pub(super) replacement_workspace_state: Option<WorkspaceConversationState>,
    pub(super) replacement_workspace_ui_state: Option<WorkspaceUiState>,
    pub(super) known_workspaces: Vec<BerylWorkspaceManifest>,
    pub(super) workspace_picker_member_paths: WorkspacePickerMemberPaths,
}

pub(super) fn spawn_create_workspace_worker(
    startup_persistence: StartupPersistence,
    workspace_persistence: BerylWorkspacePersistence,
    workspace_persistence_flush: WorkspacePersistenceFlush,
    timeout: Duration,
) -> Receiver<WorkspacePickerActionUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = workspace_persistence_flush.wait(timeout).and_then(|()| {
            create_workspace_for_picker(&startup_persistence, &workspace_persistence)
        });
        let _ = sender.send(WorkspacePickerActionUpdate::Created(result));
    });
    receiver
}

pub(super) fn spawn_create_workspace_for_target_worker(
    startup_persistence: StartupPersistence,
    workspace_persistence: BerylWorkspacePersistence,
    execution_target: WorkspaceId,
    workspace_persistence_flush: WorkspacePersistenceFlush,
    timeout: Duration,
) -> Receiver<WorkspacePickerActionUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = workspace_persistence_flush.wait(timeout).and_then(|()| {
            create_workspace_for_target(
                &startup_persistence,
                &workspace_persistence,
                execution_target,
            )
        });
        let _ = sender.send(WorkspacePickerActionUpdate::Created(result));
    });
    receiver
}

pub(super) fn spawn_switch_workspace_worker(
    startup_persistence: StartupPersistence,
    workspace_persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    workspace_persistence_flush: WorkspacePersistenceFlush,
    timeout: Duration,
) -> Receiver<WorkspacePickerActionUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = workspace_persistence_flush.wait(timeout).and_then(|()| {
            switch_workspace_for_picker(&startup_persistence, &workspace_persistence, workspace_id)
        });
        let _ = sender.send(WorkspacePickerActionUpdate::Switched(result));
    });
    receiver
}

pub(super) fn spawn_delete_workspace_worker(
    startup_persistence: StartupPersistence,
    workspace_persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    active_workspace_id: BerylWorkspaceId,
    workspace_persistence_flush: WorkspacePersistenceFlush,
    timeout: Duration,
) -> Receiver<WorkspacePickerActionUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = workspace_persistence_flush.wait(timeout).and_then(|()| {
            delete_workspace_for_picker(
                &startup_persistence,
                &workspace_persistence,
                &workspace_id,
                &active_workspace_id,
            )
        });
        let _ = sender.send(WorkspacePickerActionUpdate::Deleted {
            workspace_id,
            result,
        });
    });
    receiver
}

fn create_workspace_for_picker(
    startup_persistence: &StartupPersistence,
    workspace_persistence: &BerylWorkspacePersistence,
) -> Result<WorkspacePickerOpenedWorkspace, String> {
    create_workspace_for_target(
        startup_persistence,
        workspace_persistence,
        None::<WorkspaceId>,
    )
}

fn create_workspace_for_target(
    startup_persistence: &StartupPersistence,
    workspace_persistence: &BerylWorkspacePersistence,
    execution_target: impl Into<Option<WorkspaceId>>,
) -> Result<WorkspacePickerOpenedWorkspace, String> {
    let workspace =
        crate::create_fresh_untitled_workspace(startup_persistence, workspace_persistence)
            .map_err(|error| error.to_string())?;
    let mut workspace_state = workspace_persistence
        .load_workspace_state(workspace.id())
        .map_err(|error| error.to_string())?;
    if let Some(execution_target) = execution_target.into() {
        if apply_primary_execution_target_selection(&mut workspace_state, &execution_target)
            .map_err(|error| error.to_string())?
        {
            workspace_persistence
                .save_workspace_state(workspace.id(), &workspace_state)
                .map_err(|error| error.to_string())?;
        }
    }
    let workspace_ui_state = workspace_persistence
        .load_workspace_ui_state(workspace.id())
        .map_err(|error| error.to_string())?;
    let known_workspaces =
        crate::startup_state::resolve_known_workspaces(startup_persistence, workspace_persistence)
            .map_err(|error| error.to_string())?;
    let mut workspace_picker_member_paths =
        workspace_picker_member_paths_from_states(&known_workspaces, |workspace_id| {
            match workspace_persistence.load_workspace_state(workspace_id) {
                Ok(state) => Some(state),
                Err(error) => {
                    warn!(
                        workspace_id = workspace_id.as_str(),
                        error = %error,
                        "could not load inactive workspace members for picker row"
                    );
                    None
                }
            }
        });
    workspace_picker_member_paths.insert(
        workspace.id().clone(),
        explicit_member_path_strings(&workspace_state),
    );

    Ok(WorkspacePickerOpenedWorkspace {
        workspace,
        known_workspaces,
        workspace_picker_member_paths,
        workspace_state,
        workspace_ui_state,
    })
}

fn switch_workspace_for_picker(
    startup_persistence: &StartupPersistence,
    workspace_persistence: &BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
) -> Result<WorkspacePickerOpenedWorkspace, String> {
    let workspace = workspace_persistence
        .load_workspace_manifest(&workspace_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            format!(
                "semantic workspace '{}' disappeared before Beryl could open it",
                workspace_id.as_str()
            )
        })?;
    let workspace_state = workspace_persistence
        .load_workspace_state(&workspace_id)
        .map_err(|error| error.to_string())?;
    let workspace_ui_state = workspace_persistence
        .load_workspace_ui_state(&workspace_id)
        .map_err(|error| error.to_string())?;
    let mut metadata = startup_persistence
        .load()
        .map_err(|error| error.to_string())?;
    metadata.remember_workspace(workspace_id);
    startup_persistence
        .save(&metadata)
        .map_err(|error| error.to_string())?;
    let mut known_workspaces = workspace_persistence
        .list_workspace_manifests()
        .map_err(|error| error.to_string())?;
    crate::startup_state::sort_known_workspaces_for_picker(&mut known_workspaces, &metadata);
    let mut workspace_picker_member_paths =
        workspace_picker_member_paths_from_states(&known_workspaces, |workspace_id| {
            match workspace_persistence.load_workspace_state(workspace_id) {
                Ok(state) => Some(state),
                Err(error) => {
                    warn!(
                        workspace_id = workspace_id.as_str(),
                        error = %error,
                        "could not load inactive workspace members for picker row"
                    );
                    None
                }
            }
        });
    workspace_picker_member_paths.insert(
        workspace.id().clone(),
        explicit_member_path_strings(&workspace_state),
    );

    Ok(WorkspacePickerOpenedWorkspace {
        workspace,
        known_workspaces,
        workspace_picker_member_paths,
        workspace_state,
        workspace_ui_state,
    })
}

fn delete_workspace_for_picker(
    startup_persistence: &StartupPersistence,
    workspace_persistence: &BerylWorkspacePersistence,
    workspace_id: &BerylWorkspaceId,
    active_workspace_id: &BerylWorkspaceId,
) -> Result<WorkspacePickerDeletionOutcome, String> {
    let resolution = crate::delete_workspace_and_resolve_active_replacement(
        startup_persistence,
        workspace_persistence,
        workspace_id,
        active_workspace_id,
    )
    .map_err(|error| error.to_string())?;
    let replacement_workspace = resolution.replacement_workspace().cloned();
    let replacement_workspace_state =
        if let Some(replacement_workspace) = replacement_workspace.as_ref() {
            Some(
                workspace_persistence
                    .load_workspace_state(replacement_workspace.id())
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
    let replacement_workspace_ui_state =
        if let Some(replacement_workspace) = replacement_workspace.as_ref() {
            Some(
                workspace_persistence
                    .load_workspace_ui_state(replacement_workspace.id())
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };

    let known_workspaces = resolution.known_workspaces().to_vec();
    let mut workspace_picker_member_paths =
        workspace_picker_member_paths_from_states(&known_workspaces, |workspace_id| {
            match workspace_persistence.load_workspace_state(workspace_id) {
                Ok(state) => Some(state),
                Err(error) => {
                    warn!(
                        workspace_id = workspace_id.as_str(),
                        error = %error,
                        "could not load inactive workspace members for picker row"
                    );
                    None
                }
            }
        });
    if let (Some(replacement_workspace), Some(replacement_workspace_state)) = (
        replacement_workspace.as_ref(),
        replacement_workspace_state.as_ref(),
    ) {
        workspace_picker_member_paths.insert(
            replacement_workspace.id().clone(),
            explicit_member_path_strings(replacement_workspace_state),
        );
    }

    Ok(WorkspacePickerDeletionOutcome {
        deleted: resolution.deleted(),
        replacement_workspace,
        replacement_workspace_state,
        replacement_workspace_ui_state,
        known_workspaces,
        workspace_picker_member_paths,
    })
}

use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use beryl_backend::{ManagedBackendClientConnector, ThreadListOptions};
use beryl_model::{
    conversation::{RegisteredConversationThread, WorkspaceConversationState},
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use tracing::warn;

use crate::member_thread_inventory::MemberThreadInventoryEvent;
use crate::member_thread_inventory::{
    MemberThreadInventoryBackendThread, MemberThreadInventoryGroup, MemberThreadInventoryMemberKey,
    MemberThreadInventoryMemberKind, MemberThreadInventoryRefreshToken,
    MemberThreadInventorySnapshot, build_member_thread_inventory_snapshot_for_backend_threads,
    prepare_backend_threads_for_member_thread_inventory,
    retain_scoped_backend_threads_for_inventory_members, thread_fork_parent_metadata_read_error,
    truncate_scoped_backend_threads_for_member_thread_inventory,
};

use super::{ShellView, SurfaceNotice, workspace_members};

pub(super) enum MemberThreadInventoryUpdate {
    Finished {
        workspace_id: BerylWorkspaceId,
        token: MemberThreadInventoryRefreshToken,
        result: MemberThreadInventoryResult,
    },
}

pub(super) enum MemberThreadInventoryResult {
    Refreshed {
        snapshot: MemberThreadInventorySnapshot,
        registered_threads: Vec<RegisteredConversationThread>,
    },
    Failed {
        message: String,
    },
}

pub(super) fn spawn_member_thread_inventory_worker(
    connectors: Vec<(WorkspaceId, ManagedBackendClientConnector)>,
    workspace_id: BerylWorkspaceId,
    token: MemberThreadInventoryRefreshToken,
    workspace_state: WorkspaceConversationState,
    timeout: Duration,
) -> Receiver<MemberThreadInventoryUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result =
            run_member_thread_inventory_worker(connectors, &workspace_id, workspace_state, timeout);
        let _ = sender.send(MemberThreadInventoryUpdate::Finished {
            workspace_id,
            token,
            result,
        });
    });
    receiver
}

fn run_member_thread_inventory_worker(
    connectors: Vec<(WorkspaceId, ManagedBackendClientConnector)>,
    workspace_id: &BerylWorkspaceId,
    workspace_state: WorkspaceConversationState,
    timeout: Duration,
) -> MemberThreadInventoryResult {
    let members = match resolved_inventory_members(&workspace_state) {
        Ok(members) => members,
        Err(message) => {
            return MemberThreadInventoryResult::Failed { message };
        }
    };

    let mut backend_threads = Vec::new();
    for (execution_target, connector) in connectors {
        let mut session = match connector.connect_client(timeout) {
            Ok(session) => session,
            Err(error) => {
                return MemberThreadInventoryResult::Failed {
                    message: format!("Beryl could not connect to the managed backend: {error}"),
                };
            }
        };
        let runtime = execution_target.runtime_mode().clone();
        let runtime_members = members
            .iter()
            .filter(|member| member.runtime() == &runtime)
            .cloned()
            .collect::<Vec<_>>();
        if runtime_members.is_empty() {
            continue;
        }

        let cwd_filters = runtime_members
            .iter()
            .filter_map(|member| member.canonical_path().map(std::path::Path::to_path_buf))
            .collect::<Vec<_>>();
        if cwd_filters.is_empty() {
            continue;
        }

        let mut runtime_threads = match session
            .list_threads_with_options(ThreadListOptions::page(100).with_cwds(cwd_filters), timeout)
        {
            Ok(threads) => threads,
            Err(error) => {
                return MemberThreadInventoryResult::Failed {
                    message: format!(
                        "Beryl could not refresh the workspace thread inventory: {error}"
                    ),
                };
            }
        };

        if let Err(message) = prepare_backend_threads_for_member_thread_inventory(
            &mut runtime_threads,
            &runtime_members,
            |thread_id| {
                session
                    .read_thread_metadata(thread_id, timeout)
                    .map_err(|error| thread_fork_parent_metadata_read_error(thread_id, error))
            },
        ) {
            return MemberThreadInventoryResult::Failed { message };
        }

        backend_threads.extend(
            runtime_threads
                .into_iter()
                .map(|summary| MemberThreadInventoryBackendThread::new(runtime.clone(), summary)),
        );
    }

    retain_scoped_backend_threads_for_inventory_members(&mut backend_threads, &members);
    truncate_scoped_backend_threads_for_member_thread_inventory(&mut backend_threads);

    let snapshot = build_member_thread_inventory_snapshot_for_backend_threads(
        workspace_id.clone(),
        &workspace_state,
        members,
        backend_threads,
        current_unix_millis(),
    );
    let registered_threads = snapshot
        .groups()
        .iter()
        .flat_map(|group| group.threads().iter())
        .map(|thread| thread.to_registered_thread())
        .collect();

    MemberThreadInventoryResult::Refreshed {
        snapshot,
        registered_threads,
    }
}

fn resolved_inventory_members(
    workspace_state: &WorkspaceConversationState,
) -> Result<Vec<MemberThreadInventoryGroup>, String> {
    let Some(runtime) = workspace_state.selected_runtime().cloned() else {
        return Ok(Vec::new());
    };

    if !workspace_state.has_available_explicit_members() {
        let canonical_path =
            workspace_members::resolve_runtime_home_directory(&runtime).map_err(|error| {
                format!("Beryl could not resolve the implicit home member for inventory: {error}")
            })?;
        return Ok(vec![MemberThreadInventoryGroup::new(
            MemberThreadInventoryMemberKey::ImplicitHome,
            MemberThreadInventoryMemberKind::ImplicitHome,
            "Implicit home",
            runtime.clone(),
            Some(canonical_path),
            Vec::new(),
        )]);
    }

    Ok(workspace_state
        .available_explicit_members()
        .map(|member| {
            MemberThreadInventoryGroup::new(
                MemberThreadInventoryMemberKey::Explicit(member.id().clone()),
                MemberThreadInventoryMemberKind::Explicit,
                member.canonical_path().display().to_string(),
                member.runtime_mode().clone(),
                Some(member.canonical_path().to_path_buf()),
                Vec::new(),
            )
        })
        .collect())
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

impl ShellView {
    pub(super) fn poll_member_thread_inventory_updates(&mut self) -> bool {
        let Some(receiver) = self.member_thread_inventory_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(MemberThreadInventoryUpdate::Finished {
                workspace_id,
                token,
                result,
            }) => {
                self.member_thread_inventory_receiver = None;
                self.finish_member_thread_inventory_refresh(&workspace_id, token, result);
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.member_thread_inventory_receiver = None;
                if let Some(surface) = self.conversation_surface_mut() {
                    let token = surface.member_thread_inventory().refresh_token();
                    surface
                        .member_thread_inventory_mut()
                        .fail_refresh_for_token(
                            token,
                            "Beryl lost the background thread inventory refresh task.",
                        );
                }
                true
            }
        }
    }

    pub(super) fn begin_member_thread_inventory_refresh_if_needed(&mut self) -> bool {
        if self.member_thread_inventory_receiver.is_some()
            || self.workspace_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
            || self.workspace_picker_action_receiver.is_some()
            || self.workspace_title_receiver.is_some()
        {
            return false;
        }
        if self.conversation_surface().is_some_and(|surface| {
            surface.graph_overlay().visible() || surface.pending_thread_activation_label().is_some()
        }) {
            return false;
        }

        let Some((workspace_id, workspace_state)) = self.loaded_workspace().and_then(|loaded| {
            loaded.selected_runtime().map(|_| {
                (
                    loaded.workspace.id().clone(),
                    loaded.workspace_state.clone(),
                )
            })
        }) else {
            return false;
        };
        if !self
            .conversation_surface()
            .is_some_and(|surface| surface.member_thread_inventory().needs_refresh())
        {
            return false;
        }

        let connectors = self.backend_client_connectors();
        if connectors.is_empty() {
            return false;
        }
        let Some(token) = self
            .conversation_surface_mut()
            .map(|surface| surface.member_thread_inventory_mut().begin_refresh())
        else {
            return false;
        };
        self.member_thread_inventory_receiver = Some(spawn_member_thread_inventory_worker(
            connectors,
            workspace_id,
            token,
            workspace_state,
            self.bootstrap.probe_timeout(),
        ));
        true
    }

    fn finish_member_thread_inventory_refresh(
        &mut self,
        workspace_id: &BerylWorkspaceId,
        token: MemberThreadInventoryRefreshToken,
        result: MemberThreadInventoryResult,
    ) {
        match result {
            MemberThreadInventoryResult::Refreshed {
                snapshot,
                registered_threads,
            } => {
                if !self
                    .loaded_workspace()
                    .is_some_and(|loaded| loaded.workspace.id() == workspace_id)
                {
                    return;
                }
                if !self.conversation_surface().is_some_and(|surface| {
                    surface.member_thread_inventory().refresh_token() == token
                }) {
                    return;
                }

                let registered_threads = registered_threads
                    .into_iter()
                    .map(|mut thread| {
                        if self.thread_ignores_backend_name_for_automatic_title(
                            thread.thread_id().as_str(),
                            thread.backend_name(),
                        ) {
                            thread.set_backend_name(None);
                        }
                        thread
                    })
                    .collect::<Vec<_>>();
                let mut touched_manifest = false;
                let Some(workspace_state) = self.loaded_workspace_mut().map(|loaded| {
                    for thread in registered_threads {
                        touched_manifest |= loaded.workspace_state.remember_thread(thread);
                    }
                    loaded.workspace_state.clone()
                }) else {
                    return;
                };
                if touched_manifest {
                    self.persist_current_workspace_state(true);
                }
                if let Some(surface) = self.conversation_surface_mut() {
                    if surface
                        .member_thread_inventory_mut()
                        .finish_refresh_for_token(token, snapshot, &workspace_state)
                    {
                        surface.reconcile_thread_selector_state();
                    }
                }
            }
            MemberThreadInventoryResult::Failed { message } => {
                warn!(error = %message, "member-thread inventory refresh failed");
                if let Some(surface) = self.conversation_surface_mut() {
                    if surface
                        .member_thread_inventory_mut()
                        .fail_refresh_for_token(token, message.clone())
                    {
                        surface.set_notice(SurfaceNotice::new(
                            "Thread inventory refresh failed",
                            message,
                        ));
                    }
                }
                self.block_if_backend_process_dead(
                    "Managed backend disconnected during thread inventory refresh",
                    "The backend process exited while Beryl was refreshing the workspace thread inventory.",
                    "Beryl could not refresh the workspace thread inventory because the managed backend process is no longer alive.",
                );
            }
        }
    }

    pub(super) fn reset_member_thread_inventory_for_workspace_state(&mut self) {
        self.apply_member_thread_inventory_event(MemberThreadInventoryEvent::MemberSetChanged);
    }

    pub(super) fn mark_member_thread_inventory_refresh_needed(&mut self) {
        self.apply_member_thread_inventory_event(
            MemberThreadInventoryEvent::InventoryContentsChanged,
        );
    }

    pub(super) fn apply_member_thread_inventory_event(
        &mut self,
        event: MemberThreadInventoryEvent,
    ) {
        let Some((workspace_id, workspace_state)) = self.loaded_workspace().map(|loaded| {
            (
                loaded.workspace.id().clone(),
                loaded.workspace_state.clone(),
            )
        }) else {
            return;
        };
        if matches!(
            event,
            MemberThreadInventoryEvent::MemberSetChanged
                | MemberThreadInventoryEvent::BackendTargetOpening
        ) {
            self.member_thread_inventory_receiver = None;
        }
        if let Some(surface) = self.conversation_surface_mut() {
            surface.member_thread_inventory_mut().apply_event(
                event,
                workspace_id,
                &workspace_state,
            );
            surface.reconcile_thread_selector_state();
        }
    }
}

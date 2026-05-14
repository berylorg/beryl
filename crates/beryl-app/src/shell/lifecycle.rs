use std::time::Instant;

use beryl_backend::{HardStopCapabilities, ThreadSummary};
use beryl_model::conversation::RegisteredConversationThread;
use beryl_model::workspace::WorkspaceId;

use super::turn_worker::{ThreadActivationOutcome, TurnWorkerOutcome};
use super::{
    BlockedState, ConversationSurfaceState, FailureSummary, LoadedWorkspaceState,
    OpenWorkspaceFailure, OpenedWorkspace, RetryTarget, ShellState, ShellView, SurfaceNotice,
    ThreadHistoryPageOutcome, TurnCompletionSoundCandidate, WorkspaceSurfaceSeed,
};
use crate::backend_failure::{
    json_rpc_error_detail, non_empty_user_text, source_chain_detail, truncate_user_detail,
};
use crate::memory_diagnostics::MemoryMilestone;
use tracing::debug;

impl ShellView {
    pub(super) fn finish_workspace_open(
        &mut self,
        result: Result<OpenedWorkspace, OpenWorkspaceFailure>,
    ) {
        let (
            attempt,
            mut loaded_workspace,
            preserved_surface,
            target,
            intent,
            workspace_label,
            previous_failure,
        ) = match &self.state {
            ShellState::Opening(opening) => (
                opening.attempt,
                opening.loaded_workspace.clone(),
                opening.preserved_surface.clone(),
                opening.target.clone(),
                opening.intent,
                opening.workspace_label.clone(),
                opening.previous_failure.clone(),
            ),
            _ => (
                0,
                self.bootstrap_workspace_state(&beryl_model::workspace::WorkspaceId::host_windows(
                    "",
                )),
                None,
                RetryTarget::HostPath(String::new()),
                super::WorkspaceOpenIntent::None,
                String::new(),
                None,
            ),
        };

        match result {
            Ok(opened) => {
                let workspace_backend_state_changed = if intent
                    == super::WorkspaceOpenIntent::UseAsPrimaryMember
                {
                    match super::apply_primary_execution_target_selection(
                        &mut loaded_workspace.workspace_state,
                        &opened.execution_target,
                    ) {
                        Ok(changed) => changed,
                        Err(error) => {
                            tracing::warn!(
                                workspace_id = loaded_workspace.workspace.id().as_str(),
                                target = %opened.execution_target.display_label(),
                                error = %error,
                                "failed to persist primary workspace member selection after a successful backend open"
                            );
                            false
                        }
                    }
                } else {
                    false
                };
                let process_id = opened.server.process_id();
                loaded_workspace.record_backend_available(
                    opened.execution_target.clone(),
                    attempt,
                    process_id,
                );
                let workspace_id_for_log = loaded_workspace.workspace.id().as_str().to_string();
                let active_thread_id = opened.selected_thread_id.clone().or_else(|| {
                    opened
                        .selected_thread_history
                        .as_ref()
                        .map(|thread| thread.summary().id)
                });
                let known_threads = opened.known_threads.clone();
                let inventory_workspace_id = loaded_workspace.workspace.id().clone();
                let inventory_workspace_state = loaded_workspace.workspace_state.clone();
                let mut surface = match preserved_surface {
                    Some(mut surface) => {
                        surface.refresh_after_backend_reopen(
                            &inventory_workspace_state,
                            known_threads.clone(),
                            opened.hard_stop_capabilities.clone(),
                            opened.selected_thread_history,
                            opened.selected_thread_history_window,
                            opened.selected_thread_image_resolver,
                            active_thread_id.clone(),
                            opened.selected_thread_session_metadata,
                            opened.surface_notice,
                            opened.graph,
                            opened.graph_revision,
                            opened.graph_warning,
                        );
                        surface
                    }
                    None => ConversationSurfaceState::seeded(
                        inventory_workspace_id.clone(),
                        &inventory_workspace_state,
                        &loaded_workspace.workspace_ui_state,
                        known_threads.clone(),
                        opened.hard_stop_capabilities.clone(),
                        opened.selected_thread_history,
                        opened.selected_thread_history_window,
                        opened.selected_thread_image_resolver,
                        active_thread_id.clone(),
                        opened.selected_thread_session_metadata,
                        opened.surface_notice,
                        opened.graph,
                        opened.graph_revision,
                        opened.graph_warning,
                    ),
                };
                if intent == super::WorkspaceOpenIntent::ThreadSelectorActivation
                    && active_thread_id.is_some()
                {
                    surface.close_thread_selector();
                }
                self.status_model_cache.finish_loaded_for_target(
                    opened.execution_target.clone(),
                    opened.report.model_list().to_vec(),
                    opened.report.config_defaults().clone(),
                );
                surface.set_effective_new_thread_defaults(
                    self.status_model_cache.effective_default_turn_defaults(),
                );
                let restored_implicit_home_threads = loaded_workspace
                    .set_resolved_implicit_home_path_from_target(&opened.execution_target);
                if let Some(replaced_server) = self
                    .backend_servers
                    .insert(opened.execution_target.clone(), opened.server)
                {
                    super::spawn_managed_backend_shutdown(
                        replaced_server,
                        "replacing managed backend for execution target",
                    );
                }
                self.state = ShellState::Ready(super::ReadyState {
                    attempt,
                    loaded_workspace,
                    execution_target: opened.execution_target.clone(),
                    process_id,
                    report: opened.report,
                    cleared_failure: previous_failure,
                    surface,
                });
                if restored_implicit_home_threads {
                    self.persist_current_workspace_state(true);
                }
                MemoryMilestone::new("workspace_open_ui_applied")
                    .workspace_id(workspace_id_for_log)
                    .backend_pid(process_id)
                    .retained_state_if_enabled(|| self.retained_state_snapshot())
                    .log();
                self.update_workspace_state_for_opened_target(
                    &opened.execution_target,
                    &known_threads,
                    active_thread_id.as_deref(),
                    workspace_backend_state_changed,
                );
                self.begin_account_rate_limits_read();
                self.repair_selected_thread_title_if_needed(opened.execution_target);
            }
            Err(error) => {
                let error = normalize_workspace_failure(error);
                let stage_label = failure_stage_label(error.stage);
                MemoryMilestone::new("workspace_open_failed")
                    .workspace_id(loaded_workspace.workspace.id().as_str())
                    .note(stage_label.as_str())
                    .log();
                tracing::warn!(
                    workspace = %workspace_label,
                    stage = %stage_label,
                    title = error.title,
                    summary = %error.summary,
                    detail = %error.detail,
                    "workspace open failed"
                );
                if let Some(backend_unavailable) = error.backend_unavailable.as_ref() {
                    let availability = loaded_workspace.record_backend_unavailable(
                        backend_unavailable.target.clone(),
                        attempt,
                        backend_unavailable.unavailable.clone(),
                    );
                    let surface = preserved_surface.unwrap_or_else(|| {
                        seed_backend_unavailable_surface(
                            &loaded_workspace,
                            &backend_unavailable.target,
                            backend_unavailable.surface_seed.clone(),
                        )
                    });
                    self.cancel_thread_title_workers();
                    self.state = ShellState::BackendUnavailable(super::BackendUnavailableState {
                        attempt,
                        loaded_workspace,
                        execution_target: backend_unavailable.target.clone(),
                        availability,
                        surface,
                    });
                    return;
                }
                let disconnect = preserved_surface.is_some();
                self.cancel_thread_title_workers();
                self.state = ShellState::Blocked(BlockedState {
                    attempt,
                    loaded_workspace: Some(loaded_workspace),
                    target,
                    intent,
                    workspace_label,
                    stage: error.stage,
                    title: error.title,
                    summary: error.summary,
                    detail: error.detail,
                    next_steps: error.next_steps,
                    disconnect,
                    surface: preserved_surface,
                });
            }
        }
    }

    pub(super) fn handle_disconnect(&mut self) {
        let (attempt, loaded_workspace, execution_target, surface) = match &self.state {
            ShellState::Ready(ready) => (
                ready.attempt,
                ready.loaded_workspace.clone(),
                ready.execution_target.clone(),
                ready.surface.snapshot_for_backend_reopen(),
            ),
            _ => return,
        };

        self.cancel_thread_title_workers();
        self.shutdown_active_backend_server_in_background("managed backend disconnected");
        self.state = ShellState::Blocked(BlockedState {
            attempt,
            loaded_workspace: Some(loaded_workspace),
            target: RetryTarget::Workspace(execution_target.clone()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: execution_target.display_label(),
            stage: None,
            title: "Managed backend disconnected",
            summary:
                "The backend process for the selected workspace exited after startup had already succeeded."
                    .to_string(),
            detail:
                "Beryl kept the current workspace selection, but it cannot continue without relaunching the managed backend for this workspace."
                    .to_string(),
            next_steps: vec![
                "Retry to relaunch the managed backend for this workspace.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: true,
            surface: Some(surface),
        });
    }

    pub(super) fn failure_summary(&self) -> Option<FailureSummary> {
        match &self.state {
            ShellState::Blocked(blocked) => Some(blocked.failure_summary()),
            ShellState::BackendUnavailable(unavailable) => unavailable
                .availability
                .unavailable_reason()
                .map(|reason| FailureSummary {
                    stage: reason.stage(),
                    title: reason.title(),
                    summary: reason.summary().to_string(),
                }),
            ShellState::Ready(ready) => ready.cleared_failure.clone(),
            ShellState::Opening(opening) => opening.previous_failure.clone(),
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        }
    }

    pub(super) fn finish_turn_worker(
        &mut self,
        outcome: TurnWorkerOutcome,
    ) -> Option<TurnCompletionSoundCandidate> {
        match outcome {
            TurnWorkerOutcome::Finished {
                execution_target,
                known_threads,
                active_thread_id,
            } => {
                if let ShellState::Ready(ready) = &mut self.state {
                    ready.execution_target = execution_target.clone();
                }
                if let Some(surface) = self.conversation_surface_mut() {
                    if let Some(known_threads) = known_threads.as_ref() {
                        surface.replace_known_threads(known_threads.clone(), &active_thread_id);
                    }
                    surface.mark_selected_turn_finished_idle(&active_thread_id);
                    surface.finish_running_tool_activity_for_thread_ok(&active_thread_id);
                }
                if let Some(known_threads) = known_threads.as_ref() {
                    self.update_workspace_state_for_opened_target(
                        &execution_target,
                        known_threads,
                        Some(&active_thread_id),
                        false,
                    );
                    self.mark_member_thread_inventory_refresh_needed();
                }
                None
            }
            TurnWorkerOutcome::Failed { message } => {
                let sound_candidate = self
                    .conversation_surface_mut()
                    .and_then(|surface| surface.finish_turn_failure(message.clone()));

                self.block_if_backend_process_dead(
                    "Managed backend disconnected during turn execution",
                    "The backend process for the selected workspace exited before the active turn finished.",
                    &message,
                );
                sound_candidate
            }
        }
    }

    pub(super) fn finish_thread_activation_worker(&mut self, outcome: ThreadActivationOutcome) {
        match outcome {
            ThreadActivationOutcome::Activated {
                execution_target,
                thread,
                session_metadata,
                history_window,
                image_resolver,
            } => {
                let ui_finish_started = Instant::now();
                if let ShellState::Ready(ready) = &mut self.state {
                    ready.execution_target = execution_target.clone();
                }
                let active_execution_target = match &self.state {
                    ShellState::Ready(ready) => Some(ready.execution_target.clone()),
                    ShellState::BackendUnavailable(_) => Some(execution_target.clone()),
                    ShellState::Discovering(_)
                    | ShellState::Picker(_)
                    | ShellState::Opening(_)
                    | ShellState::WorkspaceIdle(_)
                    | ShellState::WorkspaceLoaded(_)
                    | ShellState::Blocked(_) => None,
                };
                let summary = thread.summary();
                let history_turn_count = thread.turns.len();
                let history_item_count = thread
                    .turns
                    .iter()
                    .map(|turn| turn.items.len())
                    .sum::<usize>();
                let history_generated_image_count = thread
                    .turns
                    .iter()
                    .flat_map(|turn| turn.items.iter())
                    .filter(|item| matches!(item, beryl_backend::ThreadItem::ImageGeneration(_)))
                    .count();
                let activated_idle = matches!(thread.status, beryl_backend::ThreadStatus::Idle);
                if let Some(surface) = self.conversation_surface_mut() {
                    let history_apply_started = Instant::now();
                    MemoryMilestone::new("thread_activation_ui_apply_start")
                        .thread_id(summary.id.as_str())
                        .history_counts(
                            history_turn_count,
                            history_item_count,
                            history_generated_image_count,
                        )
                        .log();
                    surface.load_thread_history_window(&thread, history_window, &image_resolver);
                    debug!(
                        thread_id = summary.id.as_str(),
                        history_turn_count,
                        history_item_count,
                        history_generated_image_count,
                        history_application_ms = super::elapsed_ms(history_apply_started.elapsed()),
                        "applied activated thread history to conversation surface"
                    );
                    surface.set_thread_session_metadata(session_metadata);
                }
                MemoryMilestone::new("thread_activation_ui_applied")
                    .thread_id(summary.id.as_str())
                    .history_counts(
                        history_turn_count,
                        history_item_count,
                        history_generated_image_count,
                    )
                    .retained_state_if_enabled(|| self.retained_state_snapshot())
                    .log();
                if activated_idle {
                    MemoryMilestone::new("thread_activation_idle_settled")
                        .thread_id(summary.id.as_str())
                        .retained_state_if_enabled(|| self.retained_state_snapshot())
                        .log();
                }
                if let Some(active_execution_target) = active_execution_target {
                    self.remember_active_thread_summary(&active_execution_target, &summary, false);
                    self.hydrate_selected_thread_token_usage_snapshot();
                    self.mark_member_thread_inventory_refresh_needed();
                    self.repair_selected_thread_title_if_needed(active_execution_target);
                }
                debug!(
                    thread_id = summary.id.as_str(),
                    thread_activation_ui_finish_ms = super::elapsed_ms(ui_finish_started.elapsed()),
                    "finished activated thread UI application"
                );
            }
            ThreadActivationOutcome::RequiresRebind { detail } => {
                MemoryMilestone::new("thread_activation_requires_rebind").log();
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.clear_pending_thread_activation();
                    surface.set_notice(SurfaceNotice::new("Thread requires rebind", detail));
                    surface.member_thread_inventory_mut().mark_refresh_needed();
                }
            }
            ThreadActivationOutcome::Failed { message } => {
                MemoryMilestone::new("thread_activation_failed").log();
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.clear_pending_thread_activation();
                    surface.set_notice(SurfaceNotice::new(
                        "Thread activation failed",
                        message.clone(),
                    ));
                }

                self.block_if_backend_process_dead(
                    "Managed backend disconnected during thread activation",
                    "The backend process for the selected workspace exited before Beryl could reopen the requested thread.",
                    &message,
                );
            }
        }
    }

    pub(super) fn finish_thread_history_page_worker(&mut self, outcome: ThreadHistoryPageOutcome) {
        match outcome {
            ThreadHistoryPageOutcome::Loaded {
                thread_id,
                request,
                page,
                image_resolver,
            } => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.finish_loading_thread_history_page(
                        &thread_id,
                        request,
                        page,
                        &image_resolver,
                    );
                }
            }
            ThreadHistoryPageOutcome::Failed { thread_id, message } => {
                if let Some(surface) = self.conversation_surface_mut()
                    && surface.selected_thread_id() == Some(thread_id.as_str())
                {
                    surface.finish_loading_older_history_failure();
                    surface.set_notice(SurfaceNotice::new(
                        "Thread history load failed",
                        message.clone(),
                    ));
                }

                self.block_if_backend_process_dead(
                    "Managed backend disconnected during thread history loading",
                    "The backend process for the selected workspace exited before Beryl could load older thread history.",
                    &message,
                );
            }
        }
    }

    pub(super) fn handle_turn_worker_stopped(&mut self) -> Option<TurnCompletionSoundCandidate> {
        let message = "Beryl lost the background task that was streaming the active turn.";
        let sound_candidate = self
            .conversation_surface_mut()
            .and_then(|surface| surface.finish_turn_failure(message));

        self.block_if_backend_process_dead(
            "Turn execution stopped unexpectedly",
            message,
            "Beryl preserved the current workspace surface, but it cannot continue until the managed backend for this workspace is relaunched.",
        );
        sound_candidate
    }

    pub(super) fn handle_thread_activation_worker_stopped(&mut self) {
        let message = "Beryl lost the background task that was reopening the requested thread.";
        if let Some(surface) = self.conversation_surface_mut() {
            surface.clear_pending_thread_activation();
            surface.set_notice(SurfaceNotice::new("Thread activation failed", message));
        }
        self.block_if_backend_process_dead(
            "Thread activation stopped unexpectedly",
            message,
            "Beryl preserved the current workspace surface, but it cannot continue until the managed backend for this workspace is relaunched.",
        );
    }

    pub(super) fn block_ready_surface(&mut self, title: &'static str, summary: &str, detail: &str) {
        let (attempt, loaded_workspace, execution_target, surface) = match &self.state {
            ShellState::Ready(ready) => (
                ready.attempt,
                ready.loaded_workspace.clone(),
                ready.execution_target.clone(),
                ready.surface.snapshot_for_backend_reopen(),
            ),
            _ => return,
        };

        let summary = non_empty_user_text(
            summary,
            "The managed backend became unavailable while this workspace was open.",
        );
        let detail = non_empty_user_text(
            detail,
            "Beryl did not receive a detailed backend error for this disconnect.",
        );
        tracing::warn!(
            workspace = %execution_target.display_label(),
            title,
            summary = %summary,
            detail = %detail,
            "workspace surface blocked by backend failure"
        );

        self.state = ShellState::Blocked(BlockedState {
            attempt,
            loaded_workspace: Some(loaded_workspace),
            target: RetryTarget::Workspace(execution_target.clone()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: execution_target.display_label(),
            stage: None,
            title,
            summary,
            detail,
            next_steps: vec![
                "Retry to relaunch the managed backend for this workspace.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: true,
            surface: Some(surface),
        });
    }
}

fn seed_backend_unavailable_surface(
    loaded_workspace: &LoadedWorkspaceState,
    execution_target: &WorkspaceId,
    seed: WorkspaceSurfaceSeed,
) -> ConversationSurfaceState {
    let (known_threads, selected_thread_id) =
        unavailable_surface_thread_seed(loaded_workspace, execution_target);
    ConversationSurfaceState::seeded(
        loaded_workspace.workspace.id().clone(),
        &loaded_workspace.workspace_state,
        &loaded_workspace.workspace_ui_state,
        known_threads,
        HardStopCapabilities::default(),
        None,
        None,
        Default::default(),
        selected_thread_id,
        None,
        None,
        seed.graph,
        seed.graph_revision,
        seed.graph_warning,
    )
}

fn unavailable_surface_thread_seed(
    loaded_workspace: &LoadedWorkspaceState,
    execution_target: &WorkspaceId,
) -> (Vec<ThreadSummary>, Option<String>) {
    let Some(thread) = loaded_workspace
        .workspace_state
        .active_thread_registration()
    else {
        return (Vec::new(), None);
    };
    if thread.requires_rebind() || thread.execution_target() != execution_target {
        return (Vec::new(), None);
    }

    let summary = thread_summary_from_registration(thread);
    let selected_thread_id = summary.id.clone();
    (vec![summary], Some(selected_thread_id))
}

fn thread_summary_from_registration(thread: &RegisteredConversationThread) -> ThreadSummary {
    ThreadSummary {
        id: thread.thread_id().as_str().to_string(),
        forked_from_id: None,
        cwd: thread.execution_target().canonical_path().to_path_buf(),
        preview: thread.preview().to_string(),
        name: thread.backend_name().map(str::to_string),
        agent_nickname: None,
        path: None,
        created_at: thread.created_at_millis(),
        updated_at: thread.updated_at_millis(),
        model_provider: String::new(),
        ephemeral: false,
    }
}

pub(super) fn blocked_state_for_error(
    error: beryl_backend::ManagedBackendError,
    attempt: u32,
    stage: beryl_backend::ManagedBackendStartupStage,
) -> BlockedState {
    match error {
        beryl_backend::ManagedBackendError::Compatibility(
            beryl_backend::CompatibilityError::PlatformFamilyMismatch {
                runtime_mode,
                expected_platform_family,
                actual_platform_family,
            },
        ) => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Backend runtime family mismatch",
            summary: "The managed backend identified itself as the wrong runtime family for the selected workspace.".to_string(),
            detail: format!(
                "Runtime mode {runtime_mode} requires platform family {expected_platform_family}, but the backend reported {actual_platform_family}."
            ),
            next_steps: vec![
                "Launch the matching runtime mode for this workspace.".to_string(),
                "If you intended WSL-Linux, re-check the distro and workspace path.".to_string(),
                "Retry the startup check after correcting the target runtime.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::Compatibility(
            beryl_backend::CompatibilityError::PlatformOsMismatch {
                runtime_mode,
                expected_platform_os,
                actual_platform_os,
            },
        ) => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Backend operating system mismatch",
            summary: "The managed backend started, but it reported the wrong operating system for the selected workspace.".to_string(),
            detail: format!(
                "Runtime mode {runtime_mode} requires OS {expected_platform_os}, but the backend reported {actual_platform_os}."
            ),
            next_steps: vec![
                "Launch the matching runtime mode for this workspace.".to_string(),
                "If you intended WSL-Linux, re-check the distro and workspace path.".to_string(),
                "Retry the startup check after correcting the target runtime.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::RequestFailed { method, error } => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Required backend method failed",
            summary: format!(
                "The backend reached startup but returned an error while Beryl was validating the required method {method}."
            ),
            detail: json_rpc_error_detail(&error),
            next_steps: vec![
                "Retry if the backend error was transient.".to_string(),
                "Verify that codex app-server works in this runtime mode outside Beryl.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::RequestTimeout { method, timeout } => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Backend request timed out",
            summary: format!(
                "The backend did not finish {method} before the startup timeout, so Beryl could not verify that this workspace is usable."
            ),
            detail: format!("{method} did not answer within {timeout:?}."),
            next_steps: vec![
                "Retry if startup was temporarily slow.".to_string(),
                "Check whether codex app-server can complete startup in this runtime mode.".to_string(),
                "Increase the probe timeout only if the backend is consistently slow but otherwise healthy.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::Spawn { program, source } => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Managed backend could not start",
            summary: "Beryl could not launch the managed backend process, so startup never reached the initialize handshake.".to_string(),
            detail: source_chain_detail(
                format!("failed to spawn backend process {program}"),
                &source,
            ),
            next_steps: vec![
                "Verify that the codex executable is on PATH for this runtime mode.".to_string(),
                "If you selected WSL-Linux, verify that the distro can run codex.".to_string(),
                "Retry after correcting the launch environment.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::MissingPipe { stream_name } => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Managed backend transport is unavailable",
            summary: "Beryl launched the managed backend process, but one of the redirected transport streams was unavailable.".to_string(),
            detail: format!("The backend process did not expose redirected {stream_name}."),
            next_steps: vec![
                "Retry if process startup was interrupted.".to_string(),
                "Check whether codex app-server can start outside Beryl.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::WriteRequest { method, source } => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Backend request could not be sent",
            summary: format!(
                "Beryl launched the managed backend but could not send the required startup request {method}."
            ),
            detail: source_chain_detail(
                format!("failed to write {method} request to backend transport"),
                &source,
            ),
            next_steps: vec![
                "Retry if the backend process exited transiently.".to_string(),
                "Check whether codex app-server can accept requests outside Beryl.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::ReadTransport { source } => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Backend transport read failed",
            summary: format!(
                "Beryl launched the managed backend but could not read a usable response while {}.",
                stage.display_label().to_lowercase()
            ),
            detail: source_chain_detail("backend transport read failed", &source),
            next_steps: vec![
                "Retry if the backend process exited transiently.".to_string(),
                "Enable debug logging if backend stderr diagnostics are needed.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::InvalidJsonLine { line, source } => {
            let mut detail =
                source_chain_detail("backend transport message was not valid JSON", &source);
            detail.push_str(" Message: ");
            detail.push_str(&truncate_user_detail(&line));
            BlockedState {
                attempt,
                loaded_workspace: None,
                target: RetryTarget::HostPath(String::new()),
                intent: super::WorkspaceOpenIntent::None,
                workspace_label: String::new(),
                stage: Some(stage),
                title: "Backend response was not JSON",
                summary: format!(
                    "The managed backend wrote non-protocol output while Beryl was waiting for a startup response during {}.",
                    stage.display_label().to_lowercase()
                ),
                detail: truncate_user_detail(&detail),
                next_steps: vec![
                    "Enable debug logging if backend stderr diagnostics are needed.".to_string(),
                    "Verify that codex app-server is the executable being launched.".to_string(),
                    "Retry after correcting the backend launch environment.".to_string(),
                ],
                disconnect: false,
                surface: None,
            }
        }
        beryl_backend::ManagedBackendError::DeserializeResponse { method, source } => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Backend response was malformed",
            summary: format!(
                "The managed backend answered {method}, but Beryl could not decode the response shape it returned."
            ),
            detail: source_chain_detail(
                format!("failed to deserialize {method} response"),
                &source,
            ),
            next_steps: vec![
                "Check that Beryl is running against a compatible codex app-server version.".to_string(),
                "Retry after updating or selecting a compatible backend.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::SerializeRequest { method, source } => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Backend request could not be encoded",
            summary: format!(
                "Beryl could not encode the required startup request {method} before sending it to the backend."
            ),
            detail: source_chain_detail(
                format!("failed to serialize {method} request payload"),
                &source,
            ),
            next_steps: vec![
                "Retry after restarting Beryl.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::ProcessExited { method } => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Managed backend exited during startup",
            summary: format!(
                "The managed backend process exited while Beryl was waiting for {method} during {}.",
                stage.display_label().to_lowercase()
            ),
            detail: "The backend transport closed before Beryl received a usable response. Managed backend stderr is available through debug logging as `backend stderr` diagnostics.".to_string(),
            next_steps: vec![
                "Enable debug logging if backend stderr diagnostics are needed.".to_string(),
                "Verify that codex app-server can start in this workspace outside Beryl.".to_string(),
                "Retry after correcting the backend launch environment.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::UnexpectedMessageShape => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Backend response shape is unsupported",
            summary: "The managed backend returned JSON that was neither a JSON-RPC response nor a notification.".to_string(),
            detail: "Beryl could not match the backend output to the JSON-RPC protocol shape expected during startup.".to_string(),
            next_steps: vec![
                "Check that Beryl is launching codex app-server, not another command.".to_string(),
                "Retry after updating or selecting a compatible backend.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
        beryl_backend::ManagedBackendError::DeserializeNotification { method, source } => {
            BlockedState {
                attempt,
                loaded_workspace: None,
                target: RetryTarget::HostPath(String::new()),
                intent: super::WorkspaceOpenIntent::None,
                workspace_label: String::new(),
                stage: Some(stage),
                title: "Backend notification was malformed",
                summary: format!(
                    "The managed backend sent {method}, but Beryl could not decode the notification payload during startup."
                ),
                detail: source_chain_detail(
                    format!("failed to deserialize {method} notification"),
                    &source,
                ),
                next_steps: vec![
                    "Check that Beryl is running against a compatible codex app-server version.".to_string(),
                    "Retry after updating or selecting a compatible backend.".to_string(),
                    "Close Beryl if you want to stop here.".to_string(),
                ],
                disconnect: false,
                surface: None,
            }
        }
        other => BlockedState {
            attempt,
            loaded_workspace: None,
            target: RetryTarget::HostPath(String::new()),
            intent: super::WorkspaceOpenIntent::None,
            workspace_label: String::new(),
            stage: Some(stage),
            title: "Managed backend startup failed",
            summary: format!(
                "Beryl lost a usable backend response while {}.",
                stage.display_label().to_lowercase()
            ),
            detail: source_chain_detail(other.to_string(), &other),
            next_steps: vec![
                "Retry if the failure may have been transient.".to_string(),
                "Check whether codex app-server can start and answer requests in this runtime mode.".to_string(),
                "Close Beryl if you want to stop here.".to_string(),
            ],
            disconnect: false,
            surface: None,
        },
    }
}

fn normalize_workspace_failure(mut failure: OpenWorkspaceFailure) -> OpenWorkspaceFailure {
    failure.summary = non_empty_user_text(
        &failure.summary,
        "Beryl stopped opening the selected workspace before it reported a specific summary.",
    );
    let detail_fallback = match failure.stage {
        Some(stage) => format!(
            "Beryl did not receive detailed error text while {}.",
            stage.display_label().to_lowercase()
        ),
        None => {
            "Beryl did not receive detailed error text while resolving the workspace.".to_string()
        }
    };
    failure.detail = non_empty_user_text(&failure.detail, &detail_fallback);
    if failure.next_steps.is_empty() {
        failure.next_steps = vec![
            "Retry the same workspace selection.".to_string(),
            "Check the console for the workspace open failure warning.".to_string(),
            "Close Beryl if you want to stop here.".to_string(),
        ];
    }
    failure
}

fn failure_stage_label(stage: Option<beryl_backend::ManagedBackendStartupStage>) -> String {
    stage
        .map(|stage| stage.display_label().to_string())
        .unwrap_or_else(|| "workspace resolution".to_string())
}

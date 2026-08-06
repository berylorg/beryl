use super::*;

impl ShellView {
    pub(super) fn workspace_picker_action_in_flight(&self) -> bool {
        self.workspace_picker_action_receiver.is_some()
            || self.phase_thread_workspace_deletion.is_some()
    }

    pub(super) fn poll_phase_thread_workspace_deletion(&mut self) -> bool {
        let Some(mut deletion) = self.phase_thread_workspace_deletion.take() else {
            return false;
        };
        match poll_phase_thread_workspace_deletion(&mut deletion, self) {
            PhaseThreadWorkspaceDeletionPoll::WaitingForPhaseThread => {
                self.phase_thread_workspace_deletion = Some(deletion);
                false
            }
            PhaseThreadWorkspaceDeletionPoll::WorkerStarted => {
                self.phase_thread_workspace_deletion = Some(deletion);
                true
            }
            PhaseThreadWorkspaceDeletionPoll::WorkerStartFailed(error) => {
                warn!(
                    workspace_id = deletion.workspace_id().as_str(),
                    error = %error,
                    "active workspace deletion could not start after phase-thread drain"
                );
                true
            }
        }
    }

    pub(super) fn complete_phase_thread_workspace_deletion(&mut self) -> bool {
        complete_phase_thread_workspace_deletion(&mut self.phase_thread_workspace_deletion)
    }

    pub(super) fn finish_workspace_picker_action(
        &mut self,
        update: WorkspacePickerActionUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match update {
            WorkspacePickerActionUpdate::Created(Ok(opened)) => {
                self.finish_workspace_picker_opened_workspace(opened, window, cx);
            }
            WorkspacePickerActionUpdate::Created(Err(message)) => {
                warn!(
                    error = %message,
                    "failed to create a fresh semantic workspace from the picker"
                );
            }
            WorkspacePickerActionUpdate::Switched(Ok(opened)) => {
                self.finish_workspace_picker_opened_workspace(opened, window, cx);
            }
            WorkspacePickerActionUpdate::Switched(Err(message)) => {
                warn!(
                    error = %message,
                    "failed to switch semantic workspaces from the picker"
                );
            }
            WorkspacePickerActionUpdate::Deleted {
                workspace_id,
                result: Ok(outcome),
            } => {
                self.complete_phase_thread_workspace_deletion();
                self.finish_workspace_picker_deleted_workspace(&workspace_id, outcome, window, cx);
            }
            WorkspacePickerActionUpdate::Deleted {
                workspace_id,
                result: Err(message),
            } => {
                self.complete_phase_thread_workspace_deletion();
                warn!(
                    workspace_id = workspace_id.as_str(),
                    error = %message,
                    "failed to delete Beryl workspace from the picker"
                );
            }
        }
    }

    pub(super) fn lifecycle_phase_thread_transition_active(&self) -> bool {
        self.phase_thread_transition.blocks_controls()
            || self.phase_thread_workspace_deletion.is_some()
    }

    pub(crate) fn reject_lifecycle_phase_thread_transition_action(
        &mut self,
        title: &'static str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.lifecycle_phase_thread_transition_active() {
            return false;
        }
        self.set_phase_thread_transition_notice(title, PHASE_THREAD_TRANSITION_BUSY_MESSAGE);
        cx.notify();
        true
    }

    pub(super) fn cancel_phase_thread_preparation(&mut self) {
        let deadline = Instant::now() + self.phase_thread_preparation_retention_timeout();
        self.phase_thread_transition.cancel_active(deadline);
    }

    pub(super) fn invalidate_phase_thread_for_accepted_workspace_replacement(
        &mut self,
        replaces_active_workspace: bool,
    ) -> bool {
        let deadline = Instant::now() + self.phase_thread_preparation_retention_timeout();
        self.phase_thread_transition
            .invalidate_for_accepted_workspace_replacement(replaces_active_workspace, deadline)
    }

    pub(super) fn prospective_workspace_state_invalidates_phase_thread(
        &self,
        prospective: &WorkspaceConversationState,
    ) -> bool {
        let Some(request) = self.phase_thread_transition.active_request() else {
            return false;
        };
        let Some(loaded) = self.workspace_shell_state() else {
            return true;
        };
        if loaded.workspace.id() != request.workspace_id() {
            return true;
        }
        let implicit_home_execution_target = loaded.resolved_implicit_home_execution_target();
        !phase_thread_request_registrations_are_available(
            request,
            prospective,
            implicit_home_execution_target.as_ref(),
        )
    }

    pub(super) fn phase_thread_preparation_retention_timeout(&self) -> Duration {
        self.bootstrap
            .probe_timeout()
            .saturating_mul(6)
            .saturating_add(APP_SHUTDOWN_OPEN_WORKER_GRACE_TIMEOUT)
    }

    pub(super) fn set_phase_thread_transition_notice(
        &mut self,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                bounded_phase_thread_notice_detail(title.into()),
                bounded_phase_thread_notice_detail(detail.into()),
            ));
        }
    }

    pub(super) fn begin_lifecycle_phase_thread_preparation(
        &mut self,
        handoff: PhaseContinueNewThreadHandoff,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.lifecycle_phase_thread_transition_active() {
            self.set_phase_thread_transition_notice(
                "Lifecycle phase thread unavailable",
                PHASE_THREAD_TRANSITION_BUSY_MESSAGE,
            );
            return false;
        }
        if !self.phase_thread_transition.can_install_active() {
            self.set_phase_thread_transition_notice(
                "Lifecycle phase thread unavailable",
                "Beryl is still retaining the bounded maximum of cancelled lifecycle preparation workers so their outcomes are not lost.",
            );
            return false;
        }
        if self.turn_receiver.is_some()
            || self.shell_tool_receiver.is_some()
            || self.status_operation_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
        {
            self.set_phase_thread_transition_notice(
                "Lifecycle phase thread unavailable",
                "Beryl could not begin clean phase-thread preparation because another thread operation is active.",
            );
            return false;
        }

        let source_thread_id = ConversationThreadId::new(handoff.source_thread_id().to_string());
        let source_turn_id = ConversationTurnId::new(handoff.source_turn_id().to_string());
        let Some((workspace_id, selected_thread_id, source_registration)) =
            self.workspace_shell_state().and_then(|loaded| {
                let selected_thread_id = self
                    .conversation_surface()?
                    .selected_thread_id()
                    .map(ConversationThreadId::new)?;
                let source = loaded
                    .workspace_state
                    .thread_registration(&source_thread_id)?
                    .clone();
                Some((loaded.workspace.id().clone(), selected_thread_id, source))
            })
        else {
            self.set_phase_thread_transition_notice(
                "Lifecycle phase thread unavailable",
                "Beryl could not freeze the completed source thread registration.",
            );
            return false;
        };
        if selected_thread_id != source_thread_id {
            self.set_phase_thread_transition_notice(
                "Lifecycle phase thread skipped",
                "The completed source thread is no longer the selected thread.",
            );
            return false;
        }

        let orchestration_root_thread_id = source_registration
            .orchestration_root_thread_id()
            .cloned()
            .unwrap_or_else(|| source_thread_id.clone());
        if source_registration.orchestration_root_thread_id().is_none() {
            let recorded = self.workspace_shell_state_mut().is_some_and(|loaded| {
                loaded
                    .workspace_state
                    .record_thread_as_orchestration_root(&source_thread_id)
                    .is_ok()
            });
            if !recorded {
                self.set_phase_thread_transition_notice(
                    "Lifecycle phase thread unavailable",
                    "Beryl could not persist the source as its lifecycle orchestration root.",
                );
                return false;
            }
            self.persist_current_workspace_state(true);
        }

        let Some((source, root, available_member_binding)) =
            self.workspace_shell_state().and_then(|loaded| {
                let source = loaded
                    .workspace_state
                    .thread_registration(&source_thread_id)?
                    .clone();
                let root = loaded
                    .workspace_state
                    .thread_registration(&orchestration_root_thread_id)?
                    .clone();
                let implicit_home_execution_target =
                    loaded.resolved_implicit_home_execution_target();
                let available_member_binding = loaded
                    .workspace_state
                    .binding_for_available_execution_target(
                        source.execution_target(),
                        implicit_home_execution_target.as_ref(),
                    );
                Some((source, root, available_member_binding))
            })
        else {
            self.set_phase_thread_transition_notice(
                "Lifecycle phase thread unavailable",
                "Beryl could not resolve the persisted lifecycle orchestration root.",
            );
            return false;
        };

        let request_generation = self.phase_thread_transition.next_generation();
        let request = match PhaseThreadPreparationRequest::new_with_available_binding(
            PhaseThreadPreparationRequestParts {
                request_generation,
                workspace_id,
                source_thread_id: source_thread_id.clone(),
                source_turn_id,
                orchestration_root_thread_id,
                source_selection_thread_id: selected_thread_id,
            },
            &source,
            &root,
            available_member_binding.as_ref(),
        ) {
            Ok(request) => request,
            Err(error) => {
                self.set_phase_thread_transition_notice(
                    "Lifecycle phase thread unavailable",
                    error.to_string(),
                );
                return false;
            }
        };
        let Some(connector) =
            self.backend_client_connector_for_execution_target(request.execution_target())
        else {
            self.set_phase_thread_transition_notice(
                "Lifecycle phase thread unavailable",
                "Beryl could not open an independent backend client for the frozen execution target.",
            );
            return false;
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        fail_accepted_source_pending_input(self, source_thread_id.as_str());
        let receiver = spawn_phase_thread_preparation_worker(
            connector,
            request.clone(),
            cancellation.clone(),
            self.bootstrap.probe_timeout(),
        );
        let task = PhaseThreadPreparationTask::new(
            request,
            handoff.resume_fragment(),
            cancellation,
            receiver,
        );
        if self.phase_thread_transition.install_active(task).is_err() {
            self.set_phase_thread_transition_notice(
                "Lifecycle phase thread unavailable",
                PHASE_THREAD_TRANSITION_BUSY_MESSAGE,
            );
            return false;
        }
        self.schedule_poll_if_needed(window, cx);
        cx.notify();
        true
    }
}

impl PhaseThreadWorkspaceDeletionHost for ShellView {
    fn phase_thread_task_pending_for_workspace(&self, workspace_id: &BerylWorkspaceId) -> bool {
        self.phase_thread_transition
            .has_task_for_workspace(workspace_id)
    }

    fn take_deferred_phase_thread_outcomes_for_workspace(
        &mut self,
        workspace_id: &BerylWorkspaceId,
    ) -> Vec<DeferredPhaseThreadOutcome> {
        self.phase_thread_transition
            .take_deferred_outcomes(workspace_id)
    }

    fn publish_released_phase_thread_outcomes(
        &mut self,
        workspace_id: &BerylWorkspaceId,
        released: &ReleasedPhaseThreadWorkspaceDeletionOutcomes,
    ) {
        for child_thread_id in released.known_remaining_child_ids() {
            warn!(
                workspace_id = workspace_id.as_str(),
                child_thread_id = child_thread_id.as_str(),
                "workspace deletion released a retained lifecycle child registration"
            );
        }
        if released.refresh_inventory() {
            self.mark_member_thread_inventory_refresh_needed();
        }
        if let Some((title, detail)) = released.final_notice() {
            self.set_phase_thread_transition_notice(title, detail);
        }
    }

    fn capture_persistence_barrier_and_start_delete_worker(
        &mut self,
        workspace_id: &BerylWorkspaceId,
    ) -> Result<(), String> {
        let app_state = self.app_state_for_worker().ok_or_else(|| {
            "configured persistence state became unavailable before deletion worker start"
                .to_string()
        })?;
        let workspace_persistence_flush = self.workspace_persistence_queue.flush();
        self.workspace_picker_action_receiver = Some(spawn_delete_workspace_worker(
            app_state.startup_persistence,
            app_state.workspace_persistence,
            workspace_id.clone(),
            workspace_id.clone(),
            workspace_persistence_flush,
            self.bootstrap.probe_timeout(),
        ));
        Ok(())
    }
}

impl PhaseThreadSourceQueueHost for ShellView {
    fn fail_source_pending_queue(&mut self, source_thread_id: &str, message: &str) -> bool {
        self.conversation_surface_mut().is_some_and(|surface| {
            surface.fail_pending_turn_input_queue_for_thread(source_thread_id, message)
        })
    }
}

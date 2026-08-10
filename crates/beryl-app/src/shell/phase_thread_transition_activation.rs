use super::*;

impl ShellView {
    fn current_phase_thread_identity(
        &self,
        request: &PhaseThreadPreparationRequest,
    ) -> Option<PhaseThreadCurrentIdentity> {
        let loaded = self.workspace_shell_state()?;
        let selected_thread_id = ConversationThreadId::new(
            self.conversation_surface()?
                .selected_thread_id()?
                .to_string(),
        );
        let source = loaded
            .workspace_state
            .thread_registration(request.source_thread_id())?;
        let root_id = source.orchestration_root_thread_id()?.clone();
        let root = loaded.workspace_state.thread_registration(&root_id)?;
        if root.orchestration_root_thread_id() != Some(&root_id) {
            return None;
        }
        let member_binding = source.member_binding()?.clone();
        if root.execution_target() != source.execution_target()
            || root.member_binding() != Some(&member_binding)
        {
            return None;
        }
        let implicit_home_execution_target = loaded.resolved_implicit_home_execution_target();
        let available_member_binding = loaded
            .workspace_state
            .binding_for_available_execution_target(
                source.execution_target(),
                implicit_home_execution_target.as_ref(),
            );
        Some(PhaseThreadCurrentIdentity {
            request_generation: self.phase_thread_transition.generation(),
            workspace_id: loaded.workspace.id().clone(),
            source_thread_id: source.thread_id().clone(),
            source_turn_id: request.source_turn_id().clone(),
            orchestration_root_thread_id: root_id,
            source_selection_thread_id: selected_thread_id,
            execution_target: source.execution_target().clone(),
            canonical_cwd: source.execution_target().canonical_path().to_path_buf(),
            member_binding,
            available_member_binding,
            source_rebind_required: source.rebind_required().is_some(),
            root_rebind_required: root.rebind_required().is_some(),
        })
    }

    pub(super) fn poll_phase_thread_preparation_updates(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(completion) = self.phase_thread_transition.poll_next(Instant::now()) else {
            return false;
        };
        let task = match &completion {
            PhaseThreadPreparationPoll::Outcome { task, .. }
            | PhaseThreadPreparationPoll::Disconnected { task }
            | PhaseThreadPreparationPoll::RetentionExpired { task } => task,
        };
        let original_workspace_is_current = self
            .workspace_shell_state()
            .is_some_and(|loaded| loaded.workspace.id() == task.request().workspace_id());
        match completion {
            PhaseThreadPreparationPoll::Outcome { task, outcome } => {
                let current_identity = self.current_phase_thread_identity(task.request());
                let guard = current_identity.as_ref().map_or_else(
                    || Err(phase_thread_transition::PhaseThreadResultGuardFailure::Workspace),
                    |current| {
                        guard_phase_thread_preparation_result(
                            task.request(),
                            &outcome.request,
                            current,
                        )
                    },
                );
                let returned_request_matches = outcome.request == *task.request();
                let kind = phase_thread_completion_kind(&outcome.result);
                let decision = reduce_phase_thread_completion(
                    task.state(),
                    returned_request_matches,
                    guard.is_ok(),
                    original_workspace_is_current,
                    kind,
                );
                if let Some(activation) =
                    apply_phase_thread_completion(self, task, outcome.result, decision, guard.err())
                {
                    self.activate_registered_phase_thread_child(activation, window, cx);
                }
            }
            PhaseThreadPreparationPoll::Disconnected { task } => {
                let decision = reduce_phase_thread_disconnect(original_workspace_is_current);
                self.apply_phase_thread_indeterminate_completion(
                    task.request(),
                    decision,
                    "The preparation worker disconnected without a result after the fork may have committed. Beryl did not guess or select a thread; the original workspace inventory will be refreshed when it is current.",
                );
            }
            PhaseThreadPreparationPoll::RetentionExpired { task } => {
                let decision = reduce_phase_thread_disconnect(original_workspace_is_current);
                self.apply_phase_thread_indeterminate_completion(
                    task.request(),
                    decision,
                    "The preparation worker did not return within the bounded cancellation drain. Its fork outcome remains indeterminate; Beryl did not guess or select a thread, and the original workspace inventory will be refreshed when it is current.",
                );
            }
        }
        self.release_phase_thread_retained_backend_servers_if_unowned();
        true
    }

    fn apply_phase_thread_indeterminate_completion(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        decision: PhaseThreadCompletionDecision,
        detail: &'static str,
    ) {
        if decision.refresh_original_workspace
            && self.phase_thread_original_workspace_is_current(request)
        {
            self.mark_member_thread_inventory_refresh_needed();
        }
        self.report_phase_thread_preparation_for_original_workspace(
            request,
            "Lifecycle phase thread outcome unknown",
            detail,
            decision.refresh_original_workspace,
            None,
        );
    }

    fn prepared_phase_thread_registration_validity(
        &self,
        request: &PhaseThreadPreparationRequest,
    ) -> Result<(), String> {
        let loaded = self
            .workspace_shell_state()
            .ok_or_else(|| "the original workspace is no longer loaded".to_string())?;
        if loaded.workspace.id() != request.workspace_id() {
            return Err("the original workspace identity no longer matches".to_string());
        }
        let source = loaded
            .workspace_state
            .thread_registration(request.source_thread_id())
            .ok_or_else(|| "the lifecycle source registration is unavailable".to_string())?;
        let root = loaded
            .workspace_state
            .thread_registration(request.orchestration_root_thread_id())
            .ok_or_else(|| {
                "the lifecycle orchestration-root registration is unavailable".to_string()
            })?;
        if source.rebind_required().is_some() || root.rebind_required().is_some() {
            return Err(
                "the lifecycle source or orchestration root requires explicit member rebinding"
                    .to_string(),
            );
        }
        if source.execution_target() != request.execution_target()
            || root.execution_target() != request.execution_target()
            || source.member_binding() != Some(request.member_binding())
            || root.member_binding() != Some(request.member_binding())
            || source.orchestration_root_thread_id() != Some(request.orchestration_root_thread_id())
            || root.orchestration_root_thread_id() != Some(request.orchestration_root_thread_id())
        {
            return Err(
                "the lifecycle source or orchestration-root registration no longer matches the frozen provenance"
                    .to_string(),
            );
        }
        let implicit_home_execution_target = loaded.resolved_implicit_home_execution_target();
        let available_member_binding = loaded
            .workspace_state
            .binding_for_available_execution_target(
                request.execution_target(),
                implicit_home_execution_target.as_ref(),
            );
        if available_member_binding.as_ref() != Some(request.member_binding()) {
            return Err(
                "the frozen workspace-member binding is unavailable; explicit rebinding may be required"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn register_prepared_phase_thread_child(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        registration: &PreparedPhaseThreadRegistration,
        activate: bool,
    ) -> Result<(), String> {
        let child_thread_id = registration.child_thread_id().clone();
        let registered = RegisteredConversationThread::new(
            child_thread_id.clone(),
            request.execution_target().clone(),
            "",
            None,
            registration.created_at_millis(),
            registration.updated_at_millis(),
        )
        .with_member_binding(request.member_binding().clone())
        .with_beryl_created();

        let Some(loaded) = self.workspace_shell_state_mut() else {
            return Err("the original workspace is no longer loaded".to_string());
        };
        if loaded.workspace.id() != request.workspace_id() {
            return Err("the original workspace identity no longer matches".to_string());
        }
        let mut updated_workspace_state = loaded.workspace_state.clone();
        updated_workspace_state.remember_thread(registered);
        updated_workspace_state
            .record_thread_orchestration_root(
                &child_thread_id,
                request.orchestration_root_thread_id(),
            )
            .map_err(|error| error.to_string())?;
        if activate {
            updated_workspace_state
                .activate_thread(&child_thread_id)
                .ok_or_else(|| {
                    "the verified child registration could not be activated".to_string()
                })?;
        }
        loaded.workspace_state = updated_workspace_state;
        self.persist_current_workspace_state(true);
        Ok(())
    }

    fn report_phase_thread_preparation_for_original_workspace(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        title: &'static str,
        detail: impl Into<String>,
        refresh_inventory: bool,
        prepared_registration: Option<PreparedPhaseThreadRegistration>,
    ) {
        let detail = detail.into();
        let original_workspace_is_current =
            self.phase_thread_original_workspace_is_current(request);
        if original_workspace_is_current && prepared_registration.is_none() {
            self.set_phase_thread_transition_notice(title, detail);
        } else {
            let outcome = DeferredPhaseThreadOutcome::new(
                request.clone(),
                bounded_phase_thread_notice_detail(title),
                bounded_phase_thread_notice_detail(&detail),
                refresh_inventory,
                prepared_registration,
            );
            self.phase_thread_transition.defer_outcome(outcome);
            if original_workspace_is_current {
                self.set_phase_thread_transition_notice(title, detail);
            }
        }
    }

    fn phase_thread_original_workspace_is_current(
        &self,
        request: &PhaseThreadPreparationRequest,
    ) -> bool {
        self.workspace_shell_state()
            .is_some_and(|loaded| loaded.workspace.id() == request.workspace_id())
    }

    pub(super) fn apply_deferred_phase_thread_outcomes_for_current_workspace(&mut self) -> bool {
        let Some(workspace_id) = self
            .workspace_shell_state()
            .map(|loaded| loaded.workspace.id().clone())
        else {
            return false;
        };
        let outcomes = self
            .phase_thread_transition
            .take_deferred_outcomes(&workspace_id);
        if outcomes.is_empty() {
            return false;
        }
        let mut retained = Vec::new();
        let mut notices = Vec::new();
        let mut refresh_inventory = false;
        for outcome in outcomes {
            if let Some(registration) = outcome.prepared_registration() {
                if let Err(error) =
                    apply_deferred_prepared_registration(self, outcome.request(), registration)
                {
                    notices.push((
                        "Lifecycle phase thread awaiting rebind".to_string(),
                        bounded_phase_thread_notice_detail(format!(
                            "Prepared backend child {} remains retained without activation until its exact workspace-member binding is available. {error}",
                            registration.child_thread_id().as_str()
                        )),
                    ));
                    retained.push(outcome);
                    continue;
                }
            }
            refresh_inventory |= outcome.refresh_inventory();
            notices.push((outcome.title().to_string(), outcome.detail().to_string()));
        }
        self.phase_thread_transition
            .restore_deferred_outcomes(retained);
        if refresh_inventory {
            self.mark_member_thread_inventory_refresh_needed();
        }
        for (title, detail) in notices {
            self.set_phase_thread_transition_notice(title, detail);
        }
        true
    }

    fn activate_registered_phase_thread_child(
        &mut self,
        activation: PreparedPhaseThreadActivation,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let PreparedPhaseThreadActivation {
            request,
            child,
            session_metadata,
            resume_fragment,
        } = activation;
        let child_summary = child.summary();
        let child_id = child_summary.id.clone();

        let automatic_title_generation_allowed =
            self.workspace_shell_state().is_some_and(|loaded| {
                loaded
                    .workspace_state
                    .thread_automatic_title_generation_eligible(&ConversationThreadId::new(
                        child_id.clone(),
                    ))
            });
        let turn_options = self
            .conversation_surface()
            .map(|surface| surface.pending_turn_start_options(Some(child_id.as_str())))
            .unwrap_or_default();
        let queued = self.conversation_surface_mut().is_some_and(|surface| {
            surface.load_thread_history(&child);
            let mut display_summary = child_summary;
            display_summary.name = None;
            surface.upsert_selected_thread(display_summary);
            surface.set_thread_session_metadata(session_metadata);
            surface.queue_pending_turn_fragment(
                child_id.clone(),
                request.execution_target().clone(),
                automatic_title_generation_allowed,
                turn_options,
                resume_fragment,
            )
        });
        if !queued {
            self.mark_member_thread_inventory_refresh_needed();
            self.set_phase_thread_transition_notice(
                "Turn error",
                "Beryl could not queue the lifecycle continuation turn in the prepared child.",
            );
            return;
        }
        self.mark_member_thread_inventory_refresh_needed();
        let started = self.begin_pending_turn_input_queue_for_thread(&child_id);
        match reduce_phase_thread_continuation_start(started) {
            PhaseThreadContinuationStartDecision::Started => {}
            PhaseThreadContinuationStartDecision::LeaveSelectedIdle => {
                let message =
                    "Beryl could not start the lifecycle continuation turn in the prepared child.";
                if let Some(surface) = self.conversation_surface_mut() {
                    let _ = surface.take_pending_turn_input_queue_for_thread(&child_id);
                    if surface.finish_turn_failure(message.to_string()).is_none() {
                        surface.set_notice(local_turn_failure_notice(message));
                    }
                }
            }
        }
        self.notify_conversation_model_refresh(cx);
    }
}

impl PhaseThreadCompletionHost for ShellView {
    fn original_workspace_is_current(&self, request: &PhaseThreadPreparationRequest) -> bool {
        self.phase_thread_original_workspace_is_current(request)
    }

    fn mark_inventory_refresh(&mut self) {
        self.mark_member_thread_inventory_refresh_needed();
    }

    fn prepared_registration_validity(
        &self,
        request: &PhaseThreadPreparationRequest,
    ) -> Result<(), String> {
        self.prepared_phase_thread_registration_validity(request)
    }

    fn register_prepared_child(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        registration: &PreparedPhaseThreadRegistration,
        activate: bool,
    ) -> Result<(), String> {
        self.register_prepared_phase_thread_child(request, registration, activate)
    }

    fn report_or_defer(
        &mut self,
        request: &PhaseThreadPreparationRequest,
        title: &'static str,
        detail: String,
        refresh_inventory: bool,
        prepared_registration: Option<PreparedPhaseThreadRegistration>,
    ) {
        self.report_phase_thread_preparation_for_original_workspace(
            request,
            title,
            detail,
            refresh_inventory,
            prepared_registration,
        );
    }
}

fn phase_thread_completion_kind(
    result: &PhaseThreadPreparationResult,
) -> PhaseThreadCompletionKind {
    match result {
        PhaseThreadPreparationResult::Prepared { .. } => PhaseThreadCompletionKind::Prepared,
        PhaseThreadPreparationResult::DefinitiveForkFailure { .. } => {
            PhaseThreadCompletionKind::DefinitiveForkFailure
        }
        PhaseThreadPreparationResult::IndeterminateFork { .. } => {
            PhaseThreadCompletionKind::IndeterminateFork
        }
        PhaseThreadPreparationResult::CancelledBeforeFork => {
            PhaseThreadCompletionKind::CancelledBeforeFork
        }
        PhaseThreadPreparationResult::KnownChildFailure(failure) => match failure.cleanup {
            PhaseThreadCleanupOutcome::Accepted => PhaseThreadCompletionKind::KnownChildCleaned,
            PhaseThreadCleanupOutcome::ChildRemains { .. }
            | PhaseThreadCleanupOutcome::Indeterminate { .. } => {
                PhaseThreadCompletionKind::KnownChildRemaining
            }
        },
    }
}

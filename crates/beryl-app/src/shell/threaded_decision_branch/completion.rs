use super::*;

impl ShellView {
    pub(super) fn finish_decision_branch_start_worker(
        &mut self,
        outcome: DecisionBranchStartOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            DecisionBranchStartOutcome::Started {
                job,
                thread_summary,
                bootstrap_turn_id,
                graph_patch,
                graph_revision,
                optimistic_mutation_id,
            } => self.finish_successful_decision_branch(
                job,
                thread_summary,
                bootstrap_turn_id,
                graph_patch,
                graph_revision,
                optimistic_mutation_id,
                window,
                cx,
            ),
            DecisionBranchStartOutcome::Failed {
                job,
                message,
                graph_failure,
            } => self.finish_failed_decision_branch(job, message, graph_failure, window, cx),
        }
    }

    fn finish_successful_decision_branch(
        &mut self,
        job: QueuedDecisionBranchJob,
        summary: ThreadSummary,
        bootstrap_turn_id: Option<ConversationTurnId>,
        graph_patch: SemanticGraphPatch,
        graph_revision: WorkspaceGraphRevision,
        optimistic_mutation_id: OptimisticGraphMutationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let branch_thread_id = ConversationThreadId::new(summary.id.clone());
        let parent_thread_id = job.parent_thread_id.clone();

        let registration = {
            let Some(loaded) = self.workspace_shell_state() else {
                self.remove_decision_branch_record(&job.record_id);
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Decision branch failed",
                        "Beryl created the decision branch, but the workspace is no longer loaded.",
                    ));
                }
                return;
            };
            let workspace_id = loaded.workspace.id().clone();
            let mut candidate_workspace_state = loaded.workspace_state.clone();
            let mut candidate_threaded_decision_state = loaded.threaded_decision_state.clone();
            match register_decision_branch_thread(
                &mut candidate_workspace_state,
                &parent_thread_id,
                &job.branch_point_turn_id,
                &summary,
                bootstrap_turn_id.clone(),
            ) {
                Ok((execution_target, touched_manifest)) => {
                    let state_changed = match candidate_threaded_decision_state
                        .activate_branch_with_bootstrap_turn(
                            &job.record_id,
                            branch_thread_id.clone(),
                            bootstrap_turn_id.clone(),
                            Some(job.branch_point_turn_id.clone()),
                            job.provenance.clone(),
                        ) {
                        Ok(changed) => changed,
                        Err(error) => {
                            self.finish_failed_decision_branch(
                                job,
                                error.to_string(),
                                GraphMutationFailureUpdate::new(
                                    workspace_id,
                                    "Decision branch activation failed",
                                ),
                                window,
                                cx,
                            );
                            return;
                        }
                    };
                    let superseded = match candidate_threaded_decision_state
                        .supersede_closed_records_for_item(
                            &job.checklist_item_id,
                            job.record_id.clone(),
                            job.provenance.clone(),
                        ) {
                        Ok(changed) => changed,
                        Err(error) => {
                            self.finish_failed_decision_branch(
                                job,
                                error.to_string(),
                                GraphMutationFailureUpdate::new(
                                    workspace_id,
                                    "Decision branch superseding failed",
                                ),
                                window,
                                cx,
                            );
                            return;
                        }
                    };
                    (
                        workspace_id,
                        candidate_workspace_state,
                        candidate_threaded_decision_state,
                        execution_target,
                        touched_manifest,
                        state_changed || superseded,
                    )
                }
                Err(message) => {
                    self.finish_failed_decision_branch(
                        job,
                        message,
                        GraphMutationFailureUpdate::new(
                            workspace_id,
                            "Decision branch registration failed",
                        ),
                        window,
                        cx,
                    );
                    return;
                }
            }
        };
        let (
            workspace_id,
            workspace_state,
            threaded_decision_state,
            execution_target,
            touched_manifest,
            state_changed,
        ) = registration;

        let Some(persistence) = self.workspace_persistence_for_worker() else {
            self.finish_failed_decision_branch(
                job,
                "Beryl created the decision branch, but workspace persistence is unavailable.",
                GraphMutationFailureUpdate::new(
                    workspace_id,
                    "Decision branch graph persistence unavailable",
                ),
                window,
                cx,
            );
            return;
        };
        let optimistic_mutation = GraphOptimisticMutation::new(
            optimistic_mutation_id,
            graph_revision,
            graph_patch.clone(),
            [job.checklist_item_id.clone()],
            "Attaching decision branch to semantic graph",
        );
        if let Some(surface) = self.conversation_surface_mut() {
            if let Err(error) = surface.begin_optimistic_graph_mutation(optimistic_mutation) {
                self.finish_failed_decision_branch(
                    job,
                    error,
                    GraphMutationFailureUpdate::new(
                        workspace_id,
                        "Decision branch graph projection failed",
                    ),
                    window,
                    cx,
                );
                return;
            }
        } else {
            self.finish_failed_decision_branch(
                job,
                "Beryl created the decision branch, but the conversation surface is no longer available.",
                GraphMutationFailureUpdate::new(
                    workspace_id,
                    "Decision branch graph projection failed",
                ),
                window,
                cx,
            );
            return;
        }
        let graph_commit = match WorkspaceGraphToolService::new(persistence)
            .apply_threaded_decision_graph_patch(&GraphPatchWriteRequest {
                workspace_id: workspace_id.clone(),
                patch: graph_patch,
                expected_base_revision: Some(graph_revision),
            }) {
            Ok(response) => GraphMutationCommitUpdate::new(
                response.commit,
                "The semantic graph already contained the decision branch attachment.",
            )
            .with_optimistic_mutation_id(optimistic_mutation_id),
            Err(error) => {
                self.finish_failed_decision_branch(
                    job,
                    format!(
                        "Beryl created Codex thread {} but could not attach it to the semantic graph: {error}",
                        summary.id
                    ),
                    GraphMutationFailureUpdate::new(
                        workspace_id,
                        "Decision branch graph persistence failed",
                    )
                    .with_optimistic_mutation_id(optimistic_mutation_id),
                    window,
                    cx,
                );
                return;
            }
        };
        self.finish_graph_mutation_update(GraphMutationUpdate::Commit(graph_commit));

        if let Some(loaded) = self.workspace_shell_state_mut() {
            loaded.workspace_state = workspace_state.clone();
            loaded.threaded_decision_state = threaded_decision_state;
        } else {
            self.finish_failed_decision_branch(
                job,
                "Beryl created the decision branch, but the workspace is no longer loaded.",
                GraphMutationFailureUpdate::new(
                    workspace_id,
                    "Decision branch workspace publication failed",
                ),
                window,
                cx,
            );
            return;
        }
        if touched_manifest {
            self.persist_current_workspace_state(true);
        }
        if state_changed {
            self.persist_current_threaded_decision_state();
        }
        self.mark_member_thread_inventory_refresh_needed();

        if let Some(candidate) = ThreadTitleCandidate::new(
            branch_thread_id.as_str().to_string(),
            job.title_seed.clone(),
        ) {
            let _ = self.repair_thread_title_from_candidate(execution_target.clone(), candidate);
        }

        match job.activation {
            DecisionBranchActivation::Background => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Decision branch created",
                        "Beryl created the decision branch in the background.",
                    ));
                }
                self.resume_parent_after_decision_branch(parent_thread_id.as_str(), window, cx);
            }
            DecisionBranchActivation::SwitchTo => {
                let _ = self.activate_thread_selector_target(
                    super::super::thread_selector::ThreadSelectorActivationTarget {
                        thread_id: branch_thread_id,
                        label: summary.name.clone().unwrap_or_else(|| summary.id.clone()),
                        execution_target,
                    },
                    super::super::thread_navigation::ThreadNavigationActivationSource::NonHistory,
                    window,
                    cx,
                );
            }
        }
    }

    fn finish_failed_decision_branch(
        &mut self,
        job: QueuedDecisionBranchJob,
        message: impl Into<String>,
        graph_failure: GraphMutationFailureUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = message.into();
        warn!(error = %message, "decision branch failed");
        self.finish_graph_mutation_update(GraphMutationUpdate::Failure(graph_failure));
        self.remove_decision_branch_record(&job.record_id);
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Decision branch failed",
                message.clone(),
            ));
        }
        self.block_if_backend_process_dead(
            "Managed backend disconnected during decision branch creation",
            "The backend process exited before Beryl could finish creating the decision branch.",
            &message,
        );
        self.resume_parent_after_decision_branch(job.parent_thread_id.as_str(), window, cx);
    }

    pub(super) fn resume_parent_after_decision_branch(
        &mut self,
        parent_thread_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.begin_next_ready_queued_decision_branch(window, cx) {
            return;
        }
        match self.pending_lifecycle_phase_continue.take() {
            Some(request) if request.thread_id() == parent_thread_id => {
                self.begin_lifecycle_phase_continue(request, window, cx);
            }
            Some(request) => {
                self.pending_lifecycle_phase_continue = Some(request);
                self.begin_pending_turn_input_queue_for_thread(parent_thread_id);
            }
            None => {
                self.begin_pending_turn_input_queue_for_thread(parent_thread_id);
            }
        }
    }

    pub(super) fn remove_decision_branch_record(&mut self, record_id: &ThreadedDecisionRecordId) {
        let changed = self
            .workspace_shell_state_mut()
            .is_some_and(|loaded| loaded.threaded_decision_state.remove_record(record_id));
        if changed {
            self.persist_current_threaded_decision_state();
        }
    }

    pub(super) fn report_decision_branch_state_error(&mut self, error: ThreadedDecisionStateError) {
        warn!(error = %error, "threaded-decision branch state update failed");
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Decision branch unavailable",
                error.to_string(),
            ));
        }
    }

    pub(super) fn report_topic_decision_plan_error(&mut self, error: TopicDecisionItemPlanError) {
        warn!(error = %error, "topic decision item planning failed");
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(
                "Topic decision unavailable",
                error.message(),
            ));
        }
    }
}

fn register_decision_branch_thread(
    workspace_state: &mut beryl_model::conversation::WorkspaceConversationState,
    source_thread_id: &ConversationThreadId,
    source_turn_id: &ConversationTurnId,
    branch_summary: &ThreadSummary,
    bootstrap_turn_id: Option<ConversationTurnId>,
) -> Result<(WorkspaceId, bool), String> {
    let source_thread = workspace_state
        .thread_registration(source_thread_id)
        .ok_or_else(|| {
            format!(
                "Beryl could not register the branch because source thread {} is no longer registered in this workspace.",
                source_thread_id.as_str()
            )
        })?;
    let execution_target = source_thread.execution_target().clone();
    let member_binding = source_thread.member_binding().cloned();
    let copied_source_name =
        copied_source_backend_name(source_thread.backend_name(), branch_summary);

    if branch_summary.cwd.as_path() != execution_target.canonical_path() {
        return Err(format!(
            "Beryl forked thread {}, but it records working directory {} instead of the source thread workspace member {}.",
            branch_summary.id,
            branch_summary.cwd.display(),
            execution_target.canonical_path().display()
        ));
    }

    let mut registered_thread = beryl_model::conversation::RegisteredConversationThread::new(
        ConversationThreadId::new(branch_summary.id.clone()),
        execution_target.clone(),
        branch_summary.preview.clone(),
        if copied_source_name.is_some() {
            None
        } else {
            branch_summary.name.clone()
        },
        branch_summary.created_at,
        branch_summary.updated_at,
    )
    .with_beryl_created()
    .with_branch_parent_thread_id(source_thread_id.clone())
    .with_transcript_branch_bootstrap(source_turn_id.clone(), bootstrap_turn_id);
    if copied_source_name.is_some() {
        registered_thread =
            registered_thread.with_ignored_backend_name_for_automatic_title(copied_source_name);
    }
    if let Some(binding) = member_binding {
        registered_thread = registered_thread.with_member_binding(binding);
    }

    let changed = workspace_state.remember_thread(registered_thread);
    Ok((execution_target, changed))
}

fn copied_source_backend_name(
    source_backend_name: Option<&str>,
    branch_summary: &ThreadSummary,
) -> Option<String> {
    let source_backend_name = normalized_backend_name(source_backend_name)?;
    let branch_backend_name = normalized_backend_name(branch_summary.name.as_deref())?;
    (branch_backend_name == source_backend_name).then(|| branch_backend_name.to_string())
}

fn normalized_backend_name(name: Option<&str>) -> Option<&str> {
    let name = name?.trim();
    (!name.is_empty()).then_some(name)
}

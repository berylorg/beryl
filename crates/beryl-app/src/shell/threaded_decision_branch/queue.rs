use super::*;

impl ShellView {
    pub(in crate::shell) fn poll_decision_branch_start_updates(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(task) = self.decision_branch_start_receiver.as_ref() else {
            return false;
        };

        match task.try_recv() {
            Ok(DecisionBranchStartUpdate::Finished(outcome)) => {
                self.decision_branch_start_receiver = None;
                self.finish_decision_branch_start_worker(outcome, window, cx);
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                let record_id = task.record_id.clone();
                let parent_thread_id = task.parent_thread_id.clone();
                let graph_failure = task.disconnected_failure(
                    "Beryl lost the background task that was starting a decision branch.",
                );
                self.decision_branch_start_receiver = None;
                self.remove_decision_branch_record(&record_id);
                self.finish_graph_mutation_update(GraphMutationUpdate::Failure(graph_failure));
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(
                        "Decision branch failed",
                        "Beryl lost the background task that was starting the decision branch.",
                    ));
                }
                self.resume_parent_after_decision_branch(parent_thread_id.as_str(), window, cx);
                true
            }
        }
    }

    pub(in crate::shell) fn begin_next_ready_queued_decision_branch(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.decision_branch_start_receiver.is_some() {
            return false;
        }
        self.hydrate_ready_decision_branch_jobs();

        let Some(job) = self.pending_decision_branch_jobs.front() else {
            return false;
        };
        if let Some(blocker) = self.queued_decision_branch_job_blocker(job) {
            match blocker {
                QueuedDecisionBranchRunBlocker::MissingChecklistItem
                | QueuedDecisionBranchRunBlocker::MissingParentThread
                | QueuedDecisionBranchRunBlocker::MissingBranchPoint => {
                    let Some(job) = self.pending_decision_branch_jobs.pop_front() else {
                        return false;
                    };
                    self.remove_decision_branch_record(&job.record_id);
                    if let Some(surface) = self.conversation_surface_mut() {
                        surface.set_notice(SurfaceNotice::new(
                            "Decision branch unavailable",
                            "Beryl could not create a queued decision branch because its checklist item, parent thread, or completed branch point is no longer available.",
                        ));
                    }
                    self.resume_parent_after_decision_branch(
                        job.parent_thread_id.as_str(),
                        window,
                        cx,
                    );
                    return true;
                }
                QueuedDecisionBranchRunBlocker::BranchWorkerActive
                | QueuedDecisionBranchRunBlocker::GraphWorkActive
                | QueuedDecisionBranchRunBlocker::ParentTurnActive
                | QueuedDecisionBranchRunBlocker::ParentCompacting
                | QueuedDecisionBranchRunBlocker::BackendUnavailable => return false,
            }
        }

        let Some(mut job) = self.pending_decision_branch_jobs.pop_front() else {
            return false;
        };
        let Some(branch_point) = self.resolve_branch_point_for_job(&job) else {
            let record_id = job.record_id.clone();
            self.remove_decision_branch_record(&record_id);
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Decision branch unavailable",
                    "The parent turn did not complete successfully, so Beryl did not create the decision branch.",
                ));
            }
            self.resume_parent_after_decision_branch(job.parent_thread_id.as_str(), window, cx);
            return true;
        };
        if job.topic_item_creation.is_none() {
            job.title_seed = branch_point.title_seed;
        } else if job.title_seed.trim().is_empty() {
            job.title_seed = branch_point.title_seed;
        }
        job.parent_context_source = branch_point.parent_context_source;

        let Some(optimistic_mutation_id) = self.conversation_surface_mut().map(|surface| {
            surface.begin_graph_mutation("Starting decision branch");
            surface.reserve_optimistic_graph_mutation_id()
        }) else {
            return false;
        };
        let Some(connector) =
            self.backend_client_connector_for_execution_target(&job.execution_target)
        else {
            return false;
        };
        let Some(graph) = self
            .conversation_surface()
            .map(|surface| surface.graph_overlay().graph().clone())
        else {
            return false;
        };
        let Some(graph_revision) = self
            .conversation_surface()
            .map(|surface| surface.graph_overlay().revision())
        else {
            return false;
        };

        self.decision_branch_start_receiver = Some(spawn_decision_branch_start_worker(
            connector,
            graph,
            graph_revision,
            job,
            optimistic_mutation_id,
            self.bootstrap.probe_timeout(),
        ));
        self.schedule_poll_if_needed(window, cx);
        true
    }
    pub(in crate::shell) fn has_queued_decision_branch_jobs_for_parent(
        &self,
        thread_id: &str,
    ) -> bool {
        self.pending_decision_branch_jobs
            .iter()
            .any(|job| job.parent_thread_id.as_str() == thread_id)
            || self.workspace_shell_state().is_some_and(|loaded| {
                loaded
                    .threaded_decision_state
                    .records()
                    .iter()
                    .any(|record| {
                        record.status() == ThreadedDecisionStatus::QueuedBranch
                            && record.parent_thread_id().as_str() == thread_id
                    })
            })
    }
    fn hydrate_ready_decision_branch_jobs(&mut self) {
        if !self.pending_decision_branch_jobs.is_empty() {
            return;
        }
        let Some((workspace_id, records)) = self.workspace_shell_state().map(|loaded| {
            (
                loaded.workspace.id().clone(),
                loaded
                    .threaded_decision_state
                    .records()
                    .iter()
                    .filter(|record| record.status() == ThreadedDecisionStatus::QueuedBranch)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        }) else {
            return;
        };
        for record in records {
            let Some(branch_point_turn_id) = record.branch_point_turn_id().cloned() else {
                continue;
            };
            let Some((execution_target, parent_thread_title, parent_thread_summary)) =
                self.loaded_workspace().and_then(|loaded| {
                    loaded
                        .workspace_state
                        .thread_registration(record.parent_thread_id())
                        .map(|thread| {
                            (
                                thread.execution_target().clone(),
                                thread.title().map(ToString::to_string),
                                Some(thread.preview().trim().to_string())
                                    .filter(|summary| !summary.is_empty()),
                            )
                        })
                })
            else {
                continue;
            };
            self.pending_decision_branch_jobs
                .push_back(QueuedDecisionBranchJob {
                    workspace_id: workspace_id.clone(),
                    record_id: record.record_id().clone(),
                    checklist_item_id: record.checklist_item_id().clone(),
                    topic_item_creation: None,
                    parent_thread_id: record.parent_thread_id().clone(),
                    parent_thread_title,
                    parent_thread_summary,
                    branch_point_turn_id,
                    parent_context_source: None,
                    execution_target,
                    title_seed: String::new(),
                    provenance: record.provenance().created().clone(),
                    activation: DecisionBranchActivation::Background,
                });
        }
    }

    fn queued_decision_branch_job_blocker(
        &self,
        job: &QueuedDecisionBranchJob,
    ) -> Option<QueuedDecisionBranchRunBlocker> {
        let Some(surface) = self.conversation_surface() else {
            return Some(QueuedDecisionBranchRunBlocker::BackendUnavailable);
        };
        if surface.selected_thread_id() != Some(job.parent_thread_id.as_str()) {
            return Some(QueuedDecisionBranchRunBlocker::ParentTurnActive);
        }
        let graph = surface.graph_overlay().graph();
        let checklist_item_available = graph.node(&job.checklist_item_id).is_some()
            || job
                .topic_item_creation
                .as_ref()
                .is_some_and(|spec| graph.node(&spec.topic_id).is_some());
        let parent_turn_active = surface
            .execution_details
            .active_turn_identity()
            .is_some_and(|active| {
                active
                    .thread_id
                    .as_deref()
                    .is_none_or(|thread_id| thread_id == job.parent_thread_id.as_str())
            })
            || matches!(
                surface.selected_thread_status,
                Some(ThreadStatus::Active { .. })
            );
        queued_decision_branch_run_blocker(QueuedDecisionBranchRunGate {
            branch_worker_active: self.decision_branch_start_receiver.is_some(),
            graph_work_active: self.graph_receiver.is_some()
                || self.graph_thread_start_receiver.is_some(),
            parent_turn_active,
            parent_compacting: surface.context_compaction_thread_id()
                == Some(job.parent_thread_id.as_str()),
            backend_available: self
                .backend_client_connector_for_execution_target(&job.execution_target)
                .is_some(),
            checklist_item_exists: checklist_item_available,
            parent_thread_registered: self.loaded_workspace().is_some_and(|loaded| {
                loaded
                    .workspace_state
                    .thread_registration(&job.parent_thread_id)
                    .is_some()
            }),
            branch_point_available: self.resolve_branch_point_for_job(job).is_some(),
        })
    }

    fn resolve_branch_point_for_job(
        &self,
        job: &QueuedDecisionBranchJob,
    ) -> Option<DecisionBranchPointResolution> {
        let surface = self.conversation_surface()?;
        let (_, turn) =
            surface
                .execution_details
                .turns()
                .iter()
                .enumerate()
                .find(|(_, turn)| {
                    turn.has_resident_payload()
                        && turn.thread_id.as_deref() == Some(job.parent_thread_id.as_str())
                        && turn.turn_id.as_deref() == Some(job.branch_point_turn_id.as_str())
                        && turn.status == TurnExecutionStatus::Completed
                })?;
        Some(DecisionBranchPointResolution {
            title_seed: title_seed_for_turn_or_node(
                Some(turn),
                surface.graph_overlay().graph(),
                &job.checklist_item_id,
            ),
            parent_context_source: parent_context_source_for_turn(turn),
        })
    }
}

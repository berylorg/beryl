use super::*;

impl ShellView {
    pub(super) fn prepare_decision_branch_job(
        &mut self,
        node_id: SemanticNodeId,
        activation: DecisionBranchActivation,
        provenance: MutationProvenance,
    ) -> Option<QueuedDecisionBranchJob> {
        let blocker = self.decision_branch_start_blocker_for_node(&node_id);
        if let Some(blocker) = blocker {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new(
                    "Decision branch unavailable",
                    blocker.message(),
                ));
            }
            return None;
        }

        let branch_point = self.current_decision_branch_point(&node_id)?;
        let timestamp = token_usage_snapshot::current_unix_millis();
        let record_id =
            next_decision_record_id(&node_id, branch_point.parent_thread_id.as_str(), timestamp)?;
        let operation_id = next_decision_operation_id(&node_id, timestamp)?;
        let record = ThreadedDecisionRecord::queued_branch(
            record_id.clone(),
            node_id.clone(),
            branch_point.parent_thread_id.clone(),
            Some(branch_point.branch_point_turn_id.clone()),
            operation_id,
            timestamp,
            provenance.clone(),
        );
        let inserted = match self.workspace_shell_state_mut() {
            Some(loaded) => loaded.threaded_decision_state.insert_record(record),
            None => return None,
        };
        match inserted {
            Ok(_) => {}
            Err(error) => {
                self.report_decision_branch_state_error(error);
                return None;
            }
        }

        Some(QueuedDecisionBranchJob {
            workspace_id: self.loaded_workspace()?.workspace.id().clone(),
            record_id,
            checklist_item_id: node_id,
            topic_item_creation: None,
            parent_thread_id: branch_point.parent_thread_id,
            parent_thread_title: branch_point.parent_thread_title,
            parent_thread_summary: branch_point.parent_thread_summary,
            branch_point_turn_id: branch_point.branch_point_turn_id,
            parent_context_source: branch_point.parent_context_source,
            execution_target: branch_point.execution_target,
            title_seed: branch_point.title_seed,
            provenance,
            activation,
        })
    }

    pub(super) fn prepare_topic_decision_branch_job(
        &mut self,
        topic_id: SemanticNodeId,
        title: String,
        summary: String,
        activation: DecisionBranchActivation,
        provenance: MutationProvenance,
    ) -> Option<(QueuedDecisionBranchJob, SemanticNodeId)> {
        if let Some(message) = self.topic_decision_start_blocker(&topic_id, &title, &summary) {
            if let Some(surface) = self.conversation_surface_mut() {
                surface.set_notice(SurfaceNotice::new("Topic decision unavailable", message));
            }
            return None;
        }

        let plan_result = {
            let graph = self.conversation_surface()?.graph_overlay().graph();
            topic_decision_item_plan(graph, &topic_id, title, summary, &provenance)
        };
        let plan = match plan_result {
            Ok(plan) => plan,
            Err(error) => {
                self.report_topic_decision_plan_error(error);
                return None;
            }
        };
        if plan.reused_existing_item() {
            let checklist_item_id = plan.checklist_item_id.clone();
            let job = self.prepare_decision_branch_job(
                checklist_item_id.clone(),
                activation,
                provenance,
            )?;
            return Some((job, checklist_item_id));
        }

        let branch_point = self.current_decision_branch_point(&topic_id)?;
        let timestamp = token_usage_snapshot::current_unix_millis();
        let record_id = next_decision_record_id(
            &plan.checklist_item_id,
            branch_point.parent_thread_id.as_str(),
            timestamp,
        )?;
        let operation_id = next_decision_operation_id(&plan.checklist_item_id, timestamp)?;
        let record = ThreadedDecisionRecord::queued_branch(
            record_id.clone(),
            plan.checklist_item_id.clone(),
            branch_point.parent_thread_id.clone(),
            Some(branch_point.branch_point_turn_id.clone()),
            operation_id,
            timestamp,
            provenance.clone(),
        );
        let inserted = match self.workspace_shell_state_mut() {
            Some(loaded) => loaded.threaded_decision_state.insert_record(record),
            None => return None,
        };
        match inserted {
            Ok(_) => {}
            Err(error) => {
                self.report_decision_branch_state_error(error);
                return None;
            }
        }

        let checklist_item_id = plan.checklist_item_id;
        Some((
            QueuedDecisionBranchJob {
                workspace_id: self.loaded_workspace()?.workspace.id().clone(),
                record_id,
                checklist_item_id: checklist_item_id.clone(),
                topic_item_creation: Some(TopicDecisionItemSpec {
                    topic_id,
                    title: plan.title.clone(),
                    summary: plan.summary.clone(),
                }),
                parent_thread_id: branch_point.parent_thread_id,
                parent_thread_title: branch_point.parent_thread_title,
                parent_thread_summary: branch_point.parent_thread_summary,
                branch_point_turn_id: branch_point.branch_point_turn_id,
                parent_context_source: branch_point.parent_context_source,
                execution_target: branch_point.execution_target,
                title_seed: plan.title,
                provenance,
                activation,
            },
            checklist_item_id,
        ))
    }

    pub(super) fn default_topic_decision_title(&self, topic_id: &SemanticNodeId) -> Option<String> {
        let graph = self.conversation_surface()?.graph_overlay().graph();
        default_topic_decision_title(graph, topic_id)
    }

    pub(super) fn topic_decision_start_blocker(
        &self,
        topic_id: &SemanticNodeId,
        title: &str,
        summary: &str,
    ) -> Option<String> {
        let Some(surface) = self.conversation_surface() else {
            return Some(
                "Open a registered parent conversation thread before starting a decision."
                    .to_string(),
            );
        };
        let graph = surface.graph_overlay().graph();
        if self.graph_receiver.is_some()
            || self.graph_thread_start_receiver.is_some()
            || self.decision_branch_start_receiver.is_some()
        {
            return Some("Retry after the current semantic graph operation finishes.".to_string());
        }
        let parent_thread_id = surface.selected_thread_id();
        if !parent_thread_id.is_some_and(|thread_id| {
            self.loaded_workspace().is_some_and(|loaded| {
                loaded
                    .workspace_state
                    .thread_registration(&ConversationThreadId::new(thread_id.to_string()))
                    .is_some()
            })
        }) {
            return Some(
                "Open a registered parent conversation thread before starting a decision."
                    .to_string(),
            );
        }
        let backend_available = self
            .current_decision_branch_execution_target()
            .and_then(|target| self.backend_client_connector_for_execution_target(&target))
            .is_some();
        if !backend_available {
            return Some(
                "Beryl does not have an active managed backend for the parent thread.".to_string(),
            );
        }
        let active = surface.execution_details.active_turn_identity();
        let active_parent_turn = active.as_ref().is_some_and(|active| {
            active.turn_id.is_some()
                && active
                    .thread_id
                    .as_deref()
                    .is_none_or(|thread_id| Some(thread_id) == parent_thread_id)
        });
        if matches!(
            surface.selected_thread_status,
            Some(ThreadStatus::Active { .. })
        ) && !active_parent_turn
        {
            return Some(
                "The parent thread is busy and Beryl cannot identify the active turn to queue after."
                    .to_string(),
            );
        }
        if surface
            .context_compaction_thread_id()
            .is_some_and(|thread_id| Some(thread_id) == parent_thread_id)
        {
            return Some("Wait for context compaction to finish before branching.".to_string());
        }
        if self.current_decision_branch_point(topic_id).is_none() && !active_parent_turn {
            return Some(
                "The parent conversation has no loaded turn to use as the branch point."
                    .to_string(),
            );
        }
        let plan = match topic_decision_item_plan(
            graph,
            topic_id,
            title.to_string(),
            summary.to_string(),
            &workspace_action_provenance("validate_topic_decision"),
        ) {
            Ok(plan) => plan,
            Err(error) => return Some(error.message()),
        };
        if self.loaded_workspace().is_some_and(|loaded| {
            loaded
                .threaded_decision_state
                .active_record_for_item(&plan.checklist_item_id)
                .is_some()
        }) {
            return Some("This checklist item already has an active decision branch.".to_string());
        }

        None
    }

    pub(super) fn decision_branch_start_blocker_for_node(
        &self,
        node_id: &SemanticNodeId,
    ) -> Option<crate::threaded_decision_branch_core::DecisionBranchStartBlocker> {
        let Some(surface) = self.conversation_surface() else {
            return Some(crate::threaded_decision_branch_core::DecisionBranchStartBlocker::MissingParentThread);
        };
        let graph = surface.graph_overlay().graph();
        let node = graph.node(node_id);
        let parent_thread_id = surface.selected_thread_id();
        let active = surface.execution_details.active_turn_identity();
        let active_parent_turn = active.as_ref().is_some_and(|active| {
            active.turn_id.is_some()
                && active
                    .thread_id
                    .as_deref()
                    .is_none_or(|thread_id| Some(thread_id) == parent_thread_id)
        });
        let parent_thread_busy_without_queue = matches!(
            surface.selected_thread_status,
            Some(ThreadStatus::Active { .. })
        ) && !active_parent_turn;
        let branch_point_available =
            self.current_decision_branch_point(node_id).is_some() || active_parent_turn;
        let backend_available = self
            .current_decision_branch_execution_target()
            .and_then(|target| self.backend_client_connector_for_execution_target(&target))
            .is_some();

        decision_branch_start_blocker(DecisionBranchStartGate {
            graph_work_active: self.graph_receiver.is_some()
                || self.graph_thread_start_receiver.is_some()
                || self.decision_branch_start_receiver.is_some(),
            backend_available,
            checklist_item_exists: node.is_some(),
            checklist_item: node.is_some_and(|node| node.facets().has_checklist_item()),
            parent_thread_registered: parent_thread_id.is_some_and(|thread_id| {
                self.loaded_workspace().is_some_and(|loaded| {
                    loaded
                        .workspace_state
                        .thread_registration(&ConversationThreadId::new(thread_id.to_string()))
                        .is_some()
                })
            }),
            branch_point_available,
            active_branch_exists: self.loaded_workspace().is_some_and(|loaded| {
                loaded
                    .threaded_decision_state
                    .active_record_for_item(node_id)
                    .is_some()
            }),
            parent_thread_busy_without_queue,
            parent_compacting: surface
                .context_compaction_thread_id()
                .is_some_and(|thread_id| Some(thread_id) == parent_thread_id),
        })
    }

    fn current_decision_branch_point(
        &self,
        node_id: &SemanticNodeId,
    ) -> Option<DecisionBranchPoint> {
        let surface = self.conversation_surface()?;
        let parent_thread_id = surface.selected_thread_id()?.to_string();
        if surface
            .context_compaction_thread_id()
            .is_some_and(|thread_id| thread_id == parent_thread_id)
        {
            return None;
        }
        let execution_target = self.current_decision_branch_execution_target()?;
        let parent_registration = self.loaded_workspace().and_then(|loaded| {
            loaded
                .workspace_state
                .thread_registration(&ConversationThreadId::new(parent_thread_id.clone()))
        });
        let parent_thread_title =
            parent_registration.and_then(|thread| thread.title().map(ToString::to_string));
        let parent_thread_summary = parent_registration
            .map(|thread| thread.preview().trim().to_string())
            .filter(|summary| !summary.is_empty());

        if let Some(active) = surface.execution_details.active_turn_identity()
            && active
                .thread_id
                .as_deref()
                .is_none_or(|thread_id| thread_id == parent_thread_id)
        {
            let turn_id = active.turn_id?;
            return Some(DecisionBranchPoint {
                parent_thread_id: ConversationThreadId::new(parent_thread_id),
                parent_thread_title,
                parent_thread_summary,
                branch_point_turn_id: ConversationTurnId::new(turn_id),
                parent_context_source: surface
                    .execution_details
                    .turns()
                    .get(active.turn_index)
                    .and_then(|turn| parent_context_source_for_turn(turn)),
                execution_target,
                title_seed: title_seed_for_turn_or_node(
                    surface.execution_details.turns().get(active.turn_index),
                    surface.graph_overlay().graph(),
                    node_id,
                ),
            });
        }

        if !matches!(surface.selected_thread_status, Some(ThreadStatus::Idle)) {
            return None;
        }
        let (_, turn) = surface
            .execution_details
            .turns()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, turn)| {
                turn.has_resident_payload()
                    && turn.thread_id.as_deref() == Some(parent_thread_id.as_str())
                    && turn.turn_id.is_some()
                    && turn.status == TurnExecutionStatus::Completed
            })?;
        Some(DecisionBranchPoint {
            parent_thread_id: ConversationThreadId::new(parent_thread_id),
            parent_thread_title,
            parent_thread_summary,
            branch_point_turn_id: ConversationTurnId::new(turn.turn_id.clone()?),
            parent_context_source: parent_context_source_for_turn(turn),
            execution_target,
            title_seed: title_seed_for_turn_or_node(
                Some(turn),
                surface.graph_overlay().graph(),
                node_id,
            ),
        })
    }

    fn current_decision_branch_execution_target(&self) -> Option<WorkspaceId> {
        match &self.state {
            ShellState::Ready(ready) => Some(ready.execution_target.clone()),
            ShellState::BackendUnavailable(unavailable) => {
                Self::selected_thread_registered_execution_target(
                    &unavailable.loaded_workspace,
                    &unavailable.surface,
                )
                .or_else(|| Some(unavailable.execution_target.clone()))
            }
            ShellState::Blocked(_)
            | ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        }
    }
}

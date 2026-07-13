use super::*;

impl ShellView {
    pub(crate) fn decision_branch_start_disabled_reason(
        &self,
        node_id: &SemanticNodeId,
    ) -> Option<String> {
        let blocker = self.decision_branch_start_blocker_for_node(node_id)?;
        Some(blocker.message().to_string())
    }

    pub(crate) fn topic_decision_start_disabled_reason(
        &self,
        topic_id: &SemanticNodeId,
    ) -> Option<String> {
        let title = self.default_topic_decision_title(topic_id)?;
        self.topic_decision_start_blocker(topic_id, &title, "")
    }

    pub(crate) fn start_decision_branch_from_node(
        &mut self,
        node_id: SemanticNodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = self.prepare_decision_branch_job(
            node_id,
            DecisionBranchActivation::SwitchTo,
            workspace_action_provenance(DECISION_BRANCH_ACTION),
        ) else {
            cx.notify();
            return;
        };

        self.pending_decision_branch_jobs.push_back(job);
        self.persist_current_threaded_decision_state();
        self.begin_next_ready_queued_decision_branch(window, cx);
        cx.notify();
    }

    pub(crate) fn start_topic_decision_from_node(
        &mut self,
        topic_id: SemanticNodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = self
            .default_topic_decision_title(&topic_id)
            .unwrap_or_else(|| "Decision".to_string());
        let Some((job, _)) = self.prepare_topic_decision_branch_job(
            topic_id,
            title,
            String::new(),
            DecisionBranchActivation::SwitchTo,
            workspace_action_provenance("start_topic_decision"),
        ) else {
            cx.notify();
            return;
        };

        self.pending_decision_branch_jobs.push_back(job);
        self.persist_current_threaded_decision_state();
        self.begin_next_ready_queued_decision_branch(window, cx);
        cx.notify();
    }
    pub(in crate::shell) fn handle_beryl_threaded_decision_dynamic_tool_request(
        &mut self,
        request: &DynamicToolCallRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DynamicToolCallResponse {
        let parsed = match parse_beryl_threaded_decision_dynamic_tool_request(request) {
            Ok(parsed) => parsed,
            Err(error) => return threaded_decision_tool_failure_response(request, error),
        };

        let checklist_item_ids = match parsed {
            ThreadedDecisionDynamicToolRequest::StartDecisionBranch { checklist_item_ids } => {
                checklist_item_ids
            }
            ThreadedDecisionDynamicToolRequest::StartTopicDecision {
                topic_node_id,
                title,
                summary,
            } => {
                return self.handle_start_topic_decision_dynamic_tool_request(
                    request,
                    topic_node_id,
                    title,
                    summary,
                    window,
                    cx,
                );
            }
            ThreadedDecisionDynamicToolRequest::ResolveDecisionBranch {
                outcome,
                summary,
                handoff_message,
            } => {
                return self.handle_decision_resolution_dynamic_tool_request(
                    request,
                    outcome,
                    summary,
                    handoff_message,
                    window,
                    cx,
                );
            }
        };

        let active_matches = self.conversation_surface().is_some_and(|surface| {
            let Some(active) = surface.active_turn_state.active_turn_identity() else {
                return false;
            };
            active.thread_id.as_deref() == Some(request.thread_id())
                && active.turn_id.as_deref() == Some(request.turn_id())
        });

        let mut results = Vec::new();
        if !active_matches {
            for checklist_item_id in checklist_item_ids {
                results.push(DecisionBranchToolItemResult::failed(
                    checklist_item_id,
                    "The decision branch tool must be called from the selected active parent turn.",
                ));
            }
            return decision_branch_tool_success_response(results);
        }

        let provenance = match dynamic_tool_provenance(request, START_DECISION_BRANCH_TOOL) {
            Ok(provenance) => provenance,
            Err(error) => {
                return DynamicToolCallResponse::failure_text(format!(
                    "{{\"ok\":false,\"error\":{{\"kind\":\"invalid_provenance\",\"message\":{}}}}}",
                    serde_json::to_string(&error.to_string())
                        .unwrap_or_else(|_| "\"invalid provenance\"".to_string())
                ));
            }
        };

        for checklist_item_id in checklist_item_ids {
            match self.prepare_decision_branch_job(
                checklist_item_id.clone(),
                DecisionBranchActivation::Background,
                provenance.clone(),
            ) {
                Some(job) => {
                    results.push(DecisionBranchToolItemResult::queued(
                        checklist_item_id,
                        job.record_id.as_str(),
                        job.parent_thread_id.as_str(),
                        job.branch_point_turn_id.as_str(),
                    ));
                    self.pending_decision_branch_jobs.push_back(job);
                }
                None => {
                    let message = self
                        .decision_branch_start_blocker_for_node(&checklist_item_id)
                        .map(|blocker| blocker.message().to_string())
                        .unwrap_or_else(|| {
                            "Beryl could not queue this decision branch from the current shell state."
                                .to_string()
                        });
                    results.push(DecisionBranchToolItemResult::failed(
                        checklist_item_id,
                        message,
                    ));
                }
            }
        }

        self.persist_current_threaded_decision_state();
        self.begin_next_ready_queued_decision_branch(window, cx);
        cx.notify();
        decision_branch_tool_success_response(results)
    }

    fn handle_start_topic_decision_dynamic_tool_request(
        &mut self,
        request: &DynamicToolCallRequest,
        topic_node_id: SemanticNodeId,
        title: String,
        summary: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DynamicToolCallResponse {
        let active_matches = self.conversation_surface().is_some_and(|surface| {
            let Some(active) = surface.active_turn_state.active_turn_identity() else {
                return false;
            };
            active.thread_id.as_deref() == Some(request.thread_id())
                && active.turn_id.as_deref() == Some(request.turn_id())
        });
        if !active_matches {
            return topic_decision_tool_success_response(TopicDecisionToolResult::failed(
                topic_node_id,
                "The topic decision tool must be called from the selected active parent turn.",
            ));
        }

        let provenance = match dynamic_tool_provenance(request, START_TOPIC_DECISION_TOOL) {
            Ok(provenance) => provenance,
            Err(error) => {
                return DynamicToolCallResponse::failure_text(format!(
                    "{{\"ok\":false,\"error\":{{\"kind\":\"invalid_provenance\",\"message\":{}}}}}",
                    serde_json::to_string(&error.to_string())
                        .unwrap_or_else(|_| "\"invalid provenance\"".to_string())
                ));
            }
        };

        let title_for_error = title.clone();
        let summary_for_error = summary.clone();
        let result = match self.prepare_topic_decision_branch_job(
            topic_node_id.clone(),
            title,
            summary,
            DecisionBranchActivation::Background,
            provenance,
        ) {
            Some((job, checklist_item_id)) => {
                let result = TopicDecisionToolResult::queued(
                    topic_node_id,
                    checklist_item_id,
                    job.record_id.as_str(),
                    job.parent_thread_id.as_str(),
                    job.branch_point_turn_id.as_str(),
                );
                self.pending_decision_branch_jobs.push_back(job);
                self.persist_current_threaded_decision_state();
                self.begin_next_ready_queued_decision_branch(window, cx);
                cx.notify();
                result
            }
            None => {
                let message = self
                    .topic_decision_start_blocker(
                        &topic_node_id,
                        &title_for_error,
                        &summary_for_error,
                    )
                    .unwrap_or_else(|| {
                        "Beryl could not queue this topic decision from the current shell state."
                            .to_string()
                    });
                TopicDecisionToolResult::failed(topic_node_id, message)
            }
        };

        topic_decision_tool_success_response(result)
    }
}

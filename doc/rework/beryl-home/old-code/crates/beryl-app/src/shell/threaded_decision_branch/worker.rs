use super::*;

pub(super) fn spawn_decision_branch_start_worker(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    graph: SemanticGraph,
    graph_revision: WorkspaceGraphRevision,
    job: QueuedDecisionBranchJob,
    optimistic_mutation_id: OptimisticGraphMutationId,
    timeout: Duration,
) -> DecisionBranchStartTask {
    let (sender, receiver) = mpsc::channel();
    let task = DecisionBranchStartTask::new(
        job.workspace_id.clone(),
        job.record_id.clone(),
        job.parent_thread_id.clone(),
        optimistic_mutation_id,
        receiver,
    );
    thread::spawn(move || {
        let outcome = run_decision_branch_start(
            persistence,
            connector,
            graph,
            graph_revision,
            job,
            optimistic_mutation_id,
            timeout,
        );
        let _ = sender.send(DecisionBranchStartUpdate::Finished(outcome));
    });
    task
}

fn run_decision_branch_start(
    persistence: BerylWorkspacePersistence,
    connector: ManagedBackendClientConnector,
    mut graph: SemanticGraph,
    graph_revision: WorkspaceGraphRevision,
    job: QueuedDecisionBranchJob,
    optimistic_mutation_id: OptimisticGraphMutationId,
    timeout: Duration,
) -> DecisionBranchStartOutcome {
    let mut planned_item_patch = None;
    let mut planned_parent_topic_id = None;
    let (checklist_item_title, checklist_item_summary) = if let Some(node) =
        graph.node(&job.checklist_item_id)
    {
        (
            node.title().trim().to_string(),
            node.summary().trim().to_string(),
        )
    } else if let Some(spec) = job.topic_item_creation.clone() {
        let plan = match topic_decision_item_plan(
            &graph,
            &spec.topic_id,
            spec.title,
            spec.summary,
            &job.provenance,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                return failed_decision_branch_start(
                    job,
                    format!("Beryl could not plan the topic decision item: {error}"),
                    optimistic_mutation_id,
                );
            }
        };
        if plan.checklist_item_id != job.checklist_item_id {
            return failed_decision_branch_start(
                job,
                "Beryl found a different decision item for that topic and title than the queued decision record expects.",
                optimistic_mutation_id,
            );
        }
        planned_parent_topic_id = Some(spec.topic_id);
        planned_item_patch = plan.patch.clone();
        (plan.title, plan.summary)
    } else {
        return failed_decision_branch_start(
            job,
            "Beryl could not find the checklist item for this decision branch.",
            optimistic_mutation_id,
        );
    };

    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            return failed_decision_branch_start(
                job,
                format!("Beryl could not connect to the managed backend: {error}"),
                optimistic_mutation_id,
            );
        }
    };
    let thread =
        match start_empty_decision_child_thread(&mut session, &job.execution_target, timeout) {
            Ok(thread) => thread,
            Err(message) => {
                return failed_decision_branch_start(job, message, optimistic_mutation_id);
            }
        };

    let summary = thread.summary();
    let child_thread_id = ConversationThreadId::new(summary.id.clone());
    let syndic_registration = match syndic_ingestion::register_cas_branch_thread_view(
        &persistence,
        &job.workspace_id,
        &job.execution_target,
        &summary.id,
        summary.created_at,
        summary.updated_at,
        job.parent_syndic_view_id.as_str(),
        job.branch_point_turn_id.as_str(),
    ) {
        Ok(registration) => registration,
        Err(error) => {
            return failed_decision_branch_start(
                job,
                format!(
                    "Beryl created Codex thread {} but could not register its Syndic branch view: {error}",
                    summary.id
                ),
                optimistic_mutation_id,
            );
        }
    };
    let context = threaded_decision_bootstrap_context(ThreadedDecisionBootstrapContextInput {
        graph: &graph,
        checklist_item_id: &job.checklist_item_id,
        checklist_item_title: &checklist_item_title,
        checklist_item_summary: &checklist_item_summary,
        planned_parent_topic_id: planned_parent_topic_id.as_ref(),
        parent_thread_id: &job.parent_thread_id,
        parent_thread_title: job.parent_thread_title.as_deref(),
        parent_thread_summary: job.parent_thread_summary.as_deref(),
        child_thread_id: &child_thread_id,
        parent_context_turn_id: Some(&job.branch_point_turn_id),
        parent_context_source: job.parent_context_source.as_deref(),
        record_id: &job.record_id,
    });
    let bootstrap_message = branch_bootstrap_message(BranchBootstrapMessageInput {
        parent_thread_id: &job.parent_thread_id,
        parent_thread_title: job.parent_thread_title.as_deref(),
        branch_context: Some(context.text()),
    });
    let bootstrap_fragment = UserInputFragment::text(bootstrap_message.clone());
    let bootstrap_admission = match syndic_ingestion::admit_user_turn(
        &persistence,
        &job.workspace_id,
        &job.execution_target,
        Some(child_thread_id.as_str()),
        std::slice::from_ref(&bootstrap_fragment),
    ) {
        Ok(admission) => admission,
        Err(error) => {
            return failed_decision_branch_start(
                job,
                format!(
                    "Beryl registered Codex thread {} but could not admit its Syndic bootstrap turn: {error}",
                    summary.id
                ),
                optimistic_mutation_id,
            );
        }
    };
    let mut ingestor = match SyndicLiveTurnIngestor::new(bootstrap_admission) {
        Ok(ingestor) => ingestor,
        Err(error) => {
            return failed_decision_branch_start(
                job,
                format!(
                    "Beryl registered Codex thread {} but could not open Syndic bootstrap capture: {error}",
                    summary.id
                ),
                optimistic_mutation_id,
            );
        }
    };
    if let Err(error) = ingestor.bind_cas_thread(child_thread_id.as_str()) {
        return failed_decision_branch_start(
            job,
            format!(
                "Beryl registered Codex thread {} but could not bind it into Syndic: {error}",
                summary.id
            ),
            optimistic_mutation_id,
        );
    }

    let bootstrap = match start_branch_bootstrap_turn_only(
        &mut session,
        &child_thread_id,
        &bootstrap_message,
        timeout,
    ) {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            let message = error.to_string();
            let _ = ingestor.mark_local_failure(message.clone());
            return failed_decision_branch_start(job, message, optimistic_mutation_id);
        }
    };
    let turn_started_event = TurnStreamEvent::TurnStarted {
        thread_id: child_thread_id.as_str().to_string(),
        turn: bootstrap.turn().clone(),
    };
    if let Err(error) = ingestor.ingest_event(&turn_started_event) {
        let message = format!("Beryl could not persist decision branch bootstrap start: {error}");
        let _ = ingestor.mark_local_failure(message.clone());
        return failed_decision_branch_start(job, message, optimistic_mutation_id);
    }
    let terminal_turn = match wait_for_decision_branch_bootstrap_terminal(
        &mut session,
        &mut ingestor,
        child_thread_id.as_str(),
        bootstrap.bootstrap_turn_id(),
        bootstrap.turn().clone(),
        timeout,
    ) {
        Ok(turn) => turn,
        Err(message) => {
            return failed_decision_branch_start(job, message, optimistic_mutation_id);
        }
    };
    if terminal_turn.status != TurnStatus::Completed {
        let message = format!(
            "Beryl decision branch bootstrap turn {} ended with status {:?}.",
            bootstrap.bootstrap_turn_id().as_str(),
            terminal_turn.status
        );
        let _ = ingestor.mark_local_failure(message.clone());
        return failed_decision_branch_start(job, message, optimistic_mutation_id);
    }
    if !turn_has_visible_bootstrap_message(&terminal_turn, &bootstrap_message) {
        let message = format!(
            "Beryl decision branch bootstrap turn {} did not contain the visible branch provenance message.",
            bootstrap.bootstrap_turn_id().as_str()
        );
        let _ = ingestor.mark_local_failure(message.clone());
        return failed_decision_branch_start(job, message, optimistic_mutation_id);
    }
    let durable_summary = match prove_branch_thread_durable_with_bootstrap_turn(
        &mut session,
        &child_thread_id,
        bootstrap.bootstrap_turn_id(),
        timeout,
    ) {
        Ok(summary) => summary,
        Err(error) => {
            let message = error.to_string();
            let _ = ingestor.mark_local_failure(message.clone());
            return failed_decision_branch_start(job, message, optimistic_mutation_id);
        }
    };

    if let Some(patch) = planned_item_patch.as_ref()
        && let Err(error) = graph.apply_patch(patch)
    {
        return failed_decision_branch_start(
            job,
            format!(
                "Beryl bootstrapped Codex thread {} but could not prepare the topic decision item graph patch: {error}",
                durable_summary.id
            ),
            optimistic_mutation_id,
        );
    }

    let Some((_, ref_patch)) = decision_branch_graph_patch(
        &graph,
        &job.checklist_item_id,
        child_thread_id.clone(),
        job.execution_target.clone(),
        &job.provenance,
    ) else {
        return failed_decision_branch_start(
            job,
            format!(
                "Beryl created Codex thread {} but could not build a valid decision graph attachment for it.",
                durable_summary.id
            ),
            optimistic_mutation_id,
        );
    };
    let mut operations = Vec::new();
    if let Some(patch) = planned_item_patch {
        operations.extend_from_slice(patch.operations());
    }
    operations.extend_from_slice(ref_patch.operations());
    let patch = SemanticGraphPatch::new(operations);

    DecisionBranchStartOutcome::Started {
        job,
        thread_summary: durable_summary,
        syndic_registration,
        bootstrap_turn_id: Some(bootstrap.bootstrap_turn_id().clone()),
        graph_patch: patch,
        graph_revision,
        optimistic_mutation_id,
    }
}

fn failed_decision_branch_start(
    job: QueuedDecisionBranchJob,
    message: impl Into<String>,
    optimistic_mutation_id: OptimisticGraphMutationId,
) -> DecisionBranchStartOutcome {
    let message = message.into();
    DecisionBranchStartOutcome::Failed {
        graph_failure: GraphMutationFailureUpdate::new(job.workspace_id.clone(), message.clone())
            .with_optimistic_mutation_id(optimistic_mutation_id),
        job,
        message,
    }
}

fn wait_for_decision_branch_bootstrap_terminal<B>(
    session: &mut B,
    ingestor: &mut SyndicLiveTurnIngestor,
    branch_thread_id: &str,
    bootstrap_turn_id: &ConversationTurnId,
    started_turn: TurnInfo,
    idle_timeout: Duration,
) -> Result<TurnInfo, String>
where
    B: BranchBootstrapBackend,
{
    if started_turn.is_terminal() {
        ingestor
            .ingest_event(&TurnStreamEvent::TurnCompleted {
                thread_id: branch_thread_id.to_string(),
                turn: started_turn.clone(),
            })
            .map_err(|error| {
                format!("Beryl could not persist terminal decision branch bootstrap turn: {error}")
            })?;
        return Ok(started_turn);
    }

    loop {
        let event = match session.next_turn_stream_event(idle_timeout) {
            Ok(Some(event)) => event,
            Ok(None) => {
                let message =
                    "timed out waiting for live decision branch bootstrap completion event"
                        .to_string();
                let _ = ingestor.mark_stream_lost(message.clone());
                return Err(message);
            }
            Err(error) => {
                let message =
                    format!("Beryl lost the decision branch bootstrap execution stream: {error}");
                let _ = ingestor.mark_stream_lost(message.clone());
                return Err(message);
            }
        };

        match event {
            TurnStreamEvent::ProtocolError { error } => {
                let message = format!(
                    "Beryl received a protocol error during decision branch bootstrap: {}",
                    error.message
                );
                let _ = ingestor.mark_stream_lost(message.clone());
                return Err(message);
            }
            TurnStreamEvent::ApprovalRequested(request) => {
                if let Err(error) = session.deny_approval_request(&request) {
                    let message =
                        format!("Beryl could not deny decision branch bootstrap approval: {error}");
                    let _ = ingestor.mark_local_failure(message.clone());
                    return Err(message);
                }
                let message = format!(
                    "Beryl decision branch bootstrap requested an approval unexpectedly: {}",
                    request.summary()
                );
                let _ = ingestor.mark_local_failure(message.clone());
                return Err(message);
            }
            TurnStreamEvent::DynamicToolCallRequested(request) => {
                let response = bootstrap_dynamic_tool_unavailable_response(&request);
                if let Err(error) = session.respond_dynamic_tool_call(&request, &response) {
                    let message = format!(
                        "Beryl could not reject decision branch bootstrap dynamic tool call: {error}"
                    );
                    let _ = ingestor.mark_local_failure(message.clone());
                    return Err(message);
                }
                let message = format!(
                    "Beryl decision branch bootstrap requested a dynamic tool unexpectedly: {}",
                    request.summary()
                );
                let _ = ingestor.mark_local_failure(message.clone());
                return Err(message);
            }
            TurnStreamEvent::TurnCompleted { thread_id, turn }
                if thread_id == branch_thread_id && turn.id == bootstrap_turn_id.as_str() =>
            {
                ingestor
                    .ingest_event(&TurnStreamEvent::TurnCompleted {
                        thread_id,
                        turn: turn.clone(),
                    })
                    .map_err(|error| {
                        format!(
                            "Beryl could not persist terminal decision branch bootstrap event in Syndic: {error}"
                        )
                    })?;
                return Ok(turn);
            }
            TurnStreamEvent::ThreadStatusChanged { thread_id, status }
                if thread_id == branch_thread_id && matches!(status, ThreadStatus::Idle) =>
            {
                let event = TurnStreamEvent::ThreadStatusChanged { thread_id, status };
                if let Err(error) = ingestor.ingest_event(&event) {
                    warn!(
                        error = %error,
                        "failed to persist idle decision branch bootstrap status before reporting failure"
                    );
                }
                let message = "decision branch bootstrap thread became idle before Beryl observed the live bootstrap completion event".to_string();
                let _ = ingestor.mark_stream_lost(message.clone());
                return Err(message);
            }
            event => {
                ingestor.ingest_event(&event).map_err(|error| {
                    format!(
                        "Beryl could not persist a decision branch bootstrap event in Syndic: {error}"
                    )
                })?;
            }
        }
    }
}

use super::*;

pub(super) fn spawn_decision_branch_start_worker(
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
    let bootstrap = match start_branch_bootstrap_turn(
        &mut session,
        &child_thread_id,
        &bootstrap_message,
        timeout,
    ) {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            return failed_decision_branch_start(job, error.to_string(), optimistic_mutation_id);
        }
    };
    let durable_summary = bootstrap.thread().clone();

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
        durable_summary.name.as_deref(),
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
        bootstrap_turn_id: bootstrap.bootstrap_turn_id().cloned(),
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

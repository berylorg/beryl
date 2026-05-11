use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use beryl_backend::{
    ManagedBackendClientConnector, ManagedBackendSession, ThreadInfo, ThreadSessionMetadata,
    ThreadSummary,
};
use beryl_model::{
    conversation::ConversationThreadId,
    provenance::{MutationProvenance, MutationSource},
    semantic_graph::{SemanticGraph, SemanticNodeId, ThreadRefDraft, ThreadRefId},
    workspace::{BerylWorkspaceId, WorkspaceId},
};
use gpui::{Context, Window};

use crate::{
    BerylWorkspacePersistence, ThreadRefUpsertRequest, WorkspaceGraphMutationCommit,
    WorkspaceGraphRevision, WorkspaceGraphToolService, thread_ref_upsert_patch,
};

use super::{
    ShellState, ShellView, SurfaceNotice,
    graph::{GraphMutationCommitUpdate, GraphMutationFailureUpdate, GraphMutationUpdate},
    graph::{GraphOptimisticMutation, OptimisticGraphMutationId},
    graph_node_action_policy::graph_node_delete_blocked_by_graph_work,
    resolve_new_thread_execution_target,
    semantic_thread_start::{SemanticThreadStartSource, start_semantic_backend_thread},
};

const UNTITLED_THREAD_LABEL: &str = "Untitled thread";

pub(super) enum GraphThreadStartUpdate {
    GraphRefPersistenceStarted(GraphOptimisticMutation),
    Finished(GraphThreadStartOutcome),
}

pub(super) struct GraphThreadStartTask {
    workspace_id: BerylWorkspaceId,
    optimistic_mutation_id: OptimisticGraphMutationId,
    receiver: Receiver<GraphThreadStartUpdate>,
}

impl GraphThreadStartTask {
    fn new(
        workspace_id: BerylWorkspaceId,
        optimistic_mutation_id: OptimisticGraphMutationId,
        receiver: Receiver<GraphThreadStartUpdate>,
    ) -> Self {
        Self {
            workspace_id,
            optimistic_mutation_id,
            receiver,
        }
    }

    pub(super) fn try_recv(&self) -> Result<GraphThreadStartUpdate, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }

    fn disconnected_failure(&self, message: &'static str) -> GraphMutationFailureUpdate {
        GraphMutationFailureUpdate::new(self.workspace_id.clone(), message)
            .with_optimistic_mutation_id(self.optimistic_mutation_id)
    }
}

pub(super) enum GraphThreadStartOutcome {
    Started {
        execution_target: WorkspaceId,
        thread: ThreadInfo,
        session_metadata: ThreadSessionMetadata,
        known_threads: Option<Vec<ThreadSummary>>,
        graph_commit: GraphMutationCommitUpdate,
    },
    Failed {
        message: String,
        graph_failure: GraphMutationFailureUpdate,
    },
}

impl ShellView {
    pub(crate) fn handle_graph_node_click(
        &mut self,
        column_index: usize,
        node_id: SemanticNodeId,
        event: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.click_count() >= 2 {
            self.start_thread_from_semantic_node(
                SemanticThreadStartSource::GraphNode,
                Some(column_index),
                node_id,
                window,
                cx,
            );
            return;
        }

        self.select_graph_node(column_index, node_id, event, window, cx);
    }

    pub(crate) fn start_checklist_item_thread_from_node(
        &mut self,
        node_id: SemanticNodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_thread_from_semantic_node(
            SemanticThreadStartSource::ChecklistItem,
            None,
            node_id,
            window,
            cx,
        );
    }

    pub(super) fn poll_graph_thread_start_updates(&mut self) -> bool {
        let Some(receiver) = self.graph_thread_start_receiver.as_ref() else {
            return false;
        };

        match receiver.try_recv() {
            Ok(GraphThreadStartUpdate::GraphRefPersistenceStarted(mutation)) => {
                if let Some(surface) = self.conversation_surface_mut() {
                    if let Err(error) = surface.begin_optimistic_graph_mutation(mutation) {
                        surface.report_optimistic_graph_mutation_failure(None, error);
                    }
                }
                true
            }
            Ok(GraphThreadStartUpdate::Finished(outcome)) => {
                self.graph_thread_start_receiver = None;
                self.finish_graph_thread_start_worker(outcome);
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                let graph_failure = receiver.disconnected_failure(
                    "Beryl lost the background task that was starting a semantic-node thread.",
                );
                self.graph_thread_start_receiver = None;
                self.handle_graph_thread_start_worker_stopped(graph_failure);
                true
            }
        }
    }

    fn start_thread_from_semantic_node(
        &mut self,
        source: SemanticThreadStartSource,
        column_index: Option<usize>,
        node_id: SemanticNodeId,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace_receiver.is_some()
            || graph_node_delete_blocked_by_graph_work(
                self.graph_receiver.is_some(),
                self.graph_thread_start_receiver.is_some(),
            )
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty()
        {
            self.set_graph_thread_start_notice(
                "Thread start unavailable",
                "Retry after the current background workspace, graph, thread, or turn operation finishes.",
            );
            cx.notify();
            return;
        }

        let Some((workspace_id, execution_target, graph, graph_revision)) =
            self.prepare_semantic_thread_start(source, column_index, &node_id)
        else {
            self.notify_checklist_sidebar_panel(cx);
            cx.notify();
            return;
        };
        self.notify_checklist_sidebar_panel(cx);

        let Some(connector) = self.backend_client_connector() else {
            self.set_graph_thread_start_notice(
                "Thread start unavailable",
                "Beryl does not have an active managed backend for this workspace.",
            );
            cx.notify();
            return;
        };

        let Some(optimistic_mutation_id) = self.conversation_surface_mut().map(|surface| {
            surface.begin_graph_mutation(source.status_message());
            surface.reserve_optimistic_graph_mutation_id()
        }) else {
            cx.notify();
            return;
        };
        let Some(persistence) = self.workspace_persistence_for_worker() else {
            self.set_graph_thread_start_notice(
                "Thread start unavailable",
                "Beryl could not open the configured workspace persistence root.",
            );
            cx.notify();
            return;
        };
        self.graph_thread_start_receiver = Some(spawn_graph_thread_start_worker(
            persistence,
            source,
            connector,
            workspace_id,
            execution_target,
            graph,
            graph_revision,
            node_id,
            optimistic_mutation_id,
            self.bootstrap.probe_timeout(),
        ));
        cx.notify();
    }

    fn prepare_semantic_thread_start(
        &mut self,
        source: SemanticThreadStartSource,
        column_index: Option<usize>,
        node_id: &SemanticNodeId,
    ) -> Option<(
        BerylWorkspaceId,
        WorkspaceId,
        SemanticGraph,
        WorkspaceGraphRevision,
    )> {
        let mut changed = false;
        let mut notice: Option<(String, String)> = None;
        let result = match &mut self.state {
            ShellState::Ready(ready) => {
                let graph = ready.surface.graph_overlay().graph().clone();
                let graph_revision = ready.surface.graph_overlay().revision();
                match graph.node(node_id) {
                    None => {
                        notice = Some((
                            "Thread start unavailable".to_string(),
                            "That semantic node is no longer available.".to_string(),
                        ));
                        None
                    }
                    Some(node) => {
                        let can_start_thread = source.can_start(node);
                        if let Some(column_index) = column_index {
                            changed |= ready.surface.select_graph_node(column_index, node_id);
                        }
                        if !can_start_thread {
                            notice = Some((
                                "Thread start unavailable".to_string(),
                                source.non_startable_detail().to_string(),
                            ));
                            None
                        } else {
                            match resolve_new_thread_execution_target(
                                &ready.loaded_workspace.workspace_state,
                                &ready.execution_target,
                            ) {
                                Ok(execution_target) => Some((
                                    ready.loaded_workspace.workspace.id().clone(),
                                    execution_target,
                                    graph,
                                    graph_revision,
                                )),
                                Err(error) => {
                                    notice = Some((
                                        "Thread start unavailable".to_string(),
                                        error.to_string(),
                                    ));
                                    None
                                }
                            }
                        }
                    }
                }
            }
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_)
            | ShellState::Blocked(_) => None,
        };

        if changed {
            self.prune_graph_scrollbar_activity();
        }
        if let Some((title, detail)) = notice {
            self.set_graph_thread_start_notice(title, detail);
        }

        result
    }

    fn finish_graph_thread_start_worker(&mut self, outcome: GraphThreadStartOutcome) {
        match outcome {
            GraphThreadStartOutcome::Started {
                execution_target,
                thread,
                session_metadata,
                known_threads,
                graph_commit,
            } => {
                let summary = thread.summary();
                if let ShellState::Ready(ready) = &mut self.state {
                    ready.execution_target = execution_target.clone();
                }
                if let Some(surface) = self.conversation_surface_mut() {
                    if let Some(known_threads) = known_threads.as_ref() {
                        surface.replace_known_threads(known_threads.clone(), &summary.id);
                    }
                    surface.load_thread_history(&thread);
                    surface.set_thread_session_metadata(session_metadata);
                }
                self.finish_graph_mutation_update(GraphMutationUpdate::Commit(graph_commit));
                if let Some(known_threads) = known_threads.as_ref() {
                    self.update_workspace_state_for_opened_target(
                        &execution_target,
                        known_threads,
                        Some(&summary.id),
                        false,
                    );
                }
                self.remember_active_thread_summary(&execution_target, &summary, true);
                self.mark_member_thread_inventory_refresh_needed();
            }
            GraphThreadStartOutcome::Failed {
                message,
                graph_failure,
            } => {
                self.finish_graph_mutation_update(GraphMutationUpdate::Failure(graph_failure));
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new("Thread start failed", message.clone()));
                }

                self.block_if_backend_process_dead(
                    "Managed backend disconnected during semantic thread start",
                    "The backend process exited before Beryl could finish starting a semantic-node thread.",
                    &message,
                );
            }
        }
    }

    fn handle_graph_thread_start_worker_stopped(
        &mut self,
        graph_failure: GraphMutationFailureUpdate,
    ) {
        let message = "Beryl lost the background task that was starting a semantic-node thread.";
        self.finish_graph_mutation_update(GraphMutationUpdate::Failure(graph_failure));
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new("Thread start failed", message));
        }
        self.block_if_backend_process_dead(
            "Semantic thread start stopped unexpectedly",
            message,
            "Beryl preserved the current workspace surface, but it cannot continue until the managed backend for this workspace is relaunched.",
        );
    }

    fn set_graph_thread_start_notice(
        &mut self,
        title: impl Into<String>,
        detail: impl Into<String>,
    ) {
        if let Some(surface) = self.conversation_surface_mut() {
            surface.set_notice(SurfaceNotice::new(title, detail));
        }
    }
}

pub(super) fn spawn_graph_thread_start_worker(
    persistence: BerylWorkspacePersistence,
    source: SemanticThreadStartSource,
    connector: ManagedBackendClientConnector,
    workspace_id: BerylWorkspaceId,
    execution_target: WorkspaceId,
    graph: SemanticGraph,
    graph_revision: WorkspaceGraphRevision,
    node_id: SemanticNodeId,
    optimistic_mutation_id: OptimisticGraphMutationId,
    timeout: Duration,
) -> GraphThreadStartTask {
    let (sender, receiver) = mpsc::channel();
    let task = GraphThreadStartTask::new(workspace_id.clone(), optimistic_mutation_id, receiver);
    thread::spawn(move || {
        let outcome = run_graph_thread_start(
            persistence,
            source,
            connector,
            workspace_id,
            execution_target,
            graph,
            graph_revision,
            node_id,
            optimistic_mutation_id,
            timeout,
            &sender,
        );
        let _ = sender.send(GraphThreadStartUpdate::Finished(outcome));
    });
    task
}

fn run_graph_thread_start(
    persistence: BerylWorkspacePersistence,
    source: SemanticThreadStartSource,
    connector: ManagedBackendClientConnector,
    workspace_id: BerylWorkspaceId,
    execution_target: WorkspaceId,
    graph: SemanticGraph,
    graph_revision: WorkspaceGraphRevision,
    node_id: SemanticNodeId,
    optimistic_mutation_id: OptimisticGraphMutationId,
    timeout: Duration,
    sender: &mpsc::Sender<GraphThreadStartUpdate>,
) -> GraphThreadStartOutcome {
    let mut session = match connector.connect_client(timeout) {
        Ok(session) => session,
        Err(error) => {
            return failed_graph_thread_start(
                &workspace_id,
                format!("Beryl could not connect to the managed backend: {error}"),
            );
        }
    };

    let started =
        match start_semantic_backend_thread(&mut session, source, &execution_target, timeout) {
            Ok(started) => started,
            Err(message) => return failed_graph_thread_start(&workspace_id, message),
        };

    let session_metadata = started.session_metadata;
    let thread = started.thread;
    let summary = thread.summary();
    let thread_id = ConversationThreadId::new(summary.id.clone());
    let request = match build_graph_started_thread_ref_request(
        source,
        &workspace_id,
        &graph,
        graph_revision,
        &node_id,
        thread_id,
        execution_target.clone(),
        summary.name.as_deref(),
    ) {
        Some(request) => request,
        None => {
            return failed_graph_thread_start(
                &workspace_id,
                format!(
                    "Beryl created Codex thread {} but could not build a valid semantic graph attachment for it.",
                    summary.id
                ),
            );
        }
    };
    let optimistic_mutation = GraphOptimisticMutation::new(
        optimistic_mutation_id,
        graph_revision,
        thread_ref_upsert_patch(&request.thread_ref, &request.provenance),
        [node_id.clone()],
        "Attaching thread to semantic graph",
    );
    let _ = sender.send(GraphThreadStartUpdate::GraphRefPersistenceStarted(
        optimistic_mutation,
    ));

    let commit = match persist_graph_started_thread_ref(&persistence, request) {
        Ok(result) => result,
        Err(error) => {
            return failed_graph_thread_start_with_optimistic_mutation(
                &workspace_id,
                format!(
                    "Beryl created Codex thread {} but could not attach it to the semantic graph: {error}",
                    summary.id
                ),
                optimistic_mutation_id,
            );
        }
    };

    let known_threads = refresh_workspace_threads(&mut session, &execution_target, timeout);
    GraphThreadStartOutcome::Started {
        execution_target,
        thread,
        session_metadata,
        known_threads,
        graph_commit: GraphMutationCommitUpdate::new(
            commit,
            "The semantic graph already contained the new thread attachment.",
        )
        .with_optimistic_mutation_id(optimistic_mutation_id),
    }
}

fn failed_graph_thread_start(
    workspace_id: &BerylWorkspaceId,
    message: impl Into<String>,
) -> GraphThreadStartOutcome {
    let message = message.into();
    GraphThreadStartOutcome::Failed {
        graph_failure: GraphMutationFailureUpdate::new(workspace_id.clone(), message.clone()),
        message,
    }
}

fn failed_graph_thread_start_with_optimistic_mutation(
    workspace_id: &BerylWorkspaceId,
    message: impl Into<String>,
    optimistic_mutation_id: OptimisticGraphMutationId,
) -> GraphThreadStartOutcome {
    let message = message.into();
    GraphThreadStartOutcome::Failed {
        graph_failure: GraphMutationFailureUpdate::new(workspace_id.clone(), message.clone())
            .with_optimistic_mutation_id(optimistic_mutation_id),
        message,
    }
}

pub(super) fn build_graph_started_thread_ref_request(
    source: SemanticThreadStartSource,
    workspace_id: &BerylWorkspaceId,
    graph: &SemanticGraph,
    graph_revision: WorkspaceGraphRevision,
    node_id: &SemanticNodeId,
    thread_id: ConversationThreadId,
    execution_target: WorkspaceId,
    title: Option<&str>,
) -> Option<ThreadRefUpsertRequest> {
    let label = thread_ref_label(title);
    let thread_ref = ThreadRefDraft::new(
        next_thread_ref_id(graph, node_id, &thread_id),
        node_id.clone(),
        thread_id,
        execution_target,
        label,
    );
    let provenance = MutationProvenance::new(
        "operator",
        current_unix_millis(),
        MutationSource::workspace_action(source.workspace_action()).ok()?,
        Some(100),
    )
    .ok()?;

    Some(ThreadRefUpsertRequest {
        workspace_id: workspace_id.clone(),
        thread_ref,
        provenance,
        expected_base_revision: Some(graph_revision),
    })
}

fn persist_graph_started_thread_ref(
    persistence: &BerylWorkspacePersistence,
    request: ThreadRefUpsertRequest,
) -> Result<WorkspaceGraphMutationCommit, String> {
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let response = service
        .upsert_thread_ref(&request)
        .map_err(|error| error.to_string())?;

    Ok(response.commit)
}

fn refresh_workspace_threads(
    session: &mut ManagedBackendSession,
    execution_target: &WorkspaceId,
    timeout: Duration,
) -> Option<Vec<ThreadSummary>> {
    let mut threads = session.list_threads(timeout).ok()?;
    threads.retain(|thread| thread.cwd == execution_target.canonical_path());
    threads.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
    });
    Some(threads)
}

fn thread_ref_label(title: Option<&str>) -> String {
    title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(UNTITLED_THREAD_LABEL)
        .to_string()
}

fn next_thread_ref_id(
    graph: &SemanticGraph,
    node_id: &SemanticNodeId,
    thread_id: &ConversationThreadId,
) -> ThreadRefId {
    let base = format!(
        "thread_ref_{}_{}",
        sanitize_id_part(node_id.as_str()),
        sanitize_id_part(thread_id.as_str())
    );
    for suffix in 0usize.. {
        let candidate = if suffix == 0 {
            base.clone()
        } else {
            format!("{base}_{suffix}")
        };
        let Ok(thread_ref_id) = ThreadRefId::new(candidate) else {
            continue;
        };
        if graph.thread_ref(&thread_ref_id).is_none() {
            return thread_ref_id;
        }
    }

    unreachable!("usize suffix space is non-empty")
}

fn sanitize_id_part(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '-' | '_' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            _ => '_',
        })
        .collect();
    if sanitized.is_empty() {
        "untitled".to_string()
    } else {
        sanitized
    }
}

fn current_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

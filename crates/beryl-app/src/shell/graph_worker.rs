use std::{
    sync::mpsc::{self, TryRecvError},
    thread,
};

use beryl_model::workspace::BerylWorkspaceId;

use crate::{
    BerylWorkspacePersistence, NodeLeafDeleteRequest, NodeSubtreeDeleteRequest,
    ThreadRefUpsertRequest, WorkspaceGraphToolService,
};

use super::graph::{GraphMutationCommitUpdate, GraphMutationUpdate, OptimisticGraphMutationId};

#[derive(Debug)]
pub(super) enum GraphUpdate {
    MutationFinished(GraphMutationUpdate),
}

pub(super) struct GraphWorkerTask {
    workspace_id: BerylWorkspaceId,
    optimistic_mutation_id: Option<OptimisticGraphMutationId>,
    receiver: mpsc::Receiver<GraphUpdate>,
}

impl GraphWorkerTask {
    fn new(
        workspace_id: BerylWorkspaceId,
        optimistic_mutation_id: Option<OptimisticGraphMutationId>,
        receiver: mpsc::Receiver<GraphUpdate>,
    ) -> Self {
        Self {
            workspace_id,
            optimistic_mutation_id,
            receiver,
        }
    }

    pub(super) fn try_recv(&self) -> Result<GraphUpdate, TryRecvError> {
        self.receiver.try_recv()
    }

    pub(super) fn disconnected_update(&self, message: &'static str) -> GraphMutationUpdate {
        match self.optimistic_mutation_id {
            Some(mutation_id) => GraphMutationUpdate::optimistic_failure(
                self.workspace_id.clone(),
                message,
                mutation_id,
            ),
            None => GraphMutationUpdate::failure(self.workspace_id.clone(), message),
        }
    }
}

pub(super) fn spawn_thread_ref_link_worker(
    persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    request: ThreadRefUpsertRequest,
    optimistic_mutation_id: Option<OptimisticGraphMutationId>,
) -> GraphWorkerTask {
    let (sender, receiver) = mpsc::channel();
    let task = GraphWorkerTask::new(workspace_id.clone(), optimistic_mutation_id, receiver);
    thread::spawn(move || {
        let update = graph_worker_update(
            workspace_id,
            optimistic_mutation_id,
            run_thread_ref_link(&persistence, &request),
        );
        let _ = sender.send(GraphUpdate::MutationFinished(update));
    });
    task
}

pub(super) fn spawn_node_subtree_delete_worker(
    persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    request: NodeSubtreeDeleteRequest,
    optimistic_mutation_id: Option<OptimisticGraphMutationId>,
) -> GraphWorkerTask {
    let (sender, receiver) = mpsc::channel();
    let task = GraphWorkerTask::new(workspace_id.clone(), optimistic_mutation_id, receiver);
    thread::spawn(move || {
        let update = graph_worker_update(
            workspace_id,
            optimistic_mutation_id,
            run_node_subtree_delete(&persistence, &request),
        );
        let _ = sender.send(GraphUpdate::MutationFinished(update));
    });
    task
}

pub(super) fn spawn_node_leaf_delete_worker(
    persistence: BerylWorkspacePersistence,
    workspace_id: BerylWorkspaceId,
    request: NodeLeafDeleteRequest,
    optimistic_mutation_id: Option<OptimisticGraphMutationId>,
) -> GraphWorkerTask {
    let (sender, receiver) = mpsc::channel();
    let task = GraphWorkerTask::new(workspace_id.clone(), optimistic_mutation_id, receiver);
    thread::spawn(move || {
        let update = graph_worker_update(
            workspace_id,
            optimistic_mutation_id,
            run_node_leaf_delete(&persistence, &request),
        );
        let _ = sender.send(GraphUpdate::MutationFinished(update));
    });
    task
}

fn graph_worker_update(
    workspace_id: BerylWorkspaceId,
    optimistic_mutation_id: Option<OptimisticGraphMutationId>,
    result: Result<GraphMutationCommitUpdate, String>,
) -> GraphMutationUpdate {
    match result {
        Ok(update) => match optimistic_mutation_id {
            Some(mutation_id) => GraphMutationUpdate::optimistic_commit(
                update.commit,
                update.no_op_message,
                mutation_id,
            ),
            None => GraphMutationUpdate::commit(update.commit, update.no_op_message),
        },
        Err(error) => match optimistic_mutation_id {
            Some(mutation_id) => {
                GraphMutationUpdate::optimistic_failure(workspace_id, error, mutation_id)
            }
            None => GraphMutationUpdate::failure(workspace_id, error),
        },
    }
}

fn run_thread_ref_link(
    persistence: &BerylWorkspacePersistence,
    request: &ThreadRefUpsertRequest,
) -> Result<GraphMutationCommitUpdate, String> {
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let response = service
        .upsert_thread_ref(request)
        .map_err(|error| error.to_string())?;

    Ok(GraphMutationCommitUpdate::new(
        response.commit,
        "That thread was already linked to the selected semantic node.",
    ))
}

fn run_node_leaf_delete(
    persistence: &BerylWorkspacePersistence,
    request: &NodeLeafDeleteRequest,
) -> Result<GraphMutationCommitUpdate, String> {
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let response = service
        .delete_node_leaf(request)
        .map_err(|error| error.to_string())?;

    Ok(GraphMutationCommitUpdate::new(
        response.commit,
        "The selected semantic node was already deleted.",
    ))
}

fn run_node_subtree_delete(
    persistence: &BerylWorkspacePersistence,
    request: &NodeSubtreeDeleteRequest,
) -> Result<GraphMutationCommitUpdate, String> {
    let service = WorkspaceGraphToolService::new(persistence.clone());
    let response = service
        .delete_node_subtree(request)
        .map_err(|error| error.to_string())?;

    Ok(GraphMutationCommitUpdate::new(
        response.commit,
        "The selected semantic node was already deleted.",
    ))
}

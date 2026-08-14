use beryl_model::semantic_graph::{SemanticGraph, SemanticNodeId, SoftLinkId};

use crate::{WorkspaceGraphMutationCommit, WorkspaceGraphRevision};

use super::column_selector::{ColumnSelectorColumn, ColumnSelectorState};

#[path = "graph/mutation.rs"]
mod mutation;

#[allow(unused_imports)]
pub(crate) use mutation::{
    GraphCommitApplication, GraphCommitProjectionError, GraphMutationCommitUpdate,
    GraphMutationFailureUpdate, GraphMutationUpdate, GraphOptimisticMutation,
    GraphOptimisticProjectionError, OptimisticGraphMutationId,
};
use mutation::{GraphMutationCoordinatorState, StagedGraphCommit};

pub(crate) const DEFAULT_GRAPH_COLUMN_EXPANDED_DEPTH: usize = 2;
const GRAPH_COLUMN_EXPANSION_OVERRIDE_MAX: usize = 512;

#[derive(Clone, Debug, Default)]
pub(crate) struct GraphOverlayState {
    committed_graph: SemanticGraph,
    graph: SemanticGraph,
    mutation_coordinator: GraphMutationCoordinatorState,
    visible: bool,
    mutation_status: Option<GraphOverlayMutationStatus>,
    last_error: Option<String>,
    columns: ColumnSelectorState<GraphColumnKey, GraphColumnSelection, SemanticNodeId>,
}

pub(crate) type GraphColumnState =
    ColumnSelectorColumn<GraphColumnKey, GraphColumnSelection, SemanticNodeId>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GraphRetainedCounts {
    pub(crate) visible_nodes: usize,
    pub(crate) visible_soft_links: usize,
    pub(crate) visible_thread_refs: usize,
    pub(crate) committed_nodes: usize,
    pub(crate) committed_soft_links: usize,
    pub(crate) committed_thread_refs: usize,
    pub(crate) columns: usize,
    pub(crate) pending_optimistic_mutations: usize,
    pub(crate) queued_commits: usize,
    pub(crate) payload_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GraphColumnKey {
    RootLevel,
    Node(SemanticNodeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphOverlayMutationStatus {
    message: String,
}

impl GraphOverlayMutationStatus {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn message(&self) -> &str {
        self.message.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GraphColumnSelection {
    Node(SemanticNodeId),
    SoftLink {
        link_id: SoftLinkId,
        target_node_id: SemanticNodeId,
    },
}

fn semantic_graph_payload_counts(graph: &SemanticGraph) -> usize {
    let node_bytes = graph
        .nodes()
        .map(|node| {
            node.id()
                .as_str()
                .len()
                .saturating_add(node.title().len())
                .saturating_add(node.summary().len())
        })
        .sum::<usize>();
    let soft_link_bytes = graph
        .soft_links()
        .map(|link| {
            link.id()
                .as_str()
                .len()
                .saturating_add(link.source_id().as_str().len())
                .saturating_add(link.target_id().as_str().len())
                .saturating_add(link.kind().as_str().len())
        })
        .sum::<usize>();
    let thread_ref_bytes = graph
        .thread_refs()
        .map(|thread_ref| {
            thread_ref
                .id()
                .as_str()
                .len()
                .saturating_add(thread_ref.node_id().as_str().len())
                .saturating_add(thread_ref.thread_id().as_str().len())
                .saturating_add(thread_ref.execution_target().display_label().len())
                .saturating_add(thread_ref.label().len())
        })
        .sum::<usize>();
    node_bytes
        .saturating_add(soft_link_bytes)
        .saturating_add(thread_ref_bytes)
}

impl GraphOverlayState {
    pub(crate) fn new(
        graph: SemanticGraph,
        revision: WorkspaceGraphRevision,
        warning: Option<String>,
    ) -> Self {
        let mut state = Self {
            committed_graph: graph.clone(),
            graph,
            mutation_coordinator: GraphMutationCoordinatorState::new(revision),
            visible: false,
            mutation_status: None,
            last_error: warning,
            columns: ColumnSelectorState::new(),
        };
        state.reconcile_columns();
        state
    }

    pub(crate) fn graph(&self) -> &SemanticGraph {
        &self.graph
    }

    pub(crate) fn retained_counts(&self) -> GraphRetainedCounts {
        let visible = semantic_graph_payload_counts(&self.graph);
        let committed = semantic_graph_payload_counts(&self.committed_graph);
        GraphRetainedCounts {
            visible_nodes: self.graph.node_count(),
            visible_soft_links: self.graph.soft_link_count(),
            visible_thread_refs: self.graph.thread_ref_count(),
            committed_nodes: self.committed_graph.node_count(),
            committed_soft_links: self.committed_graph.soft_link_count(),
            committed_thread_refs: self.committed_graph.thread_ref_count(),
            columns: self.columns().len(),
            pending_optimistic_mutations: self
                .mutation_coordinator
                .pending_optimistic_mutation_count(),
            queued_commits: self.mutation_coordinator.queued_commit_count(),
            payload_bytes: visible.saturating_add(committed),
        }
    }

    pub(crate) fn revision(&self) -> WorkspaceGraphRevision {
        self.mutation_coordinator.committed_revision()
    }

    pub(crate) fn reserve_optimistic_mutation_id(&mut self) -> OptimisticGraphMutationId {
        self.mutation_coordinator.reserve_optimistic_mutation_id()
    }

    pub(crate) fn node_has_pending_optimistic_mutation(&self, node_id: &SemanticNodeId) -> bool {
        self.mutation_coordinator
            .node_has_pending_optimistic_mutation(node_id)
    }

    #[cfg(test)]
    pub(crate) fn queued_commit_count(&self) -> usize {
        self.mutation_coordinator.queued_commit_count()
    }

    #[cfg(test)]
    pub(crate) fn pending_optimistic_mutation_count(&self) -> usize {
        self.mutation_coordinator
            .pending_optimistic_mutation_count()
    }

    pub(crate) fn visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn graph_columns_available(&self) -> bool {
        !self.graph.root_node_ids().is_empty()
    }

    pub(crate) fn mutation_pending(&self) -> bool {
        self.mutation_status.is_some()
            || self.mutation_coordinator.has_pending_optimistic_mutations()
    }

    pub(crate) fn status_message(&self) -> Option<&str> {
        self.mutation_status
            .as_ref()
            .map(GraphOverlayMutationStatus::message)
    }

    pub(crate) fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub(crate) fn columns(&self) -> &[GraphColumnState] {
        self.columns.columns()
    }

    pub(crate) fn selected_node_id(&self) -> Option<&SemanticNodeId> {
        self.columns().iter().rev().find_map(|column| {
            column
                .selection()
                .and_then(GraphColumnSelection::target_node_id)
        })
    }

    pub(crate) fn toggle_visibility(&mut self) -> bool {
        self.visible = !self.visible;
        if self.visible {
            self.reconcile_columns();
        }
        self.visible
    }

    pub(crate) fn close(&mut self) -> bool {
        let was_visible = self.visible;
        self.visible = false;
        was_visible
    }

    pub(crate) fn begin_mutation(&mut self, status_message: impl Into<String>) {
        self.mutation_status = Some(GraphOverlayMutationStatus::new(status_message));
        self.last_error = None;
    }

    pub(crate) fn begin_optimistic_mutation(
        &mut self,
        mutation: GraphOptimisticMutation,
    ) -> Result<(), GraphOptimisticProjectionError> {
        if mutation.base_revision() != self.mutation_coordinator.committed_revision() {
            return Err(GraphOptimisticProjectionError::StaleBaseRevision {
                mutation_id: mutation.id(),
                expected: self.mutation_coordinator.committed_revision(),
                actual: mutation.base_revision(),
            });
        }

        let mut graph = self.graph.clone();
        graph.apply_patch(mutation.patch()).map_err(|error| {
            GraphOptimisticProjectionError::ApplyPatch {
                mutation_id: mutation.id(),
                error,
            }
        })?;

        self.graph = graph;
        self.mutation_status = Some(GraphOverlayMutationStatus::new(
            mutation.status_message().to_string(),
        ));
        self.last_error = None;
        let pruned_pending = self
            .mutation_coordinator
            .push_pending_optimistic_mutation(mutation.into_pending());
        if pruned_pending {
            self.rebuild_visible_graph_from_committed()?;
        }
        self.reconcile_columns();
        Ok(())
    }

    pub(crate) fn finish_mutation(
        &mut self,
        graph: SemanticGraph,
        revision: WorkspaceGraphRevision,
        warning: Option<String>,
    ) {
        self.committed_graph = graph.clone();
        self.graph = graph;
        self.mutation_coordinator.reset(revision);
        self.mutation_status = None;
        self.last_error = warning;
        self.reconcile_columns();
    }

    pub(crate) fn finish_mutation_commit_update(
        &mut self,
        update: GraphMutationCommitUpdate,
    ) -> Result<GraphCommitApplication, GraphCommitProjectionError> {
        let application = self.apply_commit_update(update)?;
        match &application {
            GraphCommitApplication::Applied { warning, .. } => {
                if !self.mutation_coordinator.has_pending_optimistic_mutations() {
                    self.mutation_status = None;
                }
                self.last_error = warning.clone();
                self.reconcile_columns();
            }
            GraphCommitApplication::IgnoredStale { .. } => {
                if !self.mutation_coordinator.has_pending_optimistic_mutations() {
                    self.mutation_status = None;
                }
            }
            GraphCommitApplication::QueuedGap {
                queued_revision,
                waiting_for_revision,
            } => {
                self.mutation_status = Some(GraphOverlayMutationStatus::new(format!(
                    "Waiting for semantic graph revision {waiting_for_revision} before applying revision {queued_revision}."
                )));
            }
            GraphCommitApplication::RecoveryRequired { reason } => {
                self.graph = self.committed_graph.clone();
                self.mutation_status = Some(GraphOverlayMutationStatus::new(
                    "Recovering semantic graph projection from persisted state.",
                ));
                self.last_error = Some(reason.clone());
                self.reconcile_columns();
            }
        }
        Ok(application)
    }

    fn apply_commit_update(
        &mut self,
        update: GraphMutationCommitUpdate,
    ) -> Result<GraphCommitApplication, GraphCommitProjectionError> {
        match self.mutation_coordinator.stage_commit(update)? {
            StagedGraphCommit::Apply(update) => self.apply_ready_commits(update),
            StagedGraphCommit::QueuedGap {
                queued_revision,
                waiting_for_revision,
            } => Ok(GraphCommitApplication::QueuedGap {
                queued_revision,
                waiting_for_revision,
            }),
            StagedGraphCommit::RecoveryRequired { reason } => {
                Ok(GraphCommitApplication::RecoveryRequired { reason })
            }
            StagedGraphCommit::IgnoredStale {
                committed_revision,
                visible_revision,
            } => Ok(GraphCommitApplication::IgnoredStale {
                committed_revision,
                visible_revision,
            }),
        }
    }

    fn apply_ready_commits(
        &mut self,
        first_update: GraphMutationCommitUpdate,
    ) -> Result<GraphCommitApplication, GraphCommitProjectionError> {
        let mut next_update = Some(first_update);
        let mut graph_changed = false;
        let mut latest_manifest = None;
        let mut warning = None;
        let mut applied_revisions = Vec::new();

        while let Some(update) = next_update.take() {
            let GraphMutationCommitUpdate {
                commit,
                no_op_message,
                optimistic_mutation_id,
            } = update;
            self.apply_ready_commit(&commit)?;
            if let Some(mutation_id) = optimistic_mutation_id {
                self.mutation_coordinator
                    .remove_pending_optimistic_mutation(mutation_id);
            }
            graph_changed |= commit.changed;
            warning = (!commit.changed && !no_op_message.is_empty()).then_some(no_op_message);
            latest_manifest = Some(commit.manifest);
            applied_revisions.push(commit.committed_revision);
            self.mutation_coordinator
                .mark_committed(commit.committed_revision);
            next_update = self.mutation_coordinator.take_next_ready_commit();
        }

        let mut projection_warning = None;
        if let Err(error) = self.rebuild_visible_graph_from_committed() {
            self.mutation_coordinator
                .clear_pending_optimistic_mutations();
            self.graph = self.committed_graph.clone();
            projection_warning = Some(error.to_string());
        }

        if graph_changed {
            warning = None;
        }
        if projection_warning.is_some() {
            warning = projection_warning;
        }

        let latest_manifest =
            latest_manifest.expect("at least one staged graph commit must be applied");
        Ok(GraphCommitApplication::Applied {
            latest_manifest,
            graph_changed,
            warning,
            applied_revisions,
        })
    }

    fn apply_ready_commit(
        &mut self,
        commit: &WorkspaceGraphMutationCommit,
    ) -> Result<(), GraphCommitProjectionError> {
        if commit.base_revision != self.mutation_coordinator.committed_revision() {
            return Err(GraphCommitProjectionError::ConflictingRevision {
                visible: self.mutation_coordinator.committed_revision(),
                base: commit.base_revision,
                committed: commit.committed_revision,
            });
        }

        let mut graph = self.committed_graph.clone();
        graph
            .apply_patch(&commit.patch)
            .map_err(GraphCommitProjectionError::ApplyPatch)?;
        self.committed_graph = graph;
        Ok(())
    }

    pub(crate) fn fail_mutation(&mut self, error: impl Into<String>) {
        self.mutation_status = None;
        self.last_error = Some(error.into());
    }

    pub(crate) fn fail_optimistic_mutation(
        &mut self,
        mutation_id: Option<OptimisticGraphMutationId>,
        error: impl Into<String>,
    ) -> Result<(), GraphOptimisticProjectionError> {
        if let Some(mutation_id) = mutation_id {
            self.mutation_coordinator
                .remove_pending_optimistic_mutation(mutation_id);
            self.rebuild_visible_graph_from_committed()?;
        }
        self.mutation_status = None;
        self.last_error = Some(error.into());
        self.reconcile_columns();
        Ok(())
    }

    pub(crate) fn select_node(&mut self, column_index: usize, node_id: &SemanticNodeId) -> bool {
        if self.graph.node(node_id).is_none() || column_index >= self.columns.len() {
            return false;
        }

        let column_key = self.columns()[column_index].root_key().clone();
        let next_selection = GraphColumnSelection::Node(node_id.clone());
        let next_root = match column_key {
            GraphColumnKey::RootLevel => Some(GraphColumnKey::Node(node_id.clone())),
            GraphColumnKey::Node(root_id) => {
                (root_id != *node_id).then(|| GraphColumnKey::Node(node_id.clone()))
            }
        };
        self.columns
            .select_row(column_index, next_selection, next_root)
    }

    pub(crate) fn select_soft_link(
        &mut self,
        column_index: usize,
        link_id: &SoftLinkId,
        target_node_id: &SemanticNodeId,
    ) -> bool {
        if self.graph.soft_link(link_id).is_none()
            || self.graph.node(target_node_id).is_none()
            || column_index >= self.columns.len()
        {
            return false;
        }

        let next_selection = GraphColumnSelection::SoftLink {
            link_id: link_id.clone(),
            target_node_id: target_node_id.clone(),
        };
        self.columns.select_row(
            column_index,
            next_selection,
            Some(GraphColumnKey::Node(target_node_id.clone())),
        )
    }

    pub(crate) fn toggle_node_expansion(
        &mut self,
        column_index: usize,
        node_id: &SemanticNodeId,
        depth: usize,
    ) -> bool {
        if column_index >= self.columns.len() {
            return false;
        }

        let has_children = !self.graph.child_nodes_of(node_id).is_empty();
        let has_soft_links = self.graph.soft_links_from(node_id).next().is_some();
        let has_thread_refs = self.graph.thread_refs_for_node(node_id).next().is_some();
        if !has_children && !has_soft_links && !has_thread_refs {
            return false;
        }

        self.columns
            .columns_mut()
            .get_mut(column_index)
            .is_some_and(|column| {
                column.toggle_expansion(node_id, graph_node_default_expanded(depth))
            })
            && {
                self.prune_expansion_overrides();
                true
            }
    }

    fn reconcile_columns(&mut self) {
        let relocated_selected_node_id = self.relocated_selected_node_id();
        for column in self.columns.columns_mut() {
            reconcile_graph_column(column, &self.graph);
        }
        self.columns
            .retain_columns(|column_key| column_key.is_valid_for_graph(&self.graph));

        if self.columns.is_empty() {
            if self.graph_columns_available() {
                self.columns = ColumnSelectorState::from_root(GraphColumnKey::RootLevel);
            }
            return;
        }

        if self.columns()[0].root_key() != &GraphColumnKey::RootLevel {
            self.columns = ColumnSelectorState::from_root(GraphColumnKey::RootLevel);
            return;
        }

        let mut keep = 1usize;
        for index in 0..self.columns.len().saturating_sub(1) {
            let Some(expected_root) = self.columns()[index]
                .selection()
                .and_then(GraphColumnSelection::target_node_id)
                .map(|node_id| GraphColumnKey::Node(node_id.clone()))
            else {
                break;
            };

            if self.columns()[index + 1].root_key() != &expected_root {
                break;
            }

            keep += 1;
        }
        self.columns.truncate_columns(keep);

        if let Some(node_id) = relocated_selected_node_id {
            self.rebuild_columns_to_node(&node_id);
        }
        self.prune_expansion_overrides();
    }

    fn prune_expansion_overrides(&mut self) -> bool {
        self.columns
            .prune_expansion_overrides(GRAPH_COLUMN_EXPANSION_OVERRIDE_MAX)
    }

    fn rebuild_visible_graph_from_committed(
        &mut self,
    ) -> Result<(), GraphOptimisticProjectionError> {
        let mut graph = self.committed_graph.clone();
        let committed_revision = self.mutation_coordinator.committed_revision();
        for pending in self.mutation_coordinator.pending_optimistic_mutations() {
            debug_assert!(pending.base_revision <= committed_revision);
            graph.apply_patch(&pending.patch).map_err(|error| {
                GraphOptimisticProjectionError::ReplayPendingPatch {
                    mutation_id: pending.id,
                    error,
                }
            })?;
        }
        self.graph = graph;
        Ok(())
    }

    fn relocated_selected_node_id(&self) -> Option<SemanticNodeId> {
        self.columns().iter().rev().find_map(|column| {
            column.selection().and_then(|selection| {
                selection.relocated_target_node_id(column.root_key(), &self.graph)
            })
        })
    }

    fn rebuild_columns_to_node(&mut self, node_id: &SemanticNodeId) {
        let Some(path) = self.graph.path_to_root(node_id) else {
            return;
        };
        let path_ids = path
            .iter()
            .map(|node| node.id().clone())
            .collect::<Vec<_>>();
        if path_ids.is_empty() {
            return;
        }

        let mut trail = Vec::with_capacity(path_ids.len() + 1);
        trail.push((
            GraphColumnKey::RootLevel,
            Some(GraphColumnSelection::Node(path_ids[0].clone())),
        ));

        for (index, node_id) in path_ids.iter().enumerate() {
            let selection = path_ids
                .get(index + 1)
                .map(|next_id| GraphColumnSelection::Node(next_id.clone()));
            trail.push((GraphColumnKey::Node(node_id.clone()), selection));
        }

        self.columns.replace_trail_preserving_expansion(trail);
    }
}

fn graph_node_default_expanded(depth: usize) -> bool {
    depth < DEFAULT_GRAPH_COLUMN_EXPANDED_DEPTH
}

fn reconcile_graph_column(column: &mut GraphColumnState, graph: &SemanticGraph) {
    if let Some(selection) = column.selection()
        && !selection.is_valid_for_graph(graph)
    {
        column.clear_selection();
    }
    column.retain_expansion_overrides(|node_id| graph.node(node_id).is_some());
}

impl GraphColumnKey {
    pub(crate) fn renders_fixed_header(&self) -> bool {
        true
    }

    fn is_valid_for_graph(&self, graph: &SemanticGraph) -> bool {
        match self {
            Self::RootLevel => !graph.root_node_ids().is_empty(),
            Self::Node(node_id) => graph.node(node_id).is_some(),
        }
    }
}

impl GraphColumnSelection {
    pub(crate) fn target_node_id(&self) -> Option<&SemanticNodeId> {
        match self {
            Self::Node(node_id) => Some(node_id),
            Self::SoftLink { target_node_id, .. } => Some(target_node_id),
        }
    }

    fn is_valid_for_graph(&self, graph: &SemanticGraph) -> bool {
        match self {
            Self::Node(node_id) => graph.node(node_id).is_some(),
            Self::SoftLink {
                link_id,
                target_node_id,
            } => graph.soft_link(link_id).is_some_and(|link| {
                link.target_id() == target_node_id && graph.node(target_node_id).is_some()
            }),
        }
    }

    fn relocated_target_node_id(
        &self,
        root_key: &GraphColumnKey,
        graph: &SemanticGraph,
    ) -> Option<SemanticNodeId> {
        match self {
            Self::Node(node_id) => (graph.node(node_id).is_some()
                && !node_is_in_column_scope(root_key, graph, node_id))
            .then(|| node_id.clone()),
            Self::SoftLink {
                link_id,
                target_node_id,
            } => graph.soft_link(link_id).and_then(|link| {
                (link.target_id() == target_node_id
                    && graph.node(target_node_id).is_some()
                    && !node_is_in_column_scope(root_key, graph, link.source_id()))
                .then(|| target_node_id.clone())
            }),
        }
    }
}

fn node_is_in_column_scope(
    root_key: &GraphColumnKey,
    graph: &SemanticGraph,
    node_id: &SemanticNodeId,
) -> bool {
    match root_key {
        GraphColumnKey::RootLevel => graph.node(node_id).is_some(),
        GraphColumnKey::Node(root_id) => graph
            .path_to_root(node_id)
            .is_some_and(|path| path.iter().any(|ancestor| ancestor.id() == root_id)),
    }
}

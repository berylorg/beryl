use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::{Duration, Instant},
};

use beryl_model::{
    semantic_graph::{SemanticGraphError, SemanticGraphPatch, SemanticNodeId},
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest},
};

use crate::{WorkspaceGraphMutationCommit, WorkspaceGraphRevision};

const GRAPH_MUTATION_QUEUED_COMMIT_MAX: usize = 256;
const GRAPH_MUTATION_QUEUED_COMMIT_BYTE_LIMIT: usize = 4 * 1024 * 1024;
const GRAPH_MUTATION_PENDING_OPTIMISTIC_MAX: usize = 256;
const GRAPH_MUTATION_PENDING_OPTIMISTIC_BYTE_LIMIT: usize = 4 * 1024 * 1024;
const GRAPH_MUTATION_QUEUE_MAX_AGE: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default)]
pub(super) struct GraphMutationCoordinatorState {
    committed_revision: WorkspaceGraphRevision,
    queued_commits: BTreeMap<WorkspaceGraphRevision, QueuedGraphCommit>,
    pending_optimistic_mutations: VecDeque<PendingOptimisticGraphMutation>,
    next_optimistic_mutation_id: u64,
}

#[derive(Clone, Debug)]
struct QueuedGraphCommit {
    update: GraphMutationCommitUpdate,
    queued_at: Instant,
    retained_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingOptimisticGraphMutation {
    pub(super) id: OptimisticGraphMutationId,
    pub(super) base_revision: WorkspaceGraphRevision,
    pub(super) patch: SemanticGraphPatch,
    pub(super) affected_node_ids: BTreeSet<SemanticNodeId>,
    queued_at: Instant,
    retained_bytes: usize,
}

pub(super) enum StagedGraphCommit {
    Apply(GraphMutationCommitUpdate),
    QueuedGap {
        queued_revision: WorkspaceGraphRevision,
        waiting_for_revision: WorkspaceGraphRevision,
    },
    RecoveryRequired {
        reason: String,
    },
    IgnoredStale {
        committed_revision: WorkspaceGraphRevision,
        visible_revision: WorkspaceGraphRevision,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GraphMutationUpdate {
    Commit(GraphMutationCommitUpdate),
    Failure(GraphMutationFailureUpdate),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphMutationCommitUpdate {
    pub(crate) commit: WorkspaceGraphMutationCommit,
    pub(crate) no_op_message: String,
    pub(crate) optimistic_mutation_id: Option<OptimisticGraphMutationId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphMutationFailureUpdate {
    pub(crate) workspace_id: BerylWorkspaceId,
    pub(crate) message: String,
    pub(crate) optimistic_mutation_id: Option<OptimisticGraphMutationId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OptimisticGraphMutationId(u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphOptimisticMutation {
    id: OptimisticGraphMutationId,
    base_revision: WorkspaceGraphRevision,
    patch: SemanticGraphPatch,
    affected_node_ids: BTreeSet<SemanticNodeId>,
    status_message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GraphCommitApplication {
    Applied {
        latest_manifest: BerylWorkspaceManifest,
        graph_changed: bool,
        warning: Option<String>,
        applied_revisions: Vec<WorkspaceGraphRevision>,
    },
    QueuedGap {
        queued_revision: WorkspaceGraphRevision,
        waiting_for_revision: WorkspaceGraphRevision,
    },
    RecoveryRequired {
        reason: String,
    },
    IgnoredStale {
        committed_revision: WorkspaceGraphRevision,
        visible_revision: WorkspaceGraphRevision,
    },
}

impl GraphMutationCommitUpdate {
    pub(crate) fn new(
        commit: WorkspaceGraphMutationCommit,
        no_op_message: impl Into<String>,
    ) -> Self {
        Self {
            commit,
            no_op_message: no_op_message.into(),
            optimistic_mutation_id: None,
        }
    }

    pub(crate) fn with_optimistic_mutation_id(
        mut self,
        mutation_id: OptimisticGraphMutationId,
    ) -> Self {
        self.optimistic_mutation_id = Some(mutation_id);
        self
    }

    pub(crate) fn workspace_id(&self) -> &BerylWorkspaceId {
        &self.commit.workspace_id
    }
}

impl GraphMutationFailureUpdate {
    pub(crate) fn new(workspace_id: BerylWorkspaceId, message: impl Into<String>) -> Self {
        Self {
            workspace_id,
            message: message.into(),
            optimistic_mutation_id: None,
        }
    }

    pub(crate) fn with_optimistic_mutation_id(
        mut self,
        mutation_id: OptimisticGraphMutationId,
    ) -> Self {
        self.optimistic_mutation_id = Some(mutation_id);
        self
    }
}

impl GraphMutationUpdate {
    pub(crate) fn commit(
        commit: WorkspaceGraphMutationCommit,
        no_op_message: impl Into<String>,
    ) -> Self {
        Self::Commit(GraphMutationCommitUpdate::new(commit, no_op_message))
    }

    pub(crate) fn failure(workspace_id: BerylWorkspaceId, message: impl Into<String>) -> Self {
        Self::Failure(GraphMutationFailureUpdate::new(workspace_id, message))
    }

    pub(crate) fn optimistic_commit(
        commit: WorkspaceGraphMutationCommit,
        no_op_message: impl Into<String>,
        mutation_id: OptimisticGraphMutationId,
    ) -> Self {
        Self::Commit(
            GraphMutationCommitUpdate::new(commit, no_op_message)
                .with_optimistic_mutation_id(mutation_id),
        )
    }

    pub(crate) fn optimistic_failure(
        workspace_id: BerylWorkspaceId,
        message: impl Into<String>,
        mutation_id: OptimisticGraphMutationId,
    ) -> Self {
        Self::Failure(
            GraphMutationFailureUpdate::new(workspace_id, message)
                .with_optimistic_mutation_id(mutation_id),
        )
    }

    pub(crate) fn workspace_id(&self) -> &BerylWorkspaceId {
        match self {
            Self::Commit(update) => update.workspace_id(),
            Self::Failure(update) => &update.workspace_id,
        }
    }
}

impl OptimisticGraphMutationId {
    pub(crate) fn value(self) -> u64 {
        self.0
    }
}

impl GraphOptimisticMutation {
    pub(crate) fn new(
        id: OptimisticGraphMutationId,
        base_revision: WorkspaceGraphRevision,
        patch: SemanticGraphPatch,
        affected_node_ids: impl IntoIterator<Item = SemanticNodeId>,
        status_message: impl Into<String>,
    ) -> Self {
        Self {
            id,
            base_revision,
            patch,
            affected_node_ids: affected_node_ids.into_iter().collect(),
            status_message: status_message.into(),
        }
    }

    pub(crate) fn id(&self) -> OptimisticGraphMutationId {
        self.id
    }

    pub(crate) fn base_revision(&self) -> WorkspaceGraphRevision {
        self.base_revision
    }

    pub(super) fn status_message(&self) -> &str {
        &self.status_message
    }

    pub(super) fn patch(&self) -> &SemanticGraphPatch {
        &self.patch
    }

    pub(super) fn into_pending(self) -> PendingOptimisticGraphMutation {
        PendingOptimisticGraphMutation {
            id: self.id,
            base_revision: self.base_revision,
            patch: self.patch,
            affected_node_ids: self.affected_node_ids,
            queued_at: Instant::now(),
            retained_bytes: 0,
        }
    }
}

impl QueuedGraphCommit {
    fn new(update: GraphMutationCommitUpdate) -> Self {
        let retained_bytes = graph_commit_update_retained_bytes(&update);
        Self {
            update,
            queued_at: Instant::now(),
            retained_bytes,
        }
    }
}

impl GraphMutationCoordinatorState {
    pub(super) fn new(committed_revision: WorkspaceGraphRevision) -> Self {
        Self {
            committed_revision,
            queued_commits: BTreeMap::new(),
            pending_optimistic_mutations: VecDeque::new(),
            next_optimistic_mutation_id: 1,
        }
    }

    pub(super) fn committed_revision(&self) -> WorkspaceGraphRevision {
        self.committed_revision
    }

    pub(super) fn queued_commit_count(&self) -> usize {
        self.queued_commits.len()
    }

    #[cfg(test)]
    pub(super) fn queued_commit_payload_bytes(&self) -> usize {
        self.queued_commit_retained_bytes()
    }

    pub(super) fn pending_optimistic_mutation_count(&self) -> usize {
        self.pending_optimistic_mutations.len()
    }

    #[cfg(test)]
    pub(super) fn pending_optimistic_mutation_payload_bytes(&self) -> usize {
        self.pending_optimistic_mutation_retained_bytes()
    }

    pub(super) fn reserve_optimistic_mutation_id(&mut self) -> OptimisticGraphMutationId {
        let id = OptimisticGraphMutationId(self.next_optimistic_mutation_id);
        self.next_optimistic_mutation_id = self.next_optimistic_mutation_id.saturating_add(1);
        id
    }

    pub(super) fn node_has_pending_optimistic_mutation(&self, node_id: &SemanticNodeId) -> bool {
        self.pending_optimistic_mutations
            .iter()
            .any(|pending| pending.affected_node_ids.contains(node_id))
    }

    pub(super) fn has_pending_optimistic_mutations(&self) -> bool {
        !self.pending_optimistic_mutations.is_empty()
    }

    pub(super) fn pending_optimistic_mutations(
        &self,
    ) -> impl Iterator<Item = &PendingOptimisticGraphMutation> {
        self.pending_optimistic_mutations.iter()
    }

    pub(super) fn push_pending_optimistic_mutation(
        &mut self,
        mut mutation: PendingOptimisticGraphMutation,
    ) -> bool {
        mutation.queued_at = Instant::now();
        mutation.retained_bytes = pending_optimistic_mutation_retained_bytes(&mutation);
        self.pending_optimistic_mutations.push_back(mutation);
        self.prune_pending_optimistic_mutations()
    }

    pub(super) fn remove_pending_optimistic_mutation(
        &mut self,
        mutation_id: OptimisticGraphMutationId,
    ) -> bool {
        let Some(index) = self
            .pending_optimistic_mutations
            .iter()
            .position(|pending| pending.id == mutation_id)
        else {
            return false;
        };
        self.pending_optimistic_mutations.remove(index);
        true
    }

    pub(super) fn clear_pending_optimistic_mutations(&mut self) {
        self.pending_optimistic_mutations.clear();
    }

    pub(super) fn reset(&mut self, committed_revision: WorkspaceGraphRevision) {
        self.committed_revision = committed_revision;
        self.queued_commits.clear();
        self.pending_optimistic_mutations.clear();
    }

    pub(super) fn stage_commit(
        &mut self,
        update: GraphMutationCommitUpdate,
    ) -> Result<StagedGraphCommit, GraphCommitProjectionError> {
        let commit = &update.commit;
        if commit.committed_revision <= commit.base_revision {
            return Err(GraphCommitProjectionError::InvalidRevisionOrder {
                base: commit.base_revision,
                committed: commit.committed_revision,
            });
        }

        if commit.committed_revision <= self.committed_revision {
            return Ok(StagedGraphCommit::IgnoredStale {
                committed_revision: commit.committed_revision,
                visible_revision: self.committed_revision,
            });
        }

        if commit.base_revision == self.committed_revision {
            return Ok(StagedGraphCommit::Apply(update));
        }

        if commit.base_revision > self.committed_revision {
            if self.drop_expired_queued_commits() {
                return Ok(StagedGraphCommit::RecoveryRequired {
                    reason:
                        "semantic graph revision gap waited longer than the bounded recovery window"
                            .to_string(),
                });
            }
            let queued_revision = commit.committed_revision;
            let retained_bytes = graph_commit_update_retained_bytes(&update);
            if retained_bytes > GRAPH_MUTATION_QUEUED_COMMIT_BYTE_LIMIT {
                self.clear_runtime_projection_queues();
                return Ok(StagedGraphCommit::RecoveryRequired {
                    reason: format!(
                        "semantic graph commit {queued_revision} exceeds the queued-commit byte budget"
                    ),
                });
            }
            self.queued_commits
                .entry(queued_revision)
                .or_insert_with(|| QueuedGraphCommit::new(update));
            if self.queued_commits.len() > GRAPH_MUTATION_QUEUED_COMMIT_MAX
                || self.queued_commit_retained_bytes() > GRAPH_MUTATION_QUEUED_COMMIT_BYTE_LIMIT
            {
                self.clear_runtime_projection_queues();
                return Ok(StagedGraphCommit::RecoveryRequired {
                    reason: "semantic graph queued commit budget was exceeded".to_string(),
                });
            }
            return Ok(StagedGraphCommit::QueuedGap {
                queued_revision,
                waiting_for_revision: self.committed_revision.next(),
            });
        }

        Err(GraphCommitProjectionError::ConflictingRevision {
            visible: self.committed_revision,
            base: commit.base_revision,
            committed: commit.committed_revision,
        })
    }

    pub(super) fn mark_committed(&mut self, revision: WorkspaceGraphRevision) {
        self.committed_revision = revision;
        self.drop_stale_queued_commits();
    }

    pub(super) fn take_next_ready_commit(&mut self) -> Option<GraphMutationCommitUpdate> {
        self.drop_stale_queued_commits();
        let next_revision = self.queued_commits.iter().find_map(|(revision, update)| {
            (update.update.commit.base_revision == self.committed_revision).then_some(*revision)
        })?;
        self.queued_commits
            .remove(&next_revision)
            .map(|queued| queued.update)
    }

    fn drop_stale_queued_commits(&mut self) {
        let committed_revision = self.committed_revision;
        self.queued_commits
            .retain(|revision, _| *revision > committed_revision);
    }

    fn drop_expired_queued_commits(&mut self) -> bool {
        let now = Instant::now();
        let before = self.queued_commits.len();
        self.queued_commits.retain(|_, queued| {
            now.duration_since(queued.queued_at) <= GRAPH_MUTATION_QUEUE_MAX_AGE
        });
        before != self.queued_commits.len()
    }

    fn prune_pending_optimistic_mutations(&mut self) -> bool {
        let mut pruned = false;
        while self.pending_optimistic_mutations.len() > GRAPH_MUTATION_PENDING_OPTIMISTIC_MAX
            || self.pending_optimistic_mutation_retained_bytes()
                > GRAPH_MUTATION_PENDING_OPTIMISTIC_BYTE_LIMIT
        {
            if self.pending_optimistic_mutations.pop_front().is_none() {
                break;
            }
            pruned = true;
        }
        pruned
    }

    fn queued_commit_retained_bytes(&self) -> usize {
        self.queued_commits
            .values()
            .map(|queued| queued.retained_bytes)
            .sum()
    }

    fn pending_optimistic_mutation_retained_bytes(&self) -> usize {
        self.pending_optimistic_mutations
            .iter()
            .map(|pending| pending.retained_bytes)
            .sum()
    }

    fn clear_runtime_projection_queues(&mut self) {
        self.queued_commits.clear();
        self.pending_optimistic_mutations.clear();
    }
}

fn graph_commit_update_retained_bytes(update: &GraphMutationCommitUpdate) -> usize {
    std::mem::size_of_val(update).saturating_add(format!("{update:?}").len())
}

fn pending_optimistic_mutation_retained_bytes(mutation: &PendingOptimisticGraphMutation) -> usize {
    std::mem::size_of_val(mutation).saturating_add(format!("{mutation:?}").len())
}

#[derive(Debug)]
pub(crate) enum GraphCommitProjectionError {
    InvalidRevisionOrder {
        base: WorkspaceGraphRevision,
        committed: WorkspaceGraphRevision,
    },
    ConflictingRevision {
        visible: WorkspaceGraphRevision,
        base: WorkspaceGraphRevision,
        committed: WorkspaceGraphRevision,
    },
    ApplyPatch(SemanticGraphError),
}

#[derive(Debug)]
pub(crate) enum GraphOptimisticProjectionError {
    StaleBaseRevision {
        mutation_id: OptimisticGraphMutationId,
        expected: WorkspaceGraphRevision,
        actual: WorkspaceGraphRevision,
    },
    ApplyPatch {
        mutation_id: OptimisticGraphMutationId,
        error: SemanticGraphError,
    },
    ReplayPendingPatch {
        mutation_id: OptimisticGraphMutationId,
        error: SemanticGraphError,
    },
}

impl std::fmt::Display for GraphCommitProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRevisionOrder { base, committed } => write!(
                formatter,
                "semantic graph commit revision {committed} does not advance base revision {base}"
            ),
            Self::ConflictingRevision {
                visible,
                base,
                committed,
            } => write!(
                formatter,
                "semantic graph commit {base}->{committed} cannot be applied to visible revision {visible}"
            ),
            Self::ApplyPatch(error) => write!(
                formatter,
                "semantic graph commit could not be applied to the committed projection: {error}"
            ),
        }
    }
}

impl std::error::Error for GraphCommitProjectionError {}

impl std::fmt::Display for GraphOptimisticProjectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleBaseRevision {
                mutation_id,
                expected,
                actual,
            } => write!(
                formatter,
                "optimistic semantic graph mutation {} was based on revision {actual}, but visible graph commits are at revision {expected}",
                mutation_id.value()
            ),
            Self::ApplyPatch { mutation_id, error } => write!(
                formatter,
                "optimistic semantic graph mutation {} could not be applied: {error}",
                mutation_id.value()
            ),
            Self::ReplayPendingPatch { mutation_id, error } => write!(
                formatter,
                "pending semantic graph mutation {} could not be replayed after graph reconciliation: {error}",
                mutation_id.value()
            ),
        }
    }
}

impl std::error::Error for GraphOptimisticProjectionError {}

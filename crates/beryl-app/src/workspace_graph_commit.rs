use std::fmt;

use beryl_model::{
    semantic_graph::{SemanticGraph, SemanticGraphPatch},
    workspace::{BerylWorkspaceId, BerylWorkspaceManifest},
};
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct WorkspaceGraphRevision(u64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGraphMutationCommit {
    pub workspace_id: BerylWorkspaceId,
    pub base_revision: WorkspaceGraphRevision,
    pub committed_revision: WorkspaceGraphRevision,
    pub changed: bool,
    pub patch: SemanticGraphPatch,
    pub manifest: BerylWorkspaceManifest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceGraphStateSnapshot {
    pub graph: SemanticGraph,
    pub revision: WorkspaceGraphRevision,
}

impl WorkspaceGraphRevision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl WorkspaceGraphMutationCommit {
    pub fn new(
        workspace_id: BerylWorkspaceId,
        base_revision: WorkspaceGraphRevision,
        committed_revision: WorkspaceGraphRevision,
        changed: bool,
        patch: SemanticGraphPatch,
        manifest: BerylWorkspaceManifest,
    ) -> Self {
        Self {
            workspace_id,
            base_revision,
            committed_revision,
            changed,
            patch,
            manifest,
        }
    }
}

impl WorkspaceGraphStateSnapshot {
    pub fn new(graph: SemanticGraph, revision: WorkspaceGraphRevision) -> Self {
        Self { graph, revision }
    }
}

impl fmt::Display for WorkspaceGraphRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

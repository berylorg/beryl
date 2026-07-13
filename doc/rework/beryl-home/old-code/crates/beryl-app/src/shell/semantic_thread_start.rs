use std::{fmt, path::Path, time::Duration};

use beryl_backend::{
    ManagedBackendSession, ThreadInfo, ThreadSessionMetadata, ThreadSessionResponse,
    ThreadStartOptions,
};
use beryl_model::{semantic_graph::SemanticNode, workspace::WorkspaceId};

use crate::beryl_user_thread_start_options;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SemanticThreadStartSource {
    GraphNode,
    ChecklistItem,
}

pub(crate) struct SemanticBackendThreadStart {
    pub(crate) thread: ThreadInfo,
    pub(crate) session_metadata: ThreadSessionMetadata,
}

pub(crate) trait SemanticThreadStartBackend {
    type Error: fmt::Display;

    fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error>;
}

impl SemanticThreadStartBackend for ManagedBackendSession {
    type Error = beryl_backend::ManagedBackendError;

    fn start_thread_with_options(
        &mut self,
        cwd: &Path,
        options: ThreadStartOptions,
        timeout: Duration,
    ) -> Result<ThreadSessionResponse, Self::Error> {
        ManagedBackendSession::start_thread_with_options(self, cwd, options, timeout)
    }
}

impl SemanticThreadStartSource {
    pub(super) fn status_message(self) -> &'static str {
        match self {
            Self::GraphNode => "Starting graph-attached thread",
            Self::ChecklistItem => "Starting checklist-item thread",
        }
    }

    pub(super) fn workspace_action(self) -> &'static str {
        match self {
            Self::GraphNode => "start_graph_node_thread",
            Self::ChecklistItem => "start_checklist_item_thread",
        }
    }

    pub(super) fn non_startable_detail(self) -> &'static str {
        match self {
            Self::GraphNode => "Only topic-capable semantic nodes can start Codex threads.",
            Self::ChecklistItem => {
                "Only checklist-item rows can start checklist-item Codex threads."
            }
        }
    }

    pub(super) fn can_start(self, node: &SemanticNode) -> bool {
        match self {
            Self::GraphNode => node.facets().has_topic(),
            Self::ChecklistItem => node.facets().has_checklist_item(),
        }
    }

    fn create_failure_prefix(self) -> &'static str {
        match self {
            Self::GraphNode => "Beryl could not create a graph-attached thread",
            Self::ChecklistItem => "Beryl could not create a checklist-item thread",
        }
    }
}

pub(crate) fn semantic_thread_start_options(_: SemanticThreadStartSource) -> ThreadStartOptions {
    beryl_user_thread_start_options()
}

pub(crate) fn start_semantic_backend_thread<B>(
    backend: &mut B,
    source: SemanticThreadStartSource,
    execution_target: &WorkspaceId,
    timeout: Duration,
) -> Result<SemanticBackendThreadStart, String>
where
    B: SemanticThreadStartBackend,
{
    let response = backend
        .start_thread_with_options(
            execution_target.canonical_path(),
            semantic_thread_start_options(source),
            timeout,
        )
        .map_err(|error| format!("{}: {error}", source.create_failure_prefix()))?;
    Ok(SemanticBackendThreadStart {
        session_metadata: response.metadata(),
        thread: response.thread,
    })
}

use beryl_backend::ThreadSummary;
use beryl_model::conversation::{ConversationThreadId, WorkspaceConversationState};
use beryl_model::semantic_graph::ThreadRef;
use beryl_model::workspace::WorkspaceId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ThreadSelectionRequest {
    RestorePreferred(Option<String>),
    Exact { thread_id: String, label: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum KnownThreadSelection {
    Selected { thread_id: String, strict: bool },
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GraphThreadRefAvailability {
    Openable,
    Invalid {
        notice_title: &'static str,
        reason: String,
        detail: String,
    },
}

impl ThreadSelectionRequest {
    pub(super) fn exact(thread_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::Exact {
            thread_id: thread_id.into(),
            label: label.into(),
        }
    }
}

impl GraphThreadRefAvailability {
    pub(crate) fn is_openable(&self) -> bool {
        matches!(self, Self::Openable)
    }

    pub(crate) fn notice_title(&self) -> Option<&'static str> {
        match self {
            Self::Openable => None,
            Self::Invalid { notice_title, .. } => Some(notice_title),
        }
    }

    pub(crate) fn reason(&self) -> Option<&str> {
        match self {
            Self::Openable => None,
            Self::Invalid { reason, .. } => Some(reason),
        }
    }

    pub(crate) fn detail(&self) -> Option<&str> {
        match self {
            Self::Openable => None,
            Self::Invalid { detail, .. } => Some(detail),
        }
    }
}

pub(super) fn exact_thread_selection_request(
    thread_id: &ConversationThreadId,
    label: &str,
) -> ThreadSelectionRequest {
    ThreadSelectionRequest::exact(thread_id.as_str(), label)
}

pub(crate) fn graph_thread_ref_availability(
    workspace_state: &WorkspaceConversationState,
    thread_ref: &ThreadRef,
    implicit_home_execution_target: Option<&WorkspaceId>,
) -> GraphThreadRefAvailability {
    if let Some(reason) = workspace_state
        .thread_registration(thread_ref.thread_id())
        .and_then(|thread| thread.rebind_required())
        .map(|requirement| requirement.detail().to_string())
    {
        return GraphThreadRefAvailability::Invalid {
            notice_title: "Thread requires rebind",
            detail: thread_rebind_detail(
                thread_ref.label(),
                thread_ref.execution_target(),
                &reason,
            ),
            reason,
        };
    }

    if !workspace_state.execution_target_in_workspace_scope(
        thread_ref.execution_target(),
        implicit_home_execution_target,
    ) {
        let reason =
            "The recorded thread target is outside the current workspace scope.".to_string();
        return GraphThreadRefAvailability::Invalid {
            notice_title: "Thread link unavailable",
            detail: thread_rebind_detail(
                thread_ref.label(),
                thread_ref.execution_target(),
                &reason,
            ),
            reason,
        };
    }

    GraphThreadRefAvailability::Openable
}

pub(super) fn resolve_known_thread_selection(
    known_threads: &[ThreadSummary],
    _execution_target: &WorkspaceId,
    selection: &ThreadSelectionRequest,
) -> KnownThreadSelection {
    match selection {
        ThreadSelectionRequest::RestorePreferred(Some(thread_id)) => known_threads
            .iter()
            .find(|thread| thread.id == *thread_id)
            .map(|thread| KnownThreadSelection::Selected {
                thread_id: thread.id.clone(),
                strict: false,
            })
            .unwrap_or_else(|| {
                known_threads
                    .first()
                    .map(|thread| KnownThreadSelection::Selected {
                        thread_id: thread.id.clone(),
                        strict: false,
                    })
                    .unwrap_or(KnownThreadSelection::None)
            }),
        ThreadSelectionRequest::RestorePreferred(None) => known_threads
            .first()
            .map(|thread| KnownThreadSelection::Selected {
                thread_id: thread.id.clone(),
                strict: false,
            })
            .unwrap_or(KnownThreadSelection::None),
        ThreadSelectionRequest::Exact { .. } => KnownThreadSelection::None,
    }
}

pub(super) fn thread_rebind_detail(
    label: &str,
    execution_target: &WorkspaceId,
    reason: &str,
) -> String {
    format!(
        "Beryl cannot activate thread {label:?} on {}. {reason} Explicit rebinding is required before this thread can continue.",
        execution_target.display_label()
    )
}

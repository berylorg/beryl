use beryl_model::conversation::{
    ConversationThreadId, SyndicConversationViewId, WorkspaceConversationState,
};
use beryl_model::semantic_graph::ThreadRef;
use beryl_model::workspace::WorkspaceId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ThreadSelectionRequest {
    RestorePreferred(Option<String>),
    Exact {
        thread_id: String,
        syndic_view_id: String,
        label: String,
    },
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
    pub(super) fn exact(
        thread_id: impl Into<String>,
        syndic_view_id: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self::Exact {
            thread_id: thread_id.into(),
            syndic_view_id: syndic_view_id.into(),
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
    syndic_view_id: &SyndicConversationViewId,
    label: &str,
) -> ThreadSelectionRequest {
    ThreadSelectionRequest::exact(thread_id.as_str(), syndic_view_id.as_str(), label)
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

    if workspace_state
        .catalog_thread_registration(thread_ref.thread_id())
        .is_none()
    {
        let reason =
            "The recorded thread is not registered as a workspace Syndic conversation view."
                .to_string();
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

pub(crate) fn thread_rebind_detail(
    label: &str,
    execution_target: &WorkspaceId,
    reason: &str,
) -> String {
    format!(
        "Beryl cannot activate thread {label:?} on {}. {reason} Explicit rebinding is required before this thread can continue.",
        execution_target.display_label()
    )
}

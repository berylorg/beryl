use beryl_backend::ThreadSummary;
use beryl_model::conversation::{ConversationThreadId, WorkspaceConversationState};
use beryl_model::semantic_graph::ThreadRef;
use beryl_model::workspace::WorkspaceId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ThreadSelectionRequest {
    RestorePreferred(Option<String>),
    Exact {
        thread_id: String,
        label: String,
        expected_forked_from_id: Option<String>,
    },
    PersistedActiveRepairRequired {
        thread_id: String,
        label: String,
        detail: String,
    },
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
            expected_forked_from_id: None,
        }
    }

    fn exact_recovery(
        thread_id: impl Into<String>,
        label: impl Into<String>,
        expected_forked_from_id: Option<String>,
    ) -> Self {
        Self::Exact {
            thread_id: thread_id.into(),
            label: label.into(),
            expected_forked_from_id,
        }
    }
}

pub(super) fn backend_unavailable_thread_seed() -> (Vec<ThreadSummary>, Option<String>) {
    (Vec::new(), None)
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

pub(super) fn persisted_active_thread_selection_request(
    workspace_state: &WorkspaceConversationState,
    execution_target: &WorkspaceId,
) -> Option<ThreadSelectionRequest> {
    let thread = workspace_state.active_thread_registration()?;
    let recovery_target = persisted_active_thread_recovery_target(workspace_state);
    if recovery_target.is_none() {
        return Some(persisted_active_thread_repair_selection_request(
            workspace_state,
        )?);
    }
    if recovery_target.as_ref() != Some(execution_target) {
        return None;
    }

    let thread_id = thread.thread_id().as_str();
    let label = thread
        .title()
        .or_else(|| non_empty_trimmed(thread.preview()))
        .unwrap_or("Untitled thread");
    let expected_forked_from_id = thread
        .orchestration_root_thread_id()
        .filter(|root_thread_id| root_thread_id.as_str() != thread_id)
        .map(|root_thread_id| root_thread_id.as_str().to_string());

    Some(ThreadSelectionRequest::exact_recovery(
        thread_id,
        label,
        expected_forked_from_id,
    ))
}

pub(super) fn persisted_active_thread_disconnect_selection_request(
    workspace_state: &WorkspaceConversationState,
    execution_target: &WorkspaceId,
    selected_thread_id: &str,
) -> Option<ThreadSelectionRequest> {
    let selection = persisted_active_thread_selection_request(workspace_state, execution_target)?;
    let persisted_thread_id = match &selection {
        ThreadSelectionRequest::Exact { thread_id, .. }
        | ThreadSelectionRequest::PersistedActiveRepairRequired { thread_id, .. } => thread_id,
        ThreadSelectionRequest::RestorePreferred(_) => return None,
    };
    (persisted_thread_id == selected_thread_id).then_some(selection)
}

pub(super) fn persisted_active_thread_repair_selection_request(
    workspace_state: &WorkspaceConversationState,
) -> Option<ThreadSelectionRequest> {
    let thread = workspace_state.active_thread_registration()?;
    let reason = persisted_active_thread_repair_reason(workspace_state)?;
    let label = thread
        .title()
        .or_else(|| non_empty_trimmed(thread.preview()))
        .unwrap_or("Untitled thread");
    Some(ThreadSelectionRequest::PersistedActiveRepairRequired {
        thread_id: thread.thread_id().as_str().to_string(),
        label: label.to_string(),
        detail: bounded_repair_detail(thread_rebind_detail(
            label,
            thread.execution_target(),
            &reason,
        )),
    })
}

pub(super) fn persisted_active_thread_recovery_target(
    workspace_state: &WorkspaceConversationState,
) -> Option<WorkspaceId> {
    let thread = workspace_state.active_thread_registration()?;
    if thread.requires_rebind() {
        return None;
    }
    let current_binding =
        workspace_state.binding_for_execution_target(thread.execution_target())?;
    (thread.member_binding() == Some(&current_binding)).then(|| thread.execution_target().clone())
}

fn persisted_active_thread_repair_reason(
    workspace_state: &WorkspaceConversationState,
) -> Option<String> {
    let thread = workspace_state.active_thread_registration()?;
    if let Some(requirement) = thread.rebind_required() {
        return Some(requirement.detail().to_string());
    }

    let Some(current_binding) =
        workspace_state.binding_for_execution_target(thread.execution_target())
    else {
        return Some(
            "The recorded execution target is not an available member of this workspace."
                .to_string(),
        );
    };
    match thread.member_binding() {
        None => Some(
            "The persisted thread registration does not include an exact workspace-member binding."
                .to_string(),
        ),
        Some(binding) if binding != &current_binding => Some(
            "The persisted workspace-member binding no longer matches the current member identity."
                .to_string(),
        ),
        Some(_) => None,
    }
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

fn non_empty_trimmed(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
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
        ThreadSelectionRequest::Exact { .. }
        | ThreadSelectionRequest::PersistedActiveRepairRequired { .. } => {
            KnownThreadSelection::None
        }
    }
}

fn bounded_repair_detail(detail: String) -> String {
    const MAX_REPAIR_DETAIL_CHARS: usize = 1_200;

    if detail.chars().count() <= MAX_REPAIR_DETAIL_CHARS {
        return detail;
    }
    let mut bounded: String = detail
        .chars()
        .take(MAX_REPAIR_DETAIL_CHARS.saturating_sub(3))
        .collect();
    bounded.push_str("...");
    bounded
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

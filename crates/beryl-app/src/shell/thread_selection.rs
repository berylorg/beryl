use beryl_backend::ThreadSummary;
use beryl_model::conversation::ConversationThreadId;
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

impl ThreadSelectionRequest {
    pub(super) fn exact(thread_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::Exact {
            thread_id: thread_id.into(),
            label: label.into(),
        }
    }
}

pub(super) fn exact_thread_selection_request(
    thread_id: &ConversationThreadId,
    label: &str,
) -> ThreadSelectionRequest {
    ThreadSelectionRequest::exact(thread_id.as_str(), label)
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

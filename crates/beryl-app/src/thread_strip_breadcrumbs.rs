use std::collections::HashSet;

use beryl_model::{
    conversation::{
        ConversationThreadId, RegisteredConversationThread, WorkspaceConversationState,
    },
    workspace::WorkspaceId,
};

use crate::member_thread_inventory::resolved_thread_title;

const MAX_THREAD_STRIP_BREADCRUMB_DEPTH: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadStripBreadcrumbTrail {
    segments: Vec<ThreadStripBreadcrumbSegment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadStripBreadcrumbSegment {
    thread_id: ConversationThreadId,
    label: String,
    execution_target: Option<WorkspaceId>,
    disabled_reason: Option<String>,
    active: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransientBranchParent<'a> {
    pub(crate) child_thread_id: &'a ConversationThreadId,
    pub(crate) parent_thread_id: &'a ConversationThreadId,
}

impl ThreadStripBreadcrumbTrail {
    pub(crate) fn segments(&self) -> &[ThreadStripBreadcrumbSegment] {
        &self.segments
    }
}

impl ThreadStripBreadcrumbSegment {
    pub(crate) fn thread_id(&self) -> &ConversationThreadId {
        &self.thread_id
    }

    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    pub(crate) fn execution_target(&self) -> Option<&WorkspaceId> {
        self.execution_target.as_ref()
    }

    pub(crate) fn disabled_reason(&self) -> Option<&str> {
        self.disabled_reason.as_deref()
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    pub(crate) fn activation_available(&self) -> bool {
        !self.active && self.execution_target.is_some() && self.disabled_reason.is_none()
    }
}

pub(crate) fn thread_strip_breadcrumb_trail(
    workspace_state: &WorkspaceConversationState,
    selected_thread_id: Option<&str>,
    active_label: &str,
    transient_parent: Option<TransientBranchParent<'_>>,
) -> Option<ThreadStripBreadcrumbTrail> {
    let selected_thread_id = selected_thread_id?.trim();
    if selected_thread_id.is_empty() {
        return None;
    }

    let selected_thread_id = ConversationThreadId::new(selected_thread_id.to_string());
    let first_parent =
        parent_thread_id_for_child(workspace_state, &selected_thread_id, transient_parent)?;
    if first_parent == selected_thread_id {
        return None;
    }

    let mut visited = HashSet::new();
    visited.insert(selected_thread_id.as_str().to_string());
    let mut parent_id = first_parent;
    let mut ancestor_segments = Vec::new();

    for _ in 0..MAX_THREAD_STRIP_BREADCRUMB_DEPTH {
        if !visited.insert(parent_id.as_str().to_string()) {
            break;
        }

        let parent_registration = workspace_state.thread_registration(&parent_id);
        ancestor_segments.push(parent_segment(
            workspace_state,
            &parent_id,
            parent_registration,
        ));

        let Some(next_parent_id) =
            parent_registration.and_then(RegisteredConversationThread::branch_parent_thread_id)
        else {
            break;
        };
        if next_parent_id == &parent_id {
            break;
        }
        parent_id = next_parent_id.clone();
    }

    if ancestor_segments.is_empty() {
        return None;
    }

    ancestor_segments.reverse();
    ancestor_segments.push(ThreadStripBreadcrumbSegment {
        thread_id: selected_thread_id,
        label: active_label.to_string(),
        execution_target: None,
        disabled_reason: None,
        active: true,
    });

    Some(ThreadStripBreadcrumbTrail {
        segments: ancestor_segments,
    })
}

fn parent_thread_id_for_child(
    workspace_state: &WorkspaceConversationState,
    child_thread_id: &ConversationThreadId,
    transient_parent: Option<TransientBranchParent<'_>>,
) -> Option<ConversationThreadId> {
    if let Some(parent) = transient_parent
        && parent.child_thread_id == child_thread_id
    {
        return Some(parent.parent_thread_id.clone());
    }

    workspace_state
        .thread_registration(child_thread_id)
        .and_then(RegisteredConversationThread::branch_parent_thread_id)
        .cloned()
}

fn parent_segment(
    workspace_state: &WorkspaceConversationState,
    parent_thread_id: &ConversationThreadId,
    parent: Option<&RegisteredConversationThread>,
) -> ThreadStripBreadcrumbSegment {
    let Some(parent) = parent else {
        return ThreadStripBreadcrumbSegment {
            thread_id: parent_thread_id.clone(),
            label: "Parent unavailable".to_string(),
            execution_target: None,
            disabled_reason: Some("The parent thread is no longer registered.".to_string()),
            active: false,
        };
    };

    let label = resolved_thread_title(
        workspace_state,
        parent_thread_id,
        parent.execution_target(),
        parent.preview(),
        parent.backend_name(),
        parent.created_at_millis(),
        parent.updated_at_millis(),
    );
    let disabled_reason = parent.rebind_required().map(|requirement| {
        format!(
            "The parent thread requires rebind before it can be opened: {}",
            requirement.detail()
        )
    });

    ThreadStripBreadcrumbSegment {
        thread_id: parent_thread_id.clone(),
        label,
        execution_target: Some(parent.execution_target().clone()),
        disabled_reason,
        active: false,
    }
}

use beryl_model::{conversation::ConversationThreadId, workspace::WorkspaceId};
use gpui::{Context, Window};

use crate::member_thread_inventory::resolved_thread_title;

use super::thread_navigation::{
    PendingThreadNavigationActivation, ThreadNavigationEntry, ThreadNavigationHistory,
};
use super::thread_selection::thread_rebind_detail;
use super::thread_selector::ThreadSelectorActivationTarget;
use super::{
    ConversationSurfaceState, ShellView, SurfaceNotice, ThreadActivationStart,
    ThreadNavigationActivationSource,
};

impl ShellView {
    pub(super) fn thread_navigation_backward_disabled_reason(&self) -> Option<String> {
        self.thread_navigation_disabled_reason(ThreadNavigationActivationSource::BackwardNavigation)
    }

    pub(super) fn thread_navigation_forward_disabled_reason(&self) -> Option<String> {
        self.thread_navigation_disabled_reason(ThreadNavigationActivationSource::ForwardNavigation)
    }

    pub(super) fn activated_link_thread_target(
        &self,
        thread_id: &ConversationThreadId,
    ) -> Result<ThreadSelectorActivationTarget, (&'static str, String)> {
        let loaded = self.loaded_workspace().ok_or_else(|| {
            (
                "Thread link unavailable",
                "No workspace is loaded.".to_string(),
            )
        })?;
        let registration = loaded
            .workspace_state
            .thread_registration(thread_id)
            .ok_or_else(|| {
                (
                    "Thread link unavailable",
                    format!(
                        "Beryl cannot activate thread link {} because the thread is not registered in this workspace.",
                        thread_id.as_str()
                    ),
                )
            })?;
        let execution_target = registration.execution_target().clone();
        let label = resolved_thread_title(
            &loaded.workspace_state,
            thread_id,
            &execution_target,
            registration.preview(),
            registration.backend_name(),
            registration.created_at_millis(),
            registration.updated_at_millis(),
        );
        let label = if label.trim().is_empty() {
            "Linked thread".to_string()
        } else {
            label
        };

        if let Some(requirement) = registration.rebind_required() {
            return Err((
                "Thread requires rebind",
                thread_rebind_detail(&label, &execution_target, requirement.detail()),
            ));
        }

        let implicit_home_target = loaded.resolved_implicit_home_execution_target();
        if !loaded
            .workspace_state
            .execution_target_in_workspace_scope(&execution_target, implicit_home_target.as_ref())
        {
            return Err((
                "Thread link unavailable",
                thread_rebind_detail(
                    &label,
                    &execution_target,
                    "The recorded thread target is outside the current workspace scope.",
                ),
            ));
        }

        Ok(ThreadSelectorActivationTarget {
            thread_id: thread_id.clone(),
            label,
            execution_target,
        })
    }

    fn current_thread_navigation_history(&self) -> Option<&ThreadNavigationHistory> {
        let workspace_id = self.loaded_workspace()?.workspace.id();
        self.thread_navigation_histories.get(workspace_id)
    }

    fn current_thread_navigation_entry(&self) -> Option<ThreadNavigationEntry> {
        let loaded = self.loaded_workspace()?;
        let selected_thread_id = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_id)?;
        let thread_id = ConversationThreadId::new(selected_thread_id.to_string());
        let registration = loaded.workspace_state.thread_registration(&thread_id)?;
        ThreadNavigationEntry::new(thread_id, registration.execution_target().clone())
    }

    pub(super) fn pending_thread_navigation_activation_for_target(
        &self,
        source: ThreadNavigationActivationSource,
        target: &ThreadSelectorActivationTarget,
    ) -> Option<PendingThreadNavigationActivation> {
        let workspace_id = self.loaded_workspace()?.workspace.id().clone();
        let target =
            ThreadNavigationEntry::new(target.thread_id.clone(), target.execution_target.clone())?;
        PendingThreadNavigationActivation::new(
            workspace_id,
            source,
            self.current_thread_navigation_entry(),
            target,
        )
    }

    fn thread_navigation_activation_target(
        &self,
        entry: &ThreadNavigationEntry,
    ) -> Result<ThreadSelectorActivationTarget, (&'static str, String)> {
        let target = self.activated_link_thread_target(entry.thread_id())?;
        if &target.execution_target != entry.execution_target() {
            return Err((
                "Thread link unavailable",
                thread_rebind_detail(
                    &target.label,
                    entry.execution_target(),
                    "The recorded navigation target no longer matches the registered thread target.",
                ),
            ));
        }
        Ok(target)
    }

    fn thread_navigation_disabled_reason(
        &self,
        source: ThreadNavigationActivationSource,
    ) -> Option<String> {
        let entry = match source {
            ThreadNavigationActivationSource::BackwardNavigation => self
                .current_thread_navigation_history()
                .and_then(ThreadNavigationHistory::back_target)
                .cloned(),
            ThreadNavigationActivationSource::ForwardNavigation => self
                .current_thread_navigation_history()
                .and_then(ThreadNavigationHistory::forward_target)
                .cloned(),
            ThreadNavigationActivationSource::ThreadSelector
            | ThreadNavigationActivationSource::TranscriptThreadLink
            | ThreadNavigationActivationSource::BranchBreadcrumb
            | ThreadNavigationActivationSource::NonHistory => None,
        };
        let Some(entry) = entry else {
            return Some(match source {
                ThreadNavigationActivationSource::BackwardNavigation => {
                    "No backward thread history.".to_string()
                }
                ThreadNavigationActivationSource::ForwardNavigation => {
                    "No forward thread history.".to_string()
                }
                ThreadNavigationActivationSource::ThreadSelector
                | ThreadNavigationActivationSource::TranscriptThreadLink
                | ThreadNavigationActivationSource::BranchBreadcrumb
                | ThreadNavigationActivationSource::NonHistory => {
                    "No thread navigation target is available.".to_string()
                }
            });
        };

        if let Some(message) = self.thread_activation_busy_message() {
            return Some(message);
        }

        let target = match self.thread_navigation_activation_target(&entry) {
            Ok(target) => target,
            Err((_, message)) => return Some(message),
        };

        if let Some(block) =
            self.known_backend_unavailable_block_for_target(&target.execution_target)
        {
            return Some(block.message);
        }

        match &self.state {
            super::ShellState::Ready(ready) => {
                if ready.execution_target == target.execution_target
                    && self
                        .backend_client_connector_for_execution_target(&target.execution_target)
                        .is_none()
                {
                    self.backend_required_target_block(&target.execution_target)
                        .map(|block| block.message)
                } else {
                    None
                }
            }
            super::ShellState::BackendUnavailable(unavailable) => {
                if unavailable.execution_target == target.execution_target {
                    self.backend_required_target_block(&target.execution_target)
                        .map(|block| block.message)
                } else {
                    None
                }
            }
            super::ShellState::Blocked(blocked) if blocked.surface.is_some() => {
                Some(blocked.summary.clone())
            }
            super::ShellState::Discovering(_)
            | super::ShellState::Picker(_)
            | super::ShellState::Opening(_)
            | super::ShellState::WorkspaceIdle(_)
            | super::ShellState::WorkspaceLoaded(_)
            | super::ShellState::Blocked(_) => {
                Some("Beryl is not on a ready workspace surface.".to_string())
            }
        }
    }

    fn thread_activation_busy_message(&self) -> Option<String> {
        (self.workspace_receiver.is_some()
            || self.graph_thread_start_receiver.is_some()
            || self.transcript_branch_receiver.is_some()
            || self.transcript_edit_commit_receiver.is_some()
            || self.thread_activation_receiver.is_some()
            || self.thread_history_page_receiver.is_some()
            || self.status_operation_receiver.is_some()
            || self.turn_receiver.is_some()
            || !self.turn_steering_receivers.is_empty())
        .then(|| {
            "Beryl is already running workspace, transcript, status, or turn work that blocks thread activation."
                .to_string()
        })
    }

    pub(super) fn finish_pending_thread_navigation_activation(
        &mut self,
        activated_thread_id: &str,
        execution_target: &WorkspaceId,
    ) -> bool {
        let Some(pending) = self.pending_thread_navigation_activation.take() else {
            return false;
        };
        let target = pending.target();
        if target.thread_id().as_str() != activated_thread_id
            || target.execution_target() != execution_target
        {
            return false;
        }
        let Some(loaded) = self.loaded_workspace() else {
            return false;
        };
        if loaded.workspace.id() != pending.workspace_id() {
            return false;
        }
        if self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_id)
            != Some(activated_thread_id)
        {
            return false;
        }

        let workspace_id = pending.workspace_id().clone();
        let history = self
            .thread_navigation_histories
            .entry(workspace_id)
            .or_default();
        pending.commit(history)
    }

    pub(super) fn discard_pending_thread_navigation_activation(&mut self) {
        self.pending_thread_navigation_activation = None;
    }

    pub(super) fn discard_thread_navigation_for_execution_target(
        &mut self,
        execution_target: &WorkspaceId,
    ) -> bool {
        let mut changed = false;
        self.thread_navigation_histories.retain(|_, history| {
            changed |= history.discard_entries_for_execution_target(execution_target);
            !history.is_empty()
        });
        changed
    }

    pub(super) fn discard_all_thread_navigation_histories(&mut self) -> bool {
        let changed = !self.thread_navigation_histories.is_empty()
            || self.pending_thread_navigation_activation.is_some();
        self.thread_navigation_histories.clear();
        self.pending_thread_navigation_activation = None;
        changed
    }

    pub(super) fn activate_branch_breadcrumb_thread_target(
        &mut self,
        target: ThreadSelectorActivationTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ThreadActivationStart {
        self.activate_thread_selector_target(
            target,
            ThreadNavigationActivationSource::BranchBreadcrumb,
            window,
            cx,
        )
    }

    pub(super) fn activate_thread_navigation_backward(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ThreadActivationStart {
        self.activate_thread_navigation(
            ThreadNavigationActivationSource::BackwardNavigation,
            window,
            cx,
        )
    }

    pub(super) fn activate_thread_navigation_forward(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ThreadActivationStart {
        self.activate_thread_navigation(
            ThreadNavigationActivationSource::ForwardNavigation,
            window,
            cx,
        )
    }

    fn activate_thread_navigation(
        &mut self,
        source: ThreadNavigationActivationSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ThreadActivationStart {
        let entry = match source {
            ThreadNavigationActivationSource::BackwardNavigation => self
                .current_thread_navigation_history()
                .and_then(ThreadNavigationHistory::back_target)
                .cloned(),
            ThreadNavigationActivationSource::ForwardNavigation => self
                .current_thread_navigation_history()
                .and_then(ThreadNavigationHistory::forward_target)
                .cloned(),
            ThreadNavigationActivationSource::ThreadSelector
            | ThreadNavigationActivationSource::TranscriptThreadLink
            | ThreadNavigationActivationSource::BranchBreadcrumb
            | ThreadNavigationActivationSource::NonHistory => None,
        };
        let Some(entry) = entry else {
            return ThreadActivationStart::Rejected {
                kind: "no_navigation_target",
                message: "There is no recorded thread navigation target in that direction."
                    .to_string(),
            };
        };
        let target = match self.thread_navigation_activation_target(&entry) {
            Ok(target) => target,
            Err((title, message)) => {
                if let Some(surface) = self.conversation_surface_mut() {
                    surface.set_notice(SurfaceNotice::new(title, message.clone()));
                }
                cx.notify();
                return ThreadActivationStart::Rejected {
                    kind: "navigation_target_unavailable",
                    message,
                };
            }
        };

        self.activate_thread_selector_target(target, source, window, cx)
    }
}

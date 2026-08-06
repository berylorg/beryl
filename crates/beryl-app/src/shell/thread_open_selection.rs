use beryl_model::workspace::WorkspaceId;

use super::thread_selection::{
    ThreadSelectionRequest, persisted_active_thread_disconnect_selection_request,
    persisted_active_thread_repair_selection_request, persisted_active_thread_selection_request,
};
use super::{RetryTarget, ShellState, ShellView};

impl ShellView {
    fn preferred_thread_id_for_target(&self, execution_target: &WorkspaceId) -> Option<String> {
        match &self.state {
            ShellState::Ready(ready) if &ready.execution_target == execution_target => {
                ready.surface.selected_thread_id().map(str::to_string)
            }
            ShellState::BackendUnavailable(unavailable)
                if &unavailable.execution_target == execution_target =>
            {
                unavailable.surface.selected_thread_id().map(str::to_string)
            }
            ShellState::Blocked(blocked) if matches!(&blocked.target, RetryTarget::Workspace(target) if target == execution_target) => {
                blocked
                    .surface
                    .as_ref()
                    .and_then(|surface| surface.selected_thread_id().map(str::to_string))
            }
            _ => self
                .workspace_shell_state()
                .and_then(|loaded| loaded.workspace_state.active_thread_registration())
                .filter(|thread| {
                    thread.execution_target() == execution_target && !thread.requires_rebind()
                })
                .map(|thread| thread.thread_id().as_str().to_string()),
        }
    }

    pub(super) fn thread_selection_for_open_target(
        &self,
        target: &RetryTarget,
    ) -> ThreadSelectionRequest {
        if let Some(selection) = self.recovery_thread_for_target(target) {
            return selection;
        }

        if matches!(target, RetryTarget::WorkspacePrimary)
            && let Some(selection) = self.workspace_shell_state().and_then(|loaded| {
                persisted_active_thread_repair_selection_request(&loaded.workspace_state)
            })
        {
            return selection;
        }

        if let RetryTarget::Workspace(execution_target) = target
            && let Some(selection) = self.workspace_shell_state().and_then(|loaded| {
                persisted_active_thread_selection_request(&loaded.workspace_state, execution_target)
            })
        {
            return selection;
        }

        let preferred_thread_id = match target {
            RetryTarget::Workspace(execution_target) => {
                self.preferred_thread_id_for_target(execution_target)
            }
            RetryTarget::WorkspacePrimary => self
                .workspace_shell_state()
                .and_then(|loaded| loaded.workspace_state.active_thread())
                .map(|thread_id| thread_id.as_str().to_string()),
            RetryTarget::Startup | RetryTarget::HostPath(_) | RetryTarget::WslPath { .. } => None,
        };
        ThreadSelectionRequest::RestorePreferred(preferred_thread_id)
    }

    fn recovery_thread_for_target(&self, target: &RetryTarget) -> Option<ThreadSelectionRequest> {
        let RetryTarget::Workspace(execution_target) = target else {
            return None;
        };
        let ShellState::Blocked(blocked) = &self.state else {
            return None;
        };
        if !blocked.disconnect
            || !matches!(&blocked.target, RetryTarget::Workspace(target) if target == execution_target)
        {
            return None;
        }

        let workspace_state = blocked
            .loaded_workspace
            .as_ref()
            .map(|loaded| &loaded.workspace_state)?;
        let selected_thread_id = blocked.surface.as_ref()?.selected_thread()?.id.as_str();
        persisted_active_thread_disconnect_selection_request(
            workspace_state,
            execution_target,
            selected_thread_id,
        )
    }
}

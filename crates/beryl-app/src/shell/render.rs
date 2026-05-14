pub(super) mod checklist_sidebar;
mod code_panel;
mod column_selector;
mod common;
mod conversation;
mod graph_link_menu;
mod graph_link_menu_rows;
mod graph_overlay;
mod scrollbars;
mod startup;
mod status_operation;
mod thread_selector;
pub(super) mod transcript;
mod transcript_branch_menu;
mod workspace_picker;
mod workspace_picker_row_menu;

use gpui::{Context, Render, Window, prelude::*};

use super::ShellView;

impl Render for ShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.shell_scrollbars_animating() {
            window.request_animation_frame();
        }
        let shell = &*self;
        match &self.state {
            super::ShellState::WorkspaceIdle(idle) => conversation::render_idle_workspace_shell(
                shell,
                idle,
                &self.wsl_distro_input,
                &self.workspace_picker_filter_input,
                &self.workspace_rename_input,
                &self.conversation_input,
                window,
                cx,
            ),
            super::ShellState::WorkspaceLoaded(loaded) => {
                conversation::render_loaded_workspace_shell(
                    shell,
                    loaded,
                    &self.host_path_input,
                    &self.wsl_distro_input,
                    &self.wsl_path_input,
                    &self.workspace_picker_filter_input,
                    &self.workspace_rename_input,
                    &self.conversation_input,
                    window,
                    cx,
                )
            }
            super::ShellState::Ready(ready) => conversation::render_ready_shell(
                shell,
                ready,
                &self.transcript_panel,
                &self.checklist_sidebar_panel,
                &self.wsl_distro_input,
                &self.workspace_picker_filter_input,
                &self.workspace_rename_input,
                &self.conversation_input,
                window,
                cx,
            ),
            super::ShellState::BackendUnavailable(unavailable) => {
                conversation::render_backend_unavailable_shell(
                    shell,
                    unavailable,
                    &self.transcript_panel,
                    &self.checklist_sidebar_panel,
                    &self.wsl_distro_input,
                    &self.workspace_picker_filter_input,
                    &self.workspace_rename_input,
                    &self.conversation_input,
                    window,
                    cx,
                )
            }
            super::ShellState::Blocked(blocked) if blocked.surface.is_some() => {
                conversation::render_blocked_shell(
                    shell,
                    blocked,
                    &self.transcript_panel,
                    &self.checklist_sidebar_panel,
                    &self.wsl_distro_input,
                    &self.workspace_picker_filter_input,
                    &self.workspace_rename_input,
                    &self.conversation_input,
                    window,
                    cx,
                )
            }
            _ => startup::render_startup_shell(
                shell,
                &self.state,
                &self.startup_scroll_handle,
                &self.host_path_input,
                &self.wsl_path_input,
                cx,
            ),
        }
        .into_any_element()
    }
}

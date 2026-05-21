use gpui::Context;

use super::{ConversationSurfaceState, ScrollbarRegion, ShellState, ShellView};

impl ShellView {
    pub(in crate::shell) fn notify_transcript_panel(&self, cx: &mut Context<Self>) {
        self.transcript_panel.update(cx, |_, cx| {
            cx.notify();
        });
    }

    pub(in crate::shell) fn conversation_surface(&self) -> Option<&ConversationSurfaceState> {
        match &self.state {
            ShellState::BackendUnavailable(unavailable) => Some(&unavailable.surface),
            ShellState::Ready(ready) => Some(&ready.surface),
            ShellState::Blocked(blocked) => blocked.surface.as_ref(),
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        }
    }

    pub(in crate::shell) fn prune_graph_scrollbar_visibility(&mut self) {
        let active_graph_columns: Vec<_> = self
            .conversation_surface()
            .map(|surface| {
                surface
                    .graph_column_selector_scroll
                    .column_keys()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        self.scrollbar_visibility.retain(|region, _| match region {
            ScrollbarRegion::GraphColumn(column_key) => active_graph_columns.contains(column_key),
            _ => true,
        });
    }
}

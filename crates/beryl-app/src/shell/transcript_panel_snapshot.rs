use std::time::Instant;

use crate::diagnostic_dynamic_tools::diagnostic_duration_micros;

use super::{ShellState, ShellView, render};

impl ShellView {
    pub(in crate::shell) fn transcript_panel_snapshot(
        &self,
    ) -> Option<render::transcript::TranscriptPanelSnapshot> {
        let style_snapshot_started = Instant::now();
        let style_snapshot = self.render_style_snapshot();
        let style_snapshot_micros = diagnostic_duration_micros(style_snapshot_started.elapsed());
        let transcript_theme = style_snapshot.transcript_theme();
        let composer_measurement_micros = self.last_composer_measurement_micros();
        match &self.state {
            ShellState::Ready(ready) => Some(render::transcript::TranscriptPanelSnapshot {
                workspace_id: Some(ready.loaded_workspace.workspace.id().clone()),
                workspace: ready.execution_target.clone(),
                theme: transcript_theme.clone(),
                selected_thread_present: ready.surface.selected_thread().is_some(),
                selected_thread_id: ready.surface.selected_thread_id().map(str::to_string),
                theme_candidates: self.theme_candidate_state.snapshot(),
                pending_thread_activation_label: ready
                    .surface
                    .pending_thread_activation_label()
                    .map(str::to_string),
                transcript_width: ready.surface.transcript_width(),
                transcript_list_state: ready.surface.transcript_list_state(),
                live_scroll: ready.surface.transcript_live_scroll_effect_snapshot(),
                live_scroll_preserves_anchor_offset: ready
                    .surface
                    .transcript_live_scroll_preserves_anchor_offset(),
                older_history_loading: ready.surface.older_history_loading(),
                metrics: tracing::enabled!(tracing::Level::DEBUG)
                    .then(|| ready.surface.transcript_presentation().render_metrics()),
                activity_caret: ready.surface.transcript_activity_caret(),
                transcript_edit_mode: ready.surface.transcript_edit_mode_snapshot(),
                transcript_reset_generation: ready.surface.transcript_reset_generation(),
                content_release_generation: ready.surface.transcript_content_release_generation(),
                content_release_row_identities: ready
                    .surface
                    .transcript_content_release_row_identities()
                    .to_vec(),
                style_snapshot_micros,
                composer_measurement_micros,
            }),
            ShellState::BackendUnavailable(unavailable) => {
                Some(render::transcript::TranscriptPanelSnapshot {
                    workspace_id: Some(unavailable.loaded_workspace.workspace.id().clone()),
                    workspace: unavailable.execution_target.clone(),
                    theme: transcript_theme.clone(),
                    selected_thread_present: unavailable.surface.selected_thread().is_some(),
                    selected_thread_id: unavailable
                        .surface
                        .selected_thread_id()
                        .map(str::to_string),
                    theme_candidates: self.theme_candidate_state.snapshot(),
                    pending_thread_activation_label: unavailable
                        .surface
                        .pending_thread_activation_label()
                        .map(str::to_string),
                    transcript_width: unavailable.surface.transcript_width(),
                    transcript_list_state: unavailable.surface.transcript_list_state(),
                    live_scroll: unavailable.surface.transcript_live_scroll_effect_snapshot(),
                    live_scroll_preserves_anchor_offset: unavailable
                        .surface
                        .transcript_live_scroll_preserves_anchor_offset(),
                    older_history_loading: unavailable.surface.older_history_loading(),
                    metrics: tracing::enabled!(tracing::Level::DEBUG).then(|| {
                        unavailable
                            .surface
                            .transcript_presentation()
                            .render_metrics()
                    }),
                    activity_caret: unavailable.surface.transcript_activity_caret(),
                    transcript_edit_mode: unavailable.surface.transcript_edit_mode_snapshot(),
                    transcript_reset_generation: unavailable.surface.transcript_reset_generation(),
                    content_release_generation: unavailable
                        .surface
                        .transcript_content_release_generation(),
                    content_release_row_identities: unavailable
                        .surface
                        .transcript_content_release_row_identities()
                        .to_vec(),
                    style_snapshot_micros,
                    composer_measurement_micros,
                })
            }
            ShellState::Blocked(blocked) => blocked.surface.as_ref().map(|surface| {
                render::transcript::TranscriptPanelSnapshot {
                    workspace_id: blocked
                        .loaded_workspace
                        .as_ref()
                        .map(|loaded| loaded.workspace.id().clone()),
                    workspace: blocked.target.workspace(),
                    theme: transcript_theme.clone(),
                    selected_thread_present: surface.selected_thread().is_some(),
                    selected_thread_id: surface.selected_thread_id().map(str::to_string),
                    theme_candidates: self.theme_candidate_state.snapshot(),
                    pending_thread_activation_label: surface
                        .pending_thread_activation_label()
                        .map(str::to_string),
                    transcript_width: surface.transcript_width(),
                    transcript_list_state: surface.transcript_list_state(),
                    live_scroll: surface.transcript_live_scroll_effect_snapshot(),
                    live_scroll_preserves_anchor_offset: surface
                        .transcript_live_scroll_preserves_anchor_offset(),
                    older_history_loading: surface.older_history_loading(),
                    metrics: tracing::enabled!(tracing::Level::DEBUG)
                        .then(|| surface.transcript_presentation().render_metrics()),
                    activity_caret: surface.transcript_activity_caret(),
                    transcript_edit_mode: surface.transcript_edit_mode_snapshot(),
                    transcript_reset_generation: surface.transcript_reset_generation(),
                    content_release_generation: surface.transcript_content_release_generation(),
                    content_release_row_identities: surface
                        .transcript_content_release_row_identities()
                        .to_vec(),
                    style_snapshot_micros,
                    composer_measurement_micros,
                }
            }),
            ShellState::Discovering(_)
            | ShellState::Picker(_)
            | ShellState::Opening(_)
            | ShellState::WorkspaceIdle(_)
            | ShellState::WorkspaceLoaded(_) => None,
        }
    }
}

use gpui::{App, Context, Window, px, size};

use crate::diagnostic_dynamic_tools::{
    DiagnosticToolSnapshot, ManagedBackendProcessDiagnostic, MemoryDiagnosticSnapshot,
    MemoryDiagnosticUiCorrelation, PreviewStateDiagnostic, ProcessDiagnosticSnapshot,
    RendererDiagnosticSnapshot, SettingsWindowDiagnosticSnapshot, ThemeEditorModelDiagnostic,
    bounded_diagnostic_string, renderer_snapshot_with_shell_window,
};
use crate::memory_diagnostics::{self, RetainedStateSnapshot};

use super::{
    ComposerImagePopupMode, ConfiguredAppState, ConversationSurfaceState, ShellState, ShellView,
    TextInputRetainedAggregate, runtime_target_diagnostic,
};

impl ShellView {
    pub(super) fn retained_state_snapshot(&self) -> RetainedStateSnapshot {
        let mut snapshot = self
            .conversation_surface()
            .map(ConversationSurfaceState::retained_state_snapshot)
            .unwrap_or_default();
        let backend_work_receivers = self.backend_work_receiver_count();
        let backend_client_connection_estimate = self.backend_client_connection_estimate();
        let composer_draft = self.composer_draft.retained_counts();
        let composer_clipboard = self.composer_clipboard.retained_counts();
        snapshot.backend_work_receivers = Some(backend_work_receivers);
        snapshot.backend_event_queue_estimate = Some(backend_client_connection_estimate);
        snapshot.backend_client_connection_estimate = Some(backend_client_connection_estimate);
        snapshot.turn_steering_receivers = Some(self.turn_steering_receivers.len());
        snapshot.composer_draft_text_bytes = Some(composer_draft.display_text_bytes);
        snapshot.composer_draft_images = Some(composer_draft.image_count);
        snapshot.composer_draft_image_bytes = Some(composer_draft.image_bytes);
        snapshot.composer_draft_atoms = Some(composer_draft.atom_count);
        snapshot.composer_draft_atom_bytes = Some(composer_draft.atom_bytes);
        snapshot.composer_clipboard_payloads = Some(composer_clipboard.payloads);
        snapshot.composer_clipboard_text_bytes = Some(
            composer_clipboard
                .selected_text_bytes
                .saturating_add(composer_clipboard.fallback_text_bytes),
        );
        snapshot.composer_clipboard_images = Some(composer_clipboard.image_count);
        snapshot.composer_clipboard_image_bytes = Some(composer_clipboard.image_bytes);
        snapshot.composer_clipboard_atoms = Some(composer_clipboard.atom_count);
        snapshot.composer_clipboard_atom_bytes = Some(composer_clipboard.atom_bytes);
        snapshot.pending_composer_image_asset_paste_bytes = Some(
            self.pending_composer_image_asset_paste
                .as_ref()
                .map_or(0, |pending| {
                    pending
                        .workspace_id
                        .as_str()
                        .len()
                        .saturating_add(pending.display_text_snapshot.len())
                }),
        );
        snapshot.composer_image_popup_bytes =
            Some(self.composer_image_popup.as_ref().map_or(0, |popup| {
                popup
                    .atom_id
                    .len()
                    .saturating_add(popup.label.len())
                    .saturating_add(popup.preview_image_bytes)
            }));
        snapshot.workspace_persistence_pending_work =
            Some(self.workspace_persistence_queue.pending_work_count());
        snapshot.thread_title_workers = Some(self.thread_title_receivers.len());
        snapshot.inventory_worker_active =
            Some(usize::from(self.member_thread_inventory_receiver.is_some()));
        if let Some(total) = snapshot.retained_payload_bytes_lower_bound.as_mut() {
            *total = total
                .saturating_add(composer_draft.display_text_bytes)
                .saturating_add(composer_draft.image_bytes)
                .saturating_add(composer_draft.atom_bytes)
                .saturating_add(composer_clipboard.selected_text_bytes)
                .saturating_add(composer_clipboard.fallback_text_bytes)
                .saturating_add(composer_clipboard.image_bytes)
                .saturating_add(composer_clipboard.atom_bytes)
                .saturating_add(
                    snapshot
                        .pending_composer_image_asset_paste_bytes
                        .unwrap_or_default(),
                )
                .saturating_add(snapshot.composer_image_popup_bytes.unwrap_or_default());
        }
        snapshot
    }

    pub(super) fn handle_prepare_renderer_window_tool_result(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> RendererDiagnosticSnapshot {
        cx.activate(true);
        window.resize(size(px(1040.0), px(760.0)));
        window.activate_window();
        window.refresh();
        self.diagnostic_tool_snapshot(window, cx).renderer
    }

    pub(super) fn diagnostic_tool_snapshot(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DiagnosticToolSnapshot {
        let panel_snapshot = self.transcript_panel.read(cx).diagnostic_snapshot();
        let mut retained_state = self.retained_state_snapshot();
        self.add_text_input_retained_counts(&mut retained_state, cx);
        panel_snapshot.add_retained_counts(&mut retained_state);
        let mut visible_media = panel_snapshot.visible_media;
        visible_media.preview.composer_image_preview = self.composer_image_preview_diagnostic();
        let process = self.process_diagnostic_snapshot();
        let memory = self.memory_diagnostic_snapshot(&process);
        let renderer = renderer_snapshot_with_shell_window(
            process.clone(),
            cx.renderer_diagnostic_snapshot(),
            window.renderer_diagnostic_snapshot(),
        );
        DiagnosticToolSnapshot {
            process,
            memory,
            renderer,
            retained_state,
            visible_media,
            media_events: panel_snapshot.media_events,
            transcript_frame_metrics: panel_snapshot.transcript_frame_metrics,
            settings_window: self.settings_window_diagnostic_snapshot(cx),
        }
    }

    fn settings_window_diagnostic_snapshot(
        &self,
        cx: &mut Context<Self>,
    ) -> SettingsWindowDiagnosticSnapshot {
        self.settings_window
            .diagnostics_snapshot(cx)
            .map(SettingsWindowDiagnosticSnapshot::from)
            .map(|snapshot| {
                snapshot.with_theme_editor_model(
                    self.settings_state
                        .theme_editor_diagnostics_snapshot()
                        .map(ThemeEditorModelDiagnostic::from),
                )
            })
            .unwrap_or_else(|error| {
                SettingsWindowDiagnosticSnapshot::unavailable(error.to_string())
            })
    }

    pub(super) fn process_diagnostic_snapshot(&self) -> ProcessDiagnosticSnapshot {
        let selected_target = match &self.state {
            ShellState::Ready(ready) => Some(&ready.execution_target),
            ShellState::BackendUnavailable(unavailable) => Some(&unavailable.execution_target),
            _ => None,
        };
        let selected_workspace_id = self
            .loaded_workspace()
            .map(|loaded| loaded.workspace.id().as_str().to_string());
        let selected_thread_id = self
            .conversation_surface()
            .and_then(ConversationSurfaceState::selected_thread_id)
            .map(str::to_string);
        let managed_backend_child_pids = self
            .backend_servers
            .iter()
            .filter_map(|(target, server)| {
                server
                    .process_id()
                    .map(|pid| ManagedBackendProcessDiagnostic {
                        pid,
                        runtime_target: runtime_target_diagnostic(target),
                        selected: selected_target.is_some_and(|selected| selected == target),
                    })
            })
            .take(32)
            .collect();

        ProcessDiagnosticSnapshot {
            pid: std::process::id(),
            executable_path: std::env::current_exe()
                .ok()
                .map(|path| bounded_diagnostic_string(path.display().to_string())),
            beryl_home: self
                .app_state
                .as_ref()
                .ok()
                .map(ConfiguredAppState::home_display)
                .map(bounded_diagnostic_string),
            selected_workspace_id: selected_workspace_id.map(bounded_diagnostic_string),
            selected_thread_id: selected_thread_id.map(bounded_diagnostic_string),
            selected_runtime_target: selected_target.map(runtime_target_diagnostic),
            managed_backend_child_pids,
        }
    }

    pub(super) fn memory_diagnostic_snapshot(
        &self,
        process: &ProcessDiagnosticSnapshot,
    ) -> MemoryDiagnosticSnapshot {
        let ui = MemoryDiagnosticUiCorrelation::from_process(process);
        match memory_diagnostics::current_process_memory_snapshot() {
            Ok(counters) => MemoryDiagnosticSnapshot {
                counters: Some(counters),
                unavailable_reason: None,
                ui,
            },
            Err(error) => MemoryDiagnosticSnapshot {
                counters: None,
                unavailable_reason: Some(error.to_string()),
                ui,
            },
        }
    }

    pub(super) fn composer_image_preview_diagnostic(&self) -> Option<PreviewStateDiagnostic> {
        let popup = self.composer_image_popup.as_ref()?;
        let state = match &popup.mode {
            ComposerImagePopupMode::Menu => "menu",
            ComposerImagePopupMode::Preview => {
                if popup.preview_image.is_some() {
                    "loaded"
                } else {
                    "pending"
                }
            }
        };
        Some(PreviewStateDiagnostic {
            state: state.to_string(),
            compressed_bytes: (popup.preview_image_bytes > 0).then_some(popup.preview_image_bytes),
        })
    }

    pub(super) fn add_text_input_retained_counts(
        &self,
        snapshot: &mut RetainedStateSnapshot,
        cx: &App,
    ) {
        let mut counts = TextInputRetainedAggregate::default();
        for input in [
            &self.host_path_input,
            &self.wsl_distro_input,
            &self.wsl_path_input,
            &self.workspace_picker_filter_input,
            &self.workspace_rename_input,
            &self.conversation_input,
            &self.surface_notice_text_input,
        ] {
            counts.add(input.read(cx).retained_counts());
        }

        snapshot.text_input_count = Some(counts.count);
        snapshot.text_input_current_text_bytes = Some(counts.current_text_bytes);
        snapshot.text_input_current_atoms = Some(counts.current_atom_count);
        snapshot.text_input_current_atom_bytes = Some(counts.current_atom_bytes);
        snapshot.text_input_undo_snapshots = Some(counts.undo_snapshot_count);
        snapshot.text_input_redo_snapshots = Some(counts.redo_snapshot_count);
        snapshot.text_input_undo_bytes = Some(counts.undo_bytes);
        snapshot.text_input_redo_bytes = Some(counts.redo_bytes);
        snapshot.text_input_widget_layout_lines = Some(counts.widget_layout_lines);
        snapshot.text_input_widget_visual_lines = Some(counts.widget_visual_lines);
        snapshot.text_input_widget_visible_text_bytes = Some(counts.widget_visible_text_bytes);
        if let Some(total) = snapshot.retained_payload_bytes_lower_bound.as_mut() {
            *total = total.saturating_add(counts.payload_bytes_lower_bound());
        }
    }
}

impl From<super::settings::ThemeEditorDiagnosticsSnapshot> for ThemeEditorModelDiagnostic {
    fn from(snapshot: super::settings::ThemeEditorDiagnosticsSnapshot) -> Self {
        Self {
            candidate_definition_build_count: snapshot.candidate_definition_build_count,
            last_candidate_definition_build_micros: snapshot.last_candidate_definition_build_micros,
            preview_projection_build_count: snapshot.preview_projection_build_count,
            last_preview_projection_build_micros: snapshot.last_preview_projection_build_micros,
            role_preview_style_build_count: snapshot.role_preview_style_build_count,
            role_preview_row_count: snapshot.role_preview_row_count,
            selected_property_detail_row_count: snapshot.selected_property_detail_row_count,
            modified_state_recompute_count: snapshot.modified_state_recompute_count,
            last_modified_state_recompute_micros: snapshot.last_modified_state_recompute_micros,
        }
    }
}

use std::sync::mpsc::TryRecvError;

use gpui::{Context, Window};

use super::{
    ShellView,
    dynamic_theme_worker::{
        DynamicThemeDurableOperation, DynamicThemeDurableResult, DynamicThemeDurableTask,
        DynamicThemeDurableUpdate, spawn_dynamic_theme_durable_worker,
    },
    turn_worker::ShellDynamicToolRequest,
};
use crate::theme_dynamic_tools::{
    ThemeDynamicToolRequest, ThemeSaveAsSource, parse_beryl_theme_dynamic_tool_request,
    theme_authoring_guide_value, theme_document_summary_value, theme_mutation_value,
    theme_preview_value, theme_repository_value, theme_schema_value, theme_tool_failure_response,
    theme_tool_success_response, validate_theme_document_value,
};

#[derive(Clone)]
pub(super) struct DynamicThemePreviewState {
    pub(super) restore_projection: crate::ActiveThemeProjection,
}

impl ShellView {
    pub(super) fn handle_beryl_theme_dynamic_tool_shell_request(
        &mut self,
        request: ShellDynamicToolRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parsed = match parse_beryl_theme_dynamic_tool_request(request.request()) {
            Ok(parsed) => parsed,
            Err(error) => {
                let response =
                    theme_tool_failure_response(request.request(), error.kind(), error.to_string());
                request.respond(response);
                return;
            }
        };

        match parsed {
            ThemeDynamicToolRequest::ReadRepository { .. }
            | ThemeDynamicToolRequest::ReadSchema { .. }
            | ThemeDynamicToolRequest::ReadAuthoringGuide { .. }
            | ThemeDynamicToolRequest::ValidateDocument { .. }
            | ThemeDynamicToolRequest::Preview { .. }
            | ThemeDynamicToolRequest::StopPreview => {
                let response = match self.handle_beryl_theme_immediate_tool_result(parsed, cx) {
                    Ok(result) => theme_tool_success_response(request.request(), result),
                    Err((kind, message)) => {
                        theme_tool_failure_response(request.request(), kind, message)
                    }
                };
                request.respond(response);
            }
            ThemeDynamicToolRequest::Install { name, document } => self
                .begin_dynamic_theme_durable_tool_request(
                    request,
                    DynamicThemeDurableOperation::Install { name, document },
                    window,
                    cx,
                ),
            ThemeDynamicToolRequest::Update { theme_id, document } => self
                .begin_dynamic_theme_durable_tool_request(
                    request,
                    DynamicThemeDurableOperation::Update { theme_id, document },
                    window,
                    cx,
                ),
            ThemeDynamicToolRequest::SaveAs { name, source } => self
                .begin_dynamic_theme_durable_tool_request(
                    request,
                    DynamicThemeDurableOperation::SaveAs { name, source },
                    window,
                    cx,
                ),
            ThemeDynamicToolRequest::Activate { theme_id } => self
                .begin_dynamic_theme_durable_tool_request(
                    request,
                    DynamicThemeDurableOperation::Activate { theme_id },
                    window,
                    cx,
                ),
        }
    }

    fn handle_beryl_theme_immediate_tool_result(
        &mut self,
        parsed: ThemeDynamicToolRequest,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, (&'static str, String)> {
        match parsed {
            ThemeDynamicToolRequest::ReadRepository {
                include_active_document,
            } => theme_repository_value(
                self.settings_state.theme_repository_snapshot(),
                include_active_document,
            )
            .map_err(|error| (error.kind(), error.to_string())),
            ThemeDynamicToolRequest::ReadSchema { role_prefix, limit } => {
                theme_schema_value(role_prefix.as_deref(), limit)
                    .map_err(|error| (error.kind(), error.to_string()))
            }
            ThemeDynamicToolRequest::ReadAuthoringGuide {
                section,
                role_prefix,
                limit,
            } => Ok(theme_authoring_guide_value(
                section,
                role_prefix.as_deref(),
                limit,
            )),
            ThemeDynamicToolRequest::ValidateDocument {
                document,
                include_summary,
                explain_roles,
                role_explanation_limit,
            } => Ok(validate_theme_document_value(
                &document,
                include_summary,
                &explain_roles,
                role_explanation_limit,
                self.settings_state.theme_repository_snapshot(),
            )),
            ThemeDynamicToolRequest::Preview { document } => {
                self.handle_dynamic_theme_preview(document, cx)
            }
            ThemeDynamicToolRequest::StopPreview => Ok(self.stop_dynamic_theme_preview(cx)),
            ThemeDynamicToolRequest::Install { .. }
            | ThemeDynamicToolRequest::Update { .. }
            | ThemeDynamicToolRequest::SaveAs { .. }
            | ThemeDynamicToolRequest::Activate { .. } => Err((
                "internal_tool_routing_error",
                "durable theme dynamic tool reached immediate handler".to_string(),
            )),
        }
    }

    fn handle_dynamic_theme_preview(
        &mut self,
        document: crate::ThemeDocument,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, (&'static str, String)> {
        self.reject_duplicate_embedded_theme_id(&document)?;
        if let Some(restore_projection) = self.theme_candidate_state.stop_active_preview()
            && self.dynamic_theme_preview.is_none()
        {
            self.publish_active_theme_projection(restore_projection);
        }
        let definition = document.definition().clone();
        let projection = projection_for_theme_definition(definition)?;
        let restore_projection = self
            .dynamic_theme_preview
            .as_ref()
            .map(|preview| preview.restore_projection.clone())
            .unwrap_or_else(|| self.active_theme_projection());
        self.publish_active_theme_projection(projection.clone());
        self.dynamic_theme_preview = Some(DynamicThemePreviewState { restore_projection });
        self.refresh_theme_candidate_surfaces(cx);
        Ok(serde_json::json!({
            "preview": theme_preview_value(&projection, document.name(), false),
            "document": theme_document_summary_value(&document),
        }))
    }

    fn stop_dynamic_theme_preview(&mut self, cx: &mut Context<Self>) -> serde_json::Value {
        let Some(preview) = self.dynamic_theme_preview.take() else {
            return serde_json::json!({ "stopped": false });
        };
        self.publish_active_theme_projection(preview.restore_projection);
        self.refresh_theme_candidate_surfaces(cx);
        serde_json::json!({ "stopped": true })
    }

    fn begin_dynamic_theme_durable_tool_request(
        &mut self,
        request: ShellDynamicToolRequest,
        operation: DynamicThemeDurableOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dynamic_theme_durable_receiver.is_some() {
            let response = theme_tool_failure_response(
                request.request(),
                "theme_tool_busy",
                "Another durable theme dynamic-tool operation is already running.",
            );
            request.respond(response);
            return;
        }
        if let Err((kind, message)) = self.validate_dynamic_theme_durable_operation(&operation) {
            let response = theme_tool_failure_response(request.request(), kind, message);
            request.respond(response);
            return;
        }
        let store = match self.theme_repository_store() {
            Ok(store) => store,
            Err((kind, message)) => {
                let response = theme_tool_failure_response(request.request(), kind, message);
                request.respond(response);
                return;
            }
        };

        self.dynamic_theme_durable_receiver = Some(DynamicThemeDurableTask {
            request,
            receiver: spawn_dynamic_theme_durable_worker(operation, store),
        });
        self.schedule_poll_if_needed(window, cx);
    }

    fn validate_dynamic_theme_durable_operation(
        &self,
        operation: &DynamicThemeDurableOperation,
    ) -> Result<(), (&'static str, String)> {
        match operation {
            DynamicThemeDurableOperation::Install { document, .. } => {
                self.reject_duplicate_embedded_theme_id(document)
            }
            DynamicThemeDurableOperation::Update { theme_id, document } => {
                self.reject_theme_draft_conflict()?;
                if let Some(embedded_id) = document.id()
                    && embedded_id != theme_id
                {
                    return Err((
                        "invalid_theme_document",
                        "theme document embedded id must match themeId for update".to_string(),
                    ));
                }
                if theme_id.as_str() == crate::BUILT_IN_INSTALLED_THEME_ID {
                    return Err((
                        "theme_repository_error",
                        "the built-in fallback theme cannot be modified".to_string(),
                    ));
                }
                Ok(())
            }
            DynamicThemeDurableOperation::SaveAs { source, .. } => {
                self.reject_theme_draft_conflict()?;
                if let ThemeSaveAsSource::Document(document) = source {
                    self.reject_duplicate_embedded_theme_id(document)?;
                }
                Ok(())
            }
            DynamicThemeDurableOperation::Activate { .. } => self.reject_theme_draft_conflict(),
        }
    }

    pub(super) fn poll_dynamic_theme_durable_updates(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(task) = self.dynamic_theme_durable_receiver.take() else {
            return false;
        };

        match task.receiver.try_recv() {
            Ok(update) => {
                self.finish_dynamic_theme_durable_update(task.request, update, cx);
                true
            }
            Err(TryRecvError::Empty) => {
                self.dynamic_theme_durable_receiver = Some(task);
                false
            }
            Err(TryRecvError::Disconnected) => {
                let response = theme_tool_failure_response(
                    task.request.request(),
                    "theme_repository_error",
                    "Theme dynamic-tool repository worker stopped before returning a result.",
                );
                task.request.respond(response);
                true
            }
        }
    }

    fn finish_dynamic_theme_durable_update(
        &mut self,
        request: ShellDynamicToolRequest,
        update: DynamicThemeDurableUpdate,
        cx: &mut Context<Self>,
    ) {
        let response = match update.result {
            Ok(result) => {
                let value = self.apply_dynamic_theme_durable_success(update.operation, result, cx);
                theme_tool_success_response(request.request(), value)
            }
            Err((kind, message)) => theme_tool_failure_response(request.request(), kind, message),
        };
        request.respond(response);
    }

    fn apply_dynamic_theme_durable_success(
        &mut self,
        operation: DynamicThemeDurableOperation,
        result: DynamicThemeDurableResult,
        cx: &mut Context<Self>,
    ) -> serde_json::Value {
        let DynamicThemeDurableResult { snapshot, changed } = result;
        match operation {
            DynamicThemeDurableOperation::Install { document, .. } => {
                self.settings_state
                    .record_theme_repository_snapshot(snapshot.clone());
                self.sync_settings_window_model(cx);
                cx.notify();
                serde_json::json!({
                    "installed": true,
                    "activated": false,
                    "document": theme_document_summary_value(&document),
                    "repository": theme_mutation_value(&snapshot, changed),
                })
            }
            DynamicThemeDurableOperation::Update { document, .. } => {
                self.apply_dynamic_theme_repository_snapshot(snapshot.clone(), true, cx);
                serde_json::json!({
                    "updated": true,
                    "document": theme_document_summary_value(&document),
                    "repository": theme_mutation_value(&snapshot, changed),
                })
            }
            DynamicThemeDurableOperation::SaveAs { source, .. } => {
                let document_summary = match source {
                    ThemeSaveAsSource::Document(document) => {
                        Some(theme_document_summary_value(&document))
                    }
                    ThemeSaveAsSource::ExistingTheme(_) => None,
                };
                self.apply_dynamic_theme_repository_snapshot(snapshot.clone(), true, cx);
                serde_json::json!({
                    "savedAs": true,
                    "document": document_summary,
                    "repository": theme_mutation_value(&snapshot, changed),
                })
            }
            DynamicThemeDurableOperation::Activate { .. } => {
                self.apply_dynamic_theme_repository_snapshot(snapshot.clone(), true, cx);
                serde_json::json!({
                    "activated": true,
                    "repository": theme_mutation_value(&snapshot, changed),
                })
            }
        }
    }

    fn theme_repository_store(
        &self,
    ) -> Result<crate::ThemeRepositoryStore, (&'static str, String)> {
        self.app_state
            .as_ref()
            .ok()
            .map(|state| state.home_dir.theme_repository_store())
            .ok_or_else(|| {
                (
                    "theme_storage_unavailable",
                    "Beryl theme storage is unavailable for the configured home directory."
                        .to_string(),
                )
            })
    }

    fn reject_duplicate_embedded_theme_id(
        &self,
        document: &crate::ThemeDocument,
    ) -> Result<(), (&'static str, String)> {
        if let Some(id) = document.id()
            && self
                .settings_state
                .theme_repository_snapshot()
                .themes()
                .iter()
                .any(|theme| theme.id() == id)
        {
            return Err((
                "duplicate_theme_id",
                format!(
                    "theme document embedded id `{}` is already installed",
                    id.as_str()
                ),
            ));
        }
        Ok(())
    }

    fn reject_theme_draft_conflict(&self) -> Result<(), (&'static str, String)> {
        if self
            .settings_state
            .theme_draft_modified_for_external_change()
        {
            return Err((
                "settings_draft_conflict",
                "The settings window has unapplied theme edits. Save or discard them before CAS theme writes."
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn apply_dynamic_theme_repository_snapshot(
        &mut self,
        snapshot: crate::ThemeRepositorySnapshot,
        reset_settings_draft: bool,
        cx: &mut Context<Self>,
    ) {
        self.publish_active_theme_projection(snapshot.active_projection().clone());
        self.dynamic_theme_preview = None;
        self.theme_candidate_state
            .clear_after_durable_theme_change();
        if reset_settings_draft {
            self.settings_state
                .apply_theme_repository_snapshot_from_external(snapshot);
        } else {
            self.settings_state
                .record_theme_repository_snapshot(snapshot);
        }
        self.sync_settings_window_model(cx);
        self.refresh_theme_candidate_surfaces(cx);
    }
}

fn projection_for_theme_definition(
    definition: crate::ThemeDefinition,
) -> Result<crate::ActiveThemeProjection, (&'static str, String)> {
    let resolver = crate::ThemeResolver::new(crate::built_in_theme_schema(), definition)
        .map_err(|source| ("invalid_theme_document", source.to_string()))?;
    crate::ActiveThemeProjection::from_built_in_resolver(resolver)
        .map_err(|source| ("invalid_theme_document", source.to_string()))
}

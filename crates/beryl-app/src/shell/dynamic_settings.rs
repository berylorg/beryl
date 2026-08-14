use gpui::Context;

use super::ShellView;
use crate::settings_dynamic_tools::{
    SettingsDynamicToolRequest, gui_settings_snapshot_value,
    parse_beryl_settings_dynamic_tool_request, settings_tool_failure_response,
    settings_tool_success_response, settings_update_value, settings_validation_value,
};

impl ShellView {
    pub(super) fn handle_beryl_settings_dynamic_tool_request(
        &mut self,
        request: &beryl_backend::DynamicToolCallRequest,
        cx: &mut Context<Self>,
    ) -> beryl_backend::DynamicToolCallResponse {
        let parsed = match parse_beryl_settings_dynamic_tool_request(request) {
            Ok(parsed) => parsed,
            Err(error) => {
                return settings_tool_failure_response(request, error.kind(), error.to_string());
            }
        };

        match parsed {
            SettingsDynamicToolRequest::Read => settings_tool_success_response(
                request,
                gui_settings_snapshot_value(
                    &self.settings_state.active_preferences_snapshot(),
                    self.settings_state.theme_repository_snapshot(),
                ),
            ),
            SettingsDynamicToolRequest::Validate { update } => {
                let current = self.settings_state.active_preferences_snapshot();
                match settings_validation_value(&current, &update) {
                    Ok(result) => settings_tool_success_response(request, result),
                    Err(message) => {
                        settings_tool_failure_response(request, message.kind(), message.to_string())
                    }
                }
            }
            SettingsDynamicToolRequest::Update { update } => {
                let current = self.settings_state.active_preferences_snapshot();
                match update.apply_to(&current) {
                    Ok(preferences) => match self
                        .settings_state
                        .apply_preferences_from_external(preferences.clone())
                    {
                        Ok(changed) => {
                            self.sync_settings_window_model(cx);
                            if changed {
                                self.schedule_settings_save_poll(cx);
                            }
                            cx.notify();
                            settings_tool_success_response(
                                request,
                                settings_update_value(
                                    &preferences,
                                    changed,
                                    self.settings_state.has_pending_save(),
                                ),
                            )
                        }
                        Err(message) => settings_tool_failure_response(
                            request,
                            "settings_update_rejected",
                            message,
                        ),
                    },
                    Err(error) => {
                        self.sync_settings_window_model(cx);
                        settings_tool_failure_response(request, error.kind(), error.to_string())
                    }
                }
            }
        }
    }
}

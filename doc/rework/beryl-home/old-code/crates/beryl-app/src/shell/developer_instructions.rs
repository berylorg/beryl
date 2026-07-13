use beryl_backend::TurnStartOptions;
use tracing::warn;

use super::{ShellView, status_line::ThreadTurnDefaults};

impl ShellView {
    fn current_developer_instructions_preference(&self) -> Option<String> {
        match self.gui_preferences.lock() {
            Ok(preferences) => preferences.agent.developer_instructions.clone(),
            Err(error) => {
                warn!(
                    error = %error,
                    "failed to read developer-instructions preferences"
                );
                None
            }
        }
    }

    fn current_hidden_developer_instructions(&self) -> Option<String> {
        self.current_developer_instructions_preference()
    }

    pub(in crate::shell) fn turn_options_with_current_developer_instructions(
        &self,
        selected_thread_id: Option<&str>,
        options: TurnStartOptions,
    ) -> TurnStartOptions {
        let Some(defaults) = self
            .conversation_surface()
            .map(|surface| surface.effective_turn_context_defaults(selected_thread_id))
        else {
            return options;
        };
        self.turn_options_with_current_developer_instructions_defaults(
            selected_thread_id,
            options,
            defaults,
        )
    }

    pub(in crate::shell) fn turn_options_with_current_developer_instructions_defaults(
        &self,
        selected_thread_id: Option<&str>,
        options: TurnStartOptions,
        defaults: ThreadTurnDefaults,
    ) -> TurnStartOptions {
        let Some(_model) = defaults.model() else {
            warn!(
                thread_id = selected_thread_id.unwrap_or("<new-thread>"),
                "hidden developer-instructions context could not be applied or reset because no effective model is known for turn-start collaboration settings"
            );
            return options.without_developer_instructions_context();
        };

        super::status_line::turn_start_options_with_developer_instructions_context(
            options,
            self.current_hidden_developer_instructions(),
            defaults,
        )
    }
}

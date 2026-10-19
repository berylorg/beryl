use super::*;

impl MainWindowShellRoot {
    pub(crate) fn diagnostic_request_thread_activation(
        &mut self,
        thread: beryl_model::SyndicThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
        accepted: impl FnOnce(Result<&'static str, String>, &mut Window, &mut Context<Self>) + 'static,
    ) -> Result<(), String> {
        self.request_ordinary_thread_activation(thread, window, cx, move |result, window, cx| {
            accepted(
                result.map(|acceptance| match acceptance {
                    OrdinaryThreadActivationAcceptance::Current => "current",
                    OrdinaryThreadActivationAcceptance::Admitted => "opening",
                }),
                window,
                cx,
            );
        })
    }

    pub(crate) fn diagnostic_transcript_facts(&self, app: &App, limit: usize) -> serde_json::Value {
        let failure = self
            .running_threads
            .activation_failure
            .as_ref()
            .map(|(thread, error)| serde_json::json!({ "threadId": thread, "message": error }));
        serde_json::json!({
            "windowId": self.controller.as_ref().map(|controller| controller.window_id()),
            "selectedThreadId": self.diagnostic_selected_thread(app),
            "pendingThreadId": self.running_threads.pending_activation,
            "activationPending": self.running_threads.has_activation_custody(),
            "activationFailure": failure,
            "resourcePreparationPending": self.running_threads.diagnostic_resource_preparation_pending()
                || self.running_threads.transcript.read(app).resource_demand().is_some(),
            "transcript": self.running_threads.transcript.read(app).diagnostic_ui_facts(limit),
            "activity": self.diagnostic_activity_facts(app),
        })
    }
}

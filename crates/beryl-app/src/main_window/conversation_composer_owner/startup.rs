use super::*;

impl MainWindowConversationComposer {
    pub fn release_startup_widget(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<
        impl std::future::Future<Output = Result<MainWindowComposerWidgetRelease, String>> + use<>,
        String,
    > {
        if !self.startup_interaction_gated
            || self.startup_release_started
            || self.route != MainWindowConversationComposerRoute::Selected
            || self.window_close.is_some()
            || !matches!(self.phase, MainWindowConversationComposerPhase::Live)
        {
            return Err("startup composer is not available for release".to_owned());
        }
        let (completion, receipt) = futures_channel::oneshot::channel();
        self.startup_release_started = true;
        self.startup_release_completion = Some(completion);
        match self.begin_widget_release_fence(window, cx) {
            Ok(_) => self.advance_startup_release(window, cx),
            Err(error) => self.complete_startup_release(Err(error)),
        }
        Ok(async move {
            receipt
                .await
                .map_err(|_| "startup composer release completion was lost".to_owned())?
        })
    }

    fn complete_startup_release(
        &mut self,
        result: Result<MainWindowComposerWidgetRelease, String>,
    ) {
        if let Some(completion) = self.startup_release_completion.take() {
            let _ = completion.send(result);
        }
    }

    pub(super) fn fail_startup_release_if_needed(&mut self) {
        if self.startup_release_completion.is_some()
            && let Some(error) = self.last_error.clone()
        {
            self.complete_startup_release(Err(error));
        }
    }

    pub(super) fn advance_startup_release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fail_startup_release_if_needed();
        if self.startup_release_completion.is_some() && self.widget_release_ready(cx) {
            let service = match self.bound_service() {
                Ok(service) => service,
                Err(error) => {
                    self.complete_startup_release(Err(error));
                    return;
                }
            };
            let selection = self.selection;
            let result = self.release_widget_with(window, cx, |requests| {
                service.release_startup_widget_work(selection, requests)
            });
            self.complete_startup_release(result);
        }
    }

    pub(in crate::main_window) fn startup_interaction_gated(&self) -> bool {
        self.startup_interaction_gated
    }

    pub(in crate::main_window) fn gate_startup_interaction(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.startup_interaction_gated = true;
        self.input.update(cx, |input, input_cx| {
            input.set_read_only(true, input_cx);
            input.set_enabled(false, input_cx);
            if input.is_enabled() {
                Err("startup composer could not disable input".to_owned())
            } else {
                Ok(())
            }
        })
    }

    pub(in crate::main_window) fn prepare_startup_interaction_release(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.startup_interaction_gated
            || self.startup_release_started
            || self.window_close.is_some()
            || !self.selected_first_presentable(cx)
        {
            return Err("startup composer is not ready for interaction".to_owned());
        }
        self.input.update(cx, |input, input_cx| {
            input.set_enabled(true, input_cx);
            if input.is_enabled() {
                Ok(())
            } else {
                Err("startup composer could not enable input".to_owned())
            }
        })
    }

    pub(in crate::main_window) fn commit_startup_interaction_release(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.startup_interaction_gated = false;
        self.sync_mutation_gate(cx);
    }
}

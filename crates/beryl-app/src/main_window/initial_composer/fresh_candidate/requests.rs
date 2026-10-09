use super::*;
use crate::main_window::MainWindowFreshClaimWidgetReply;
use crate::main_window::composer_slot::dispatch::translate;

impl MainWindowFreshComposerPreparation {
    pub(crate) fn prepare_widget_batch(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        reply: &mut MainWindowFreshClaimWidgetReply,
        cancellation: &CommandCancellation,
    ) -> Result<(), String> {
        let batch = reply.batch().clone();
        let selection = batch.close().selection();
        if !self.is_ordinary_claim()
            || batch.requests().is_empty()
            || batch.requests().len() > 16
            || self.selection() != Some(selection)
            || selection.window_id() != self.window.window_id()
            || self
                .candidate
                .service
                .as_ref()
                .is_none_or(|service| !service.window_close_is_current(batch.close()))
        {
            return Err("fresh ordinary request batch differs from its prepared source".into());
        }
        let access = candidate
            .recovery_access()
            .map_err(|error| error.to_string())?;
        self.validate_access(&access)?;
        for request in &batch.requests()[reply.prepared()..] {
            if cancellation.is_cancelled() {
                return Err("fresh ordinary request batch cancelled".into());
            }
            self.validate_access(&access)?;
            let response = match request {
                gpui_text_input::RangeTextInputRequest::Page(request) => {
                    crate::main_window::MainWindowComposerDispatchOutcome::Page(
                        translate::candidate_text_page(
                            &self.candidate.storage,
                            &access,
                            selection.binding(),
                            *request,
                        )
                        .map_err(|error| error.to_string())?,
                    )
                }
                gpui_text_input::RangeTextInputRequest::ObjectPage(request) => {
                    crate::main_window::MainWindowComposerDispatchOutcome::ObjectPage(
                        translate::candidate_object_page(
                            &self.candidate.storage,
                            &access,
                            selection.binding(),
                            *request,
                        )
                        .map_err(|error| error.to_string())?,
                    )
                }
                gpui_text_input::RangeTextInputRequest::CancelPage(_)
                | gpui_text_input::RangeTextInputRequest::ReleasePage(_)
                | gpui_text_input::RangeTextInputRequest::CancelObjectPage(_)
                | gpui_text_input::RangeTextInputRequest::ReleaseObjectPage(_) => {
                    self.candidate
                        .service
                        .as_ref()
                        .unwrap()
                        .release_fresh_candidate_widget(selection, std::slice::from_ref(request))?;
                    crate::main_window::MainWindowComposerDispatchOutcome::Released
                }
                _ => {
                    return Err(
                        "fresh ordinary candidate refuses an interactive mutation request".into(),
                    );
                }
            };
            self.validate_access(&access)?;
            if cancellation.is_cancelled() {
                return Err("fresh ordinary request batch cancelled after read".into());
            }
            reply.push(response);
        }
        self.validate_access(&access)
    }
}

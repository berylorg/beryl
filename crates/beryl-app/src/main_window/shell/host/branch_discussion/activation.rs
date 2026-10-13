use super::*;

impl MainWindowShellRoot {
    pub(super) fn activate_captured_branch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(operation) = self.branch_discussion.operation.clone() else {
            return;
        };
        let (created, parent) = {
            let state = operation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (state.created, state.captured.captured.source.thread_id())
        };
        let Some(created) = created else {
            return;
        };
        #[cfg(all(test, feature = "test-faults"))]
        {
            operation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .activation_attempts += 1;
        }
        if self
            .cached_running_selection(cx)
            .is_none_or(|(selection, _)| selection.claim().thread_id() != parent)
        {
            self.finish_captured_branch_activation(
                Err("The original source view changed before discussion activation.".into()),
                false,
                window,
                cx,
            );
            return;
        }
        if let Err(error) =
            self.request_ordinary_thread_activation(created.thread_id, window, cx, |_, _, _| {})
        {
            self.finish_captured_branch_activation(Err(error), false, window, cx);
        }
    }

    pub(super) fn retry_captured_branch_activation(
        &mut self,
        captured: &crate::syndic_transcript::CapturedDiscussionSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.running_threads.views_retired
            || self.running_threads.reads_suspended
            || self.running_selection_interaction_gated(cx)
            || self.ordinary_close_interaction_gated
            || self.shutdown_interaction_gated
            || self.startup_interaction_gated()
            || self
                .running_threads
                .transcript
                .read(cx)
                .discussion_menu_source()
                .as_ref()
                != Some(captured)
            || self
                .cached_running_selection(cx)
                .is_none_or(|(selection, _)| {
                    selection.claim().thread_id() != captured.source.thread_id()
                })
        {
            return;
        }
        if self
            .branch_discussion
            .operation
            .as_ref()
            .is_none_or(|operation| {
                operation
                    .try_lock()
                    .is_ok_and(|state| &state.captured.captured != captured)
            })
        {
            let Some(index) = self
                .branch_discussion
                .retained
                .iter()
                .position(|operation| {
                    operation.try_lock().is_ok_and(|state| {
                        &state.captured.captured == captured && state.stage == BranchStage::Retry
                    })
                })
            else {
                return;
            };
            if let Some(previous) = self.branch_discussion.operation.take() {
                self.branch_discussion.retained.push(previous);
            }
            self.branch_discussion.operation = Some(self.branch_discussion.retained.remove(index));
        }
        let Some(operation) = self.branch_discussion.operation.clone() else {
            return;
        };
        let mut state = operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.stage != BranchStage::Retry
            || self.ordinary_close_interaction_gated
            || self.shutdown_interaction_gated
            || self.startup_interaction_gated()
        {
            return;
        }
        state.stage = BranchStage::Activating;
        drop(state);
        self.publish_captured_branch_state(BranchStage::Activating, cx);
        self.activate_captured_branch(window, cx);
    }

    pub(in crate::main_window::shell::host) fn finish_captured_branch_activation(
        &mut self,
        result: Result<(), String>,
        unavailable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(operation) = self.branch_discussion.operation.clone() else {
            return;
        };
        let mut state = operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.stage != BranchStage::Activating {
            return;
        }
        state.stage = if unavailable {
            BranchStage::Unavailable
        } else if result.is_err() {
            BranchStage::Retry
        } else {
            BranchStage::Complete
        };
        state.error = result.as_ref().err().cloned();
        let stage = state.stage;
        drop(state);
        self.publish_captured_branch_state(stage, cx);
        self.running_threads.transcript.update(cx, |panel, _| {
            panel.finish_captured_discussion_selection(stage == BranchStage::Complete, window)
        });
        if let Err(error) = result {
            self.report_captured_branch_failure(&error, window, cx);
        }
        if stage == BranchStage::Complete {
            let token = operation
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .notice
                .take();
            self.branch_discussion.operation = None;
            self.remove_running_activation_failure(token);
            self.sync_notices(window, cx);
        }
    }
}

use super::*;

impl MainWindowConversationComposer {
    pub fn gpui_input(&self) -> Entity<RangeTextInput> {
        self.input.clone()
    }

    pub const fn selection_identity(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub fn synchronize_lifecycle_selection(
        &mut self,
        expected: MainWindowComposerSelectionIdentity,
        successor: MainWindowComposerSelectionIdentity,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.selection != expected
            || expected.binding().range_binding() != successor.binding().range_binding()
        {
            return Err("composer lifecycle selection changed its editor binding".to_owned());
        }
        self.selection = successor;
        self.image_surfaces.selection_changed(successor);
        self.input
            .update(cx, |input, _| {
                input.set_history_frontier(
                    input.history_frontier(),
                    successor.binding().range_history_frontier(),
                )
            })
            .map_err(|_| "composer input rebind was rejected".to_owned())
    }

    pub fn release_widget(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<MainWindowComposerWidgetRelease, String> {
        match self.phase {
            MainWindowConversationComposerPhase::Released(release) => return Ok(release),
            MainWindowConversationComposerPhase::Fencing => {}
            MainWindowConversationComposerPhase::Live => {
                return Err("conversation composer widget must be fenced before release".to_owned());
            }
            MainWindowConversationComposerPhase::Releasing => {
                return Err("conversation composer widget release is already active".to_owned());
            }
            MainWindowConversationComposerPhase::ReleaseFailed => {
                return Err("conversation composer widget release previously failed".to_owned());
            }
        }
        if !self.widget_release_ready(cx) {
            return Err(
                "conversation composer widget release is waiting for quiescence".to_owned(),
            );
        }
        self.phase = MainWindowConversationComposerPhase::Releasing;
        self.scheduled = false;
        if let Some(clipboard) = self.propagated_clipboard.take() {
            clipboard.cancel();
        }
        self.propagated_cut = None;
        self.pending_marker_metadata = None;
        self.pending_marker_removal = None;
        self.image_surface_attachment = None;
        self.image_surfaces.clear();
        self.admitted_positions = None;
        let requests = self
            .input
            .update(cx, |input, input_cx| input.dispose(window, input_cx));
        match self.service.release_widget_work(self.selection, requests) {
            Ok(release) => {
                self.phase = MainWindowConversationComposerPhase::Released(release);
                Ok(release)
            }
            Err(error) => {
                self.phase = MainWindowConversationComposerPhase::ReleaseFailed;
                self.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub(super) fn is_live(&self) -> bool {
        matches!(self.phase, MainWindowConversationComposerPhase::Live)
    }

    pub(super) fn can_pump(&self) -> bool {
        matches!(
            self.phase,
            MainWindowConversationComposerPhase::Live
                | MainWindowConversationComposerPhase::Fencing
        )
    }

    pub fn begin_widget_release_fence(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        match self.phase {
            MainWindowConversationComposerPhase::Live => {
                self.phase = MainWindowConversationComposerPhase::Fencing;
                if let Some(clipboard) = self.propagated_clipboard.take() {
                    clipboard.cancel();
                }
                self.image_surfaces.clear();
                if let Some(attachment) = self.image_surface_attachment.take()
                    && let Err(error) = self.input.update(cx, |input, input_cx| {
                        input.dismiss_active_inline_object_surface(
                            attachment,
                            InlineObjectSurfaceDismissal::ClearObject,
                            window,
                            input_cx,
                        )
                    })
                    && !matches!(error, gpui_text_input::RangeTextInputError::Stale)
                {
                    return Err("composer marker surface dismissal was rejected".into());
                }
                self.input
                    .update(cx, |input, input_cx| input.set_enabled(false, input_cx));
                self.schedule_pump(window, cx);
            }
            MainWindowConversationComposerPhase::Fencing => {}
            MainWindowConversationComposerPhase::Releasing => {
                return Err("conversation composer widget release is already active".to_owned());
            }
            MainWindowConversationComposerPhase::Released(_) => return Ok(true),
            MainWindowConversationComposerPhase::ReleaseFailed => {
                return Err("conversation composer widget release previously failed".to_owned());
            }
        }
        Ok(self.widget_release_ready(cx))
    }

    pub fn widget_release_ready(&self, cx: &mut Context<Self>) -> bool {
        matches!(self.phase, MainWindowConversationComposerPhase::Fencing)
            && self.active_flight.is_none()
            && self.last_error.is_none()
            && self.input.update(cx, |input, _| input.is_quiescent())
    }

    pub fn resume_after_widget_release_fence(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !matches!(self.phase, MainWindowConversationComposerPhase::Fencing) {
            return Err("conversation composer widget is not fenced".to_owned());
        }
        self.phase = MainWindowConversationComposerPhase::Live;
        self.input
            .update(cx, |input, input_cx| input.set_enabled(true, input_cx));
        self.schedule_pump(window, cx);
        Ok(())
    }
}

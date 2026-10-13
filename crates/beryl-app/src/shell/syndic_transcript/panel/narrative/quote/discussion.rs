use super::*;

impl SyndicTranscriptPanel {
    pub(in super::super::super) fn restore_captured_discussion_selection(
        &mut self,
        window: &Window,
    ) {
        if !(self.quote_state.branch_pending || self.quote_state.restore_branch_selection)
            || self.narrative_selection.is_some()
            || !self.character_geometry_current(window)
        {
            return;
        }
        let Some((original, captured, environment)) = &self.quote_state.preserved_branch_selection
        else {
            return;
        };
        if self
            .geometry
            .as_ref()
            .is_none_or(|geometry| &geometry.environment != environment)
        {
            return;
        }
        let snapshot = self.host.shared_snapshot();
        let mut selection = original.clone();
        selection.activation = snapshot.activation_revision;
        selection.revision = snapshot.presentation_revision;
        let Some(display_ranges) = self.narrative_ranges(&selection) else {
            return;
        };
        let ranges = display_ranges
            .into_iter()
            .map(|(id, display)| {
                self.mapped_geometry(&id)
                    .and_then(|geometry| geometry.source_range(display))
                    .map(|range| (id, range))
            })
            .collect::<Option<Vec<_>>>();
        let Some(ranges) = ranges else {
            return;
        };
        if self
            .host
            .mapped_discussion_source(selection.activation, selection.revision, &ranges)
            .as_ref()
            != Some(captured)
        {
            return;
        }
        self.narrative_selection = Some(selection);
        self.quote_state.restore_branch_selection = false;
    }

    pub(crate) fn finish_captured_discussion_selection(
        &mut self,
        success: bool,
        window: &mut Window,
    ) {
        if success {
            self.quote_state.preserved_branch_selection = None;
            self.quote_state.preserved_code_selection = None;
            self.quote_state.restore_branch_selection = false;
        } else {
            self.quote_state.restore_branch_selection = true;
            if self.quote_state.preserved_code_selection.is_some() {
                self.restore_code_discussion_selection(window);
            } else if self.quote_state.branch_label == Some("Retry") {
                self.focus_handle.focus(window);
            }
        }
    }

    pub(crate) fn finish_captured_discussion_noncommit(&mut self, window: &mut Window) {
        self.close_quote_menu(window);
        self.invalidate_transcript_menu();
    }

    pub(in super::super::super) fn invalidate_captured_discussion_geometry(&mut self) {
        self.quote_state.preserved_branch_selection = None;
        self.quote_state.restore_branch_selection = false;
    }

    pub(crate) fn discussion_retry_request_current(
        &self,
        request: &crate::syndic_transcript::DiscussionCreationRequested,
        window: &Window,
    ) -> bool {
        self.quote_state.popup.is_some()
            && self.quote_state.branch_retry
            && self.quote_request_current(
                request.request,
                request.activation,
                request.presentation,
                window,
            )
            && self.discussion_menu_source().as_ref() == Some(&request.captured)
    }
    pub(in super::super::super) fn captured_discussion_interaction_gated(&self) -> bool {
        self.quote_state.branch_pending
    }
    pub(crate) fn accept_discussion_source_qualification(
        &mut self,
        request: &crate::syndic_transcript::DiscussionCreationRequested,
        result: Result<(), &'static str>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.quote_request_current(
            request.request,
            request.activation,
            request.presentation,
            window,
        ) {
            return;
        }
        self.quote_state.branch_qualified = Some(result);
        let rows = self.transcript_menu_rows();
        if let Some(popup) = &mut self.quote_state.popup {
            popup.reconcile(&rows);
        }
        cx.notify();
    }

    pub(crate) fn discussion_menu_source(
        &self,
    ) -> Option<crate::syndic_transcript::CapturedDiscussionSource> {
        if let Some(capture) = &self.quote_state.code_capture {
            return Some(capture.captured.clone());
        }
        let capture = self.quote_state.capture.as_ref()?;
        self.host.mapped_discussion_source(
            capture.selection.activation,
            capture.selection.revision,
            &capture.ranges,
        )
    }
    pub(crate) fn set_discussion_creation_state(
        &mut self,
        source: Option<crate::syndic_transcript::CapturedDiscussionSource>,
        label: Option<&'static str>,
        reason: Option<&'static str>,
        retry: bool,
        cx: &mut Context<Self>,
    ) {
        let pending = matches!(
            label,
            Some("Creating discussion…" | "Reconciling discussion…" | "Opening discussion…")
        );
        if self.quote_state.branch_label == label
            && self.quote_state.branch_disabled_reason == reason
            && self.quote_state.branch_retry == retry
            && self.quote_state.branch_source == source
            && self.quote_state.branch_pending == pending
        {
            return;
        }
        self.quote_state.branch_label = label;
        self.quote_state.branch_disabled_reason = reason;
        self.quote_state.branch_retry = retry;
        self.quote_state.branch_source = source;
        self.quote_state.branch_pending = pending;
        let rows = self.transcript_menu_rows();
        if let Some(popup) = &mut self.quote_state.popup {
            popup.reconcile(&rows);
        }
        cx.notify();
    }
}

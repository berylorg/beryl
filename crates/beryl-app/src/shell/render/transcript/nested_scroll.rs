use crate::shell::transcript_markdown::TranscriptCodePanelIdentity;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct TranscriptNestedScrollOwnership {
    selected_panel_identity: Option<TranscriptCodePanelIdentity>,
}

impl TranscriptNestedScrollOwnership {
    pub(super) fn selected_panel_id(&self) -> Option<&str> {
        self.selected_panel_identity
            .as_ref()
            .map(TranscriptCodePanelIdentity::as_str)
    }

    pub(super) fn selected_panel_identity(&self) -> Option<&TranscriptCodePanelIdentity> {
        self.selected_panel_identity.as_ref()
    }

    #[cfg(test)]
    pub(super) fn panel_owns_vertical_wheel(
        &self,
        panel_identity: &TranscriptCodePanelIdentity,
    ) -> bool {
        self.selected_panel_identity.as_ref() == Some(panel_identity)
    }

    pub(super) fn select_panel(&mut self, panel_identity: TranscriptCodePanelIdentity) -> bool {
        if self.selected_panel_identity.as_ref() == Some(&panel_identity) {
            return false;
        }

        self.selected_panel_identity = Some(panel_identity);
        true
    }

    pub(super) fn clear_to_transcript(&mut self) -> bool {
        self.selected_panel_identity.take().is_some()
    }

    pub(super) fn retain_visible_panel_identities<'a>(
        &mut self,
        visible_panel_identities: impl IntoIterator<Item = &'a TranscriptCodePanelIdentity>,
    ) -> bool {
        let Some(selected_panel_identity) = self.selected_panel_identity.as_ref() else {
            return false;
        };
        if visible_panel_identities
            .into_iter()
            .any(|panel_identity| panel_identity == selected_panel_identity)
        {
            return false;
        }

        self.selected_panel_identity = None;
        true
    }

    pub(super) fn record_scrollbar_activity(
        &mut self,
        _panel_identity: &TranscriptCodePanelIdentity,
    ) -> bool {
        false
    }

    #[cfg(test)]
    pub(super) fn handle_escape(&mut self) -> bool {
        false
    }
}

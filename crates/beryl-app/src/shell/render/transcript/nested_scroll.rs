#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct TranscriptNestedScrollOwnership {
    selected_panel_id: Option<String>,
}

impl TranscriptNestedScrollOwnership {
    pub(super) fn selected_panel_id(&self) -> Option<&str> {
        self.selected_panel_id.as_deref()
    }

    #[cfg(test)]
    pub(super) fn panel_owns_vertical_wheel(&self, panel_id: &str) -> bool {
        self.selected_panel_id.as_deref() == Some(panel_id)
    }

    pub(super) fn select_panel(&mut self, panel_id: impl Into<String>) -> bool {
        let panel_id = panel_id.into();
        if self.selected_panel_id.as_deref() == Some(panel_id.as_str()) {
            return false;
        }

        self.selected_panel_id = Some(panel_id);
        true
    }

    pub(super) fn clear_to_transcript(&mut self) -> bool {
        self.selected_panel_id.take().is_some()
    }

    pub(super) fn retain_visible_panel_ids<'a>(
        &mut self,
        visible_panel_ids: impl IntoIterator<Item = &'a str>,
    ) -> bool {
        let Some(selected_panel_id) = self.selected_panel_id.as_deref() else {
            return false;
        };
        if visible_panel_ids
            .into_iter()
            .any(|panel_id| panel_id == selected_panel_id)
        {
            return false;
        }

        self.selected_panel_id = None;
        true
    }

    pub(super) fn record_scrollbar_activity(&mut self, _panel_id: &str) -> bool {
        false
    }

    #[cfg(test)]
    pub(super) fn handle_escape(&mut self) -> bool {
        false
    }
}

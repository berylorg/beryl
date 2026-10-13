use super::*;

impl SyndicTranscriptPanel {
    pub(crate) fn transcript_menu_open_diagnostic_for_test(&self) -> Option<&str> {
        self.quote_state.last_open_diagnostic.as_deref()
    }
    pub(crate) fn transcript_menu_focused_id_for_test(&self) -> Option<&str> {
        self.quote_state.popup.as_ref()?.focused_id()
    }
    pub(crate) fn transcript_menu_row_position_for_test(
        &self,
        index: usize,
        window: &Window,
    ) -> Option<Point<Pixels>> {
        let rows = self.transcript_menu_rows();
        if index >= rows.len() {
            return None;
        }
        let bounds = self.quote_state.popup.as_ref()?.bounds(
            self.quote_state.anchor?,
            rows.len(),
            window.viewport_size(),
        );
        Some(
            bounds.origin
                + point(
                    bounds.size.width / 2.,
                    px(4. + widget::ROW_HEIGHT * (index as f32 + 0.5)),
                ),
        )
    }
    #[cfg(test)]
    pub(crate) fn quote_menu_open_for_test(&self) -> bool {
        self.quote_state.popup.is_some()
    }
    #[cfg(test)]
    pub(crate) fn transcript_menu_rows_for_test(&self) -> Vec<(String, bool, Option<String>)> {
        let rows = self.transcript_menu_rows();
        (0..rows.len())
            .map(|index| {
                let row = rows.row(index);
                (
                    row.label.to_string(),
                    row.enabled,
                    row.disabled_reason.map(|reason| reason.to_string()),
                )
            })
            .collect()
    }
    #[cfg(test)]
    pub(crate) fn refuse_edit_menu_pointer_for_test(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self
            .transcript_menu_rows()
            .index_of("transcript-edit-message")
        {
            quote_input(
                self,
                MenuEvent::Pointer {
                    invocation: self.quote_state.serial,
                    index,
                    id: Arc::from("transcript-edit-message"),
                },
                window,
                cx,
            );
        }
    }
    #[cfg(test)]
    pub(crate) fn quote_pointer_facts_for_test(
        &self,
        pointer: Point<Pixels>,
        window: &Window,
    ) -> (bool, Option<usize>, Vec<Range<usize>>, bool, u64) {
        let hit = self.narrative_hit(pointer);
        let ranges = self
            .narrative_selection
            .as_ref()
            .and_then(|selection| self.narrative_ranges(selection))
            .unwrap_or_default()
            .into_iter()
            .filter(|(id, _)| hit.as_ref().is_some_and(|hit| &hit.record == id))
            .map(|(_, range)| range)
            .collect();
        let eligible = self.quote_capture().is_some_and(|capture| {
            self.host
                .mapped_quote_markdown(
                    capture.selection.activation,
                    capture.selection.revision,
                    &capture.ranges,
                )
                .is_ok()
        });
        (
            self.character_geometry_current(window),
            hit.map(|hit| hit.display),
            ranges,
            eligible,
            self.quote_state.serial,
        )
    }
}

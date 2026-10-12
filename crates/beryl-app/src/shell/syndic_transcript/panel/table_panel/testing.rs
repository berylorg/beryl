use super::*;
impl SyndicTranscriptPanel {
    pub(crate) fn inline_table_source_for_test(&self) -> Option<&str> {
        Some(
            self.table_panels
                .first()?
                .viewport
                .owner
                .inline()?
                .prepared
                .source(),
        )
    }
    pub(crate) fn inline_table_viewport_for_test(
        &self,
    ) -> Option<(
        gpui::Bounds<gpui::Pixels>,
        gpui::Bounds<gpui::Pixels>,
        f32,
        f32,
    )> {
        let table = self.table_panels.first()?;
        table.viewport.owner.inline()?;
        let scroll = table.viewport.scroll.borrow();
        Some((
            scroll.bounds,
            scroll.header_bounds,
            scroll.row_height,
            scroll.column_width,
        ))
    }
    pub(crate) fn table_query_state_for_test(&self) -> String {
        let state = self
            .table_panels
            .first()
            .map(|table| {
                table.viewport.owner.indexed().map_or_else(
                    || "inline".to_owned(),
                    |owner| self.host.indexed_table_state_for_test(&owner.resource_id),
                )
            })
            .unwrap_or_else(|| "unmounted".to_owned());
        format!(
            "measured={:?}, published={}, candidate={}, {state}",
            self.last_frame_window
                .as_ref()
                .map(|frame| frame.presentation_revision),
            self.host.shared_snapshot().presentation_revision,
            self.staged.is_some()
        )
    }
    pub(crate) fn table_fallback_command_geometry_for_test(
        &self,
        row: u64,
    ) -> Option<(gpui::Bounds<gpui::Pixels>, gpui::Bounds<gpui::Pixels>)> {
        self.table_panels
            .first()?
            .fallback_bounds
            .borrow()
            .iter()
            .find(|(ordinal, _, _)| *ordinal == row)
            .map(|(_, copy, save)| (*copy, *save))
    }
    pub(crate) fn table_command_geometry_for_test(
        &self,
    ) -> Option<(gpui::Bounds<gpui::Pixels>, gpui::Bounds<gpui::Pixels>)> {
        self.table_panels.first()?.command_bounds.get()
    }
    pub(crate) fn table_commands_current_for_test(&self) -> bool {
        self.table_panels.first().is_some_and(|table| {
            table
                .viewport
                .owner
                .inline()
                .is_some_and(|owner| self.host.inline_table_current(owner))
                || !table.viewport.source_pages().is_empty()
                    && table.actions.binding == table.viewport.binding
                    && table.actions.root == table.viewport.root.as_ref().map(|root| root.digest)
        })
    }
    pub(crate) fn table_copy_pending_for_test(&self) -> bool {
        self.table_panels
            .first()
            .is_some_and(|table| table.actions.copy_pending())
    }
    pub(crate) fn table_save_pending_for_test(&self) -> bool {
        self.table_panels.first().is_some_and(|table| {
            table.actions.save.is_some() || table.actions.inline.save.is_some()
        })
    }
    pub(crate) fn table_viewport_for_test(
        &self,
    ) -> Option<(
        super::super::super::ResourceId,
        gpui::Bounds<gpui::Pixels>,
        gpui::Bounds<gpui::Pixels>,
        f32,
        f32,
    )> {
        let table = self.table_panels.first()?;
        let scroll = table.viewport.scroll.borrow();
        Some((
            table.viewport.owner.indexed()?.resource_id.clone(),
            scroll.bounds,
            scroll.header_bounds,
            scroll.row_height,
            scroll.column_width,
        ))
    }
    pub(crate) fn table_select_for_test(
        &mut self,
        row: u64,
        column: u64,
        extend: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(table) = self.table_panels.first_mut() else {
            return false;
        };
        let result = table.viewport.select(TableCellId::at(row, column), extend);
        self.keyboard_copy.gesture.reset();
        cx.notify();
        result
    }
    pub(crate) fn table_source_pages_for_test(
        &self,
    ) -> Vec<std::sync::Arc<super::super::super::IndexedResourceResponse>> {
        self.table_panels
            .first()
            .map_or_else(Vec::new, |table| table.viewport.source_pages().to_vec())
    }
    pub(crate) fn table_feedback_for_test(&self) -> &'static str {
        self.table_panels
            .first()
            .map_or("", |table| table.actions.feedback)
    }
    pub(crate) fn set_table_copy_writer_for_test(
        &mut self,
        writer: impl FnMut(
            &gpui::ClipboardItem,
            gpui::ClipboardLimits,
            &mut gpui::App,
        ) -> Result<(), gpui::ClipboardError>
        + 'static,
    ) {
        self.table_panels
            .first_mut()
            .expect("mountedTable")
            .actions
            .writer = Some(Box::new(writer));
    }
    pub(crate) fn table_save_choice_for_test(
        &mut self,
        receiver: futures_channel::oneshot::Receiver<Result<Option<gpui::SavePathChoice>, ()>>,
    ) {
        self.table_panels
            .first_mut()
            .expect("mountedTable")
            .actions
            .save_choice = Some(receiver);
    }
    pub(crate) fn begin_table_save_for_test(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(table) = self.table_panels.first() {
            self.begin_table_save(table.id, window, cx);
        }
    }
    pub(crate) fn table_copy_for_test(&mut self, stripped: bool, cx: &mut Context<Self>) {
        if let Some(table) = self.table_panels.first() {
            self.begin_table_copy(
                table.id,
                if stripped {
                    super::super::copy_keyboard::CopyMethod::Stripped
                } else {
                    super::super::copy_keyboard::CopyMethod::Markdown
                },
                cx,
            );
        }
    }
}

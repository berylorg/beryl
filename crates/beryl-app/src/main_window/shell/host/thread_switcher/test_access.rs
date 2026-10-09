use super::*;

impl MainWindowShellRoot {
    pub(crate) fn test_thread_switcher_toolbar_height(&self) -> (bool, f32) {
        (self.creation.is_none(), self.notice_chrome_height())
    }
    pub(crate) fn test_thread_switcher_visible_row_point(
        &self,
        thread: SyndicThreadId,
    ) -> Option<(PickerRowKey, gpui::Point<gpui::Pixels>, Option<String>)> {
        let position = self
            .thread_switcher
            .threads
            .iter()
            .position(|(_, id, _)| *id == thread)?;
        let (key, _, reason) = &self.thread_switcher.threads[position];
        let style = ThreadRootPickerStyle::default();
        let y = self.notice_chrome_height()
            + style.border_width
            + style.padding_y
            + style.header_height
            + style.search_height
            + style.heading_height
            + style.gap * 3.
            + position as f32 * style.row_stride()
            + style.row_height / 2.;
        Some((
            key.clone(),
            gpui::point(gpui::px(12. + style.width / 2.), gpui::px(y)),
            reason.clone(),
        ))
    }
    pub(crate) fn test_thread_switcher_thread_key(
        &self,
        thread: SyndicThreadId,
    ) -> Option<PickerRowKey> {
        self.thread_switcher
            .threads
            .iter()
            .find(|(_, candidate, _)| *candidate == thread)
            .map(|(key, _, _)| key.clone())
    }
    pub(crate) fn test_thread_switcher_focus(&self) -> gpui::FocusHandle {
        self.thread_switcher.focus.clone()
    }
    pub(crate) fn test_thread_switcher_root_keys(&self) -> Vec<PickerRowKey> {
        self.thread_switcher
            .roots
            .iter()
            .map(|(key, _)| key.clone())
            .collect()
    }
    pub(crate) fn test_thread_switcher_reader(&mut self, reader: PublishedCatalogQueryReader) {
        self.thread_switcher.fixture_reader = Some(reader.clone());
        self.thread_switcher.reader = Some(reader);
    }
    pub(crate) fn test_thread_switcher_picker(&self) -> Option<Entity<ThreadRootPicker>> {
        self.thread_switcher.picker.clone()
    }
    pub(crate) fn test_thread_switcher_revision(&self) -> Option<HomeRevision> {
        self.thread_switcher.home_revision
    }
    pub(crate) fn test_thread_switcher_retained(&self) -> (bool, bool, usize) {
        (
            self.thread_switcher.anchor.is_some(),
            self.thread_switcher.refined.is_some(),
            self.thread_switcher.jobs.len(),
        )
    }
    pub(crate) fn test_thread_switcher_failure(&self) -> Option<&str> {
        self.thread_switcher.failure.as_deref()
    }
    pub(crate) fn test_thread_switcher_dismiss(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_thread_switcher(window, cx);
    }
    pub(crate) fn test_thread_switcher_keys(&self) -> Vec<PickerRowKey> {
        self.thread_switcher
            .threads
            .iter()
            .map(|(key, _, _)| key.clone())
            .collect()
    }
    pub(crate) fn test_thread_switcher_runtime_keys(&self) -> Vec<PickerRowKey> {
        self.thread_switcher
            .runtimes
            .iter()
            .map(|(key, _)| key.clone())
            .collect()
    }
}

use super::*;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn publish_selected_lineage(
        &mut self,
        selected: crate::main_window::MainWindowComposerSelectionIdentity,
        head: syndic_storage::ThreadLineageHead,
        title: beryl_state::CatalogResolvedTitle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if head.leaf_thread_id() != selected.claim().thread_id() {
            return;
        }
        let changed = self.running_threads.lineage.selected != Some(selected)
            || self.running_threads.lineage.head.as_ref() != Some(&head)
            || self.running_threads.lineage.title.as_ref() != Some(&title);
        if changed {
            let selected_changed = self
                .running_threads
                .lineage
                .selected
                .map(|selected| selected.claim().thread_id())
                != Some(selected.claim().thread_id());
            if (selected_changed || head.total_parent_count() == 0)
                && self
                    .running_threads
                    .lineage
                    .widget
                    .as_ref()
                    .is_some_and(|widget| widget.read(cx).focused_owner(window).is_some())
            {
                self.notice_safe_focus(cx).focus(window);
            }
            self.cancel_lineage_reads(cx);
            let lineage = &mut self.running_threads.lineage;
            if lineage
                .selected
                .map(|selected| selected.claim().thread_id())
                != Some(selected.claim().thread_id())
            {
                lineage.widget = None;
                lineage.subscription = None;
            }
            lineage.selected = Some(selected);
            lineage.head = Some(head);
            lineage.title = Some(title);
            lineage.observation = None;
            if lineage
                .head
                .as_ref()
                .is_some_and(|head| head.total_parent_count() == 0)
            {
                lineage.widget = None;
                lineage.subscription = None;
            } else if let Some(revision) = lineage.revision.checked_add(1) {
                lineage.revision = revision;
                let query = LineageQuery {
                    revision,
                    selected: selected.claim().thread_id(),
                    parent_count: lineage.head.as_ref().unwrap().total_parent_count(),
                };
                let title = lineage
                    .title
                    .as_ref()
                    .and_then(|title| title.text())
                    .unwrap_or("Untitled")
                    .to_owned();
                if let Some(widget) = lineage.widget.clone() {
                    widget.update(cx, |widget, cx| {
                        widget.set_inert(true, cx);
                        widget.replace(query, title, window, cx);
                    });
                } else {
                    let widget = cx.new(|cx| {
                        let mut widget = ThreadLineage::new(query, title, window, cx);
                        widget.set_inert(true, cx);
                        widget
                    });
                    lineage.subscription = Some(cx.subscribe_in(
                        &widget,
                        window,
                        |root, widget, event, window, cx| {
                            root.lineage_event(&widget, event.clone(), window, cx);
                        },
                    ));
                    lineage.widget = Some(widget);
                }
            }
        }
        self.sync_selected_lineage(window, cx);
    }

    pub(in crate::main_window::shell) fn running_recovery_focus_current(
        &self,
        focus: &gpui::FocusHandle,
        app: &gpui::App,
    ) -> bool {
        if self.controller.is_none() {
            return false;
        }
        if &self.shell_focus == focus
            || self.thread_navigation.focus.contains(focus)
            || self.thread_switcher_focus_current(focus)
        {
            return true;
        }
        let lineage = &self.running_threads.lineage;
        lineage.selected.is_some_and(|selected| {
            self.running_threads.transcript_claim == Some(selected.claim())
                && self
                    .running_threads
                    .selected_title
                    .as_ref()
                    .is_some_and(|(identity, _)| *identity == selected)
                && lineage.widget.as_ref().is_some_and(|widget| {
                    widget.read(app).query().selected == selected.claim().thread_id()
                        && &widget.read(app).focus_proxy() == focus
                })
        })
    }
}

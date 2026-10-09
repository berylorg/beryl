use super::*;

impl MainWindowShellRoot {
    pub(super) fn switcher_event(
        &mut self,
        event: PickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::RequestPage(request) => {
                if self.thread_switcher.command.is_some()
                    && !self.thread_switcher.refresh_proven
                    && !self.thread_switcher.opening_pending
                {
                    self.finish_switcher_configuration(true, window, cx);
                } else if self.thread_switcher.anchor.is_none()
                    && !self.thread_switcher.opening_pending
                {
                    self.switcher_open_anchor(window, cx);
                } else {
                    self.switcher_request_page(request, false, window, cx);
                }
            }
            PickerEvent::RequestRuntimePage(request) => {
                if request.query_revision != self.thread_switcher.revision {
                    return;
                }
                if !self.thread_switcher.runtime_requests.contains(&request) {
                    if self.thread_switcher.runtime_requests.len() == 2 {
                        self.thread_switcher.runtime_requests.remove(0);
                    }
                    self.thread_switcher.runtime_requests.push(request.clone());
                }
                if self.thread_switcher.anchor.is_none() && !self.thread_switcher.opening_pending {
                    self.switcher_open_anchor(window, cx);
                    return;
                }
                self.switcher_request_page(request, true, window, cx);
            }
            PickerEvent::QueryChanged {
                collection_key,
                query_revision,
                query,
            } => {
                if collection_key != self.thread_switcher.mode.key()
                    || self.thread_switcher.command.is_some()
                    || self.thread_switcher.activation.is_some()
                {
                    return;
                }
                if query_revision <= self.thread_switcher.revision {
                    return;
                }
                let Ok(query) = CatalogNormalizedQuery::new(&query) else {
                    return;
                };
                self.thread_switcher.cancel_jobs();
                self.thread_switcher.query = query;
                self.thread_switcher.revision = query_revision;
                self.thread_switcher.primary_ready = false;
                self.thread_switcher.runtime_ready = false;
                self.thread_switcher.runtime_requests.clear();
                if let Some(picker) = self.thread_switcher.picker.clone() {
                    picker.update(cx, |picker, pcx| {
                        picker.replace_runtime_collection(
                            PickerCollectionKey("switcher-runtimes".into()),
                            query_revision,
                            0,
                            pcx,
                        )
                    });
                }
                self.switcher_refine(window, cx);
            }
            PickerEvent::Activate(key) => self.switcher_activate(key, window, cx),
            PickerEvent::Command(command) => self.switcher_command(command, window, cx),
            PickerEvent::Dismiss => {
                if self.thread_switcher.command.is_none() {
                    self.thread_switcher.discard();
                }
                cx.notify();
            }
            PickerEvent::SelectionChanged(_) => {}
        }
    }

    fn switcher_change_mode(
        &mut self,
        mode: SwitcherMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.thread_switcher.picker.clone() else {
            return;
        };
        self.thread_switcher.cancel_jobs();
        self.thread_switcher.mode = mode;
        self.thread_switcher.query = CatalogNormalizedQuery::new("").expect("empty query");
        self.thread_switcher.revision = picker
            .read(cx)
            .diagnostics()
            .query_revision
            .checked_add(1)
            .expect("switcher revision");
        self.thread_switcher.primary_ready = false;
        self.thread_switcher.runtime_ready = false;
        self.thread_switcher.runtime_requests.clear();
        let revision = self.thread_switcher.revision;
        let key = self.thread_switcher.mode.key();
        let (heading, helper, presentation, back) = match &self.thread_switcher.mode {
            SwitcherMode::All => (
                "THREADS FOR ALL ROOTS".into(),
                "Choose a thread from any configured root.".into(),
                PickerRowPresentation::Thread,
                None,
            ),
            SwitcherMode::Root { path, .. } => (
                format!("THREADS FOR {path}"),
                "Choose a thread from this root.".into(),
                PickerRowPresentation::Thread,
                Some(PickerCommandState::enabled("Clear root scope")),
            ),
            SwitcherMode::Roots(runtime) => (
                format!("ROOTS FOR {}", runtime.environment_label()),
                "Choose a root to show its threads.".into(),
                PickerRowPresentation::Root,
                Some(PickerCommandState::enabled("Back to threads")),
            ),
        };
        picker.update(cx, |picker, pcx| {
            picker.replace_collection(key, revision, 0, None, window, pcx);
            picker.clear_search(pcx);
            picker.set_row_presentation(presentation, pcx);
            picker.set_collection_labels(helper, heading, "No matching rows.".into(), pcx);
            picker.set_return_command(back, pcx);
            picker.replace_runtime_collection(
                PickerCollectionKey("switcher-runtimes".into()),
                revision,
                0,
                pcx,
            );
            picker.request_initial_page(pcx);
            picker.request_runtime_initial_page(pcx);
        });
        // Collection restoration may clear a retained search with another widget revision.
        self.thread_switcher.revision = picker.read(cx).diagnostics().query_revision;
        let revision = self.thread_switcher.revision;
        picker.update(cx, |picker, pcx| {
            picker.replace_runtime_collection(
                PickerCollectionKey("switcher-runtimes".into()),
                revision,
                0,
                pcx,
            )
        });
        self.switcher_refine(window, cx);
    }

    fn switcher_command(
        &mut self,
        command: PickerCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.thread_switcher.command.is_some() || self.thread_switcher.activation.is_some() {
            return;
        }
        match command {
            PickerCommand::Return => self.switcher_change_mode(SwitcherMode::All, window, cx),
            PickerCommand::BrowseRoots(key) => {
                if let Some(runtime) = self
                    .thread_switcher
                    .runtimes
                    .iter()
                    .find(|(resident, _)| resident == &key)
                    .map(|(_, row)| row.runtime().clone())
                {
                    self.switcher_change_mode(SwitcherMode::Roots(runtime), window, cx);
                }
            }
            command @ (PickerCommand::AddRuntime | PickerCommand::AddRoot(_)) => {
                let runtime = match &command {
                    PickerCommand::AddRoot(key) => self
                        .thread_switcher
                        .runtimes
                        .iter()
                        .find(|(resident, _)| resident == key)
                        .map(|(_, row)| row.runtime().clone()),
                    _ => None,
                };
                self.thread_switcher.command = Some(command.clone());
                if let Err(error) =
                    self.begin_switcher_setup_command(command.clone(), runtime, window, cx)
                {
                    self.thread_switcher.command = None;
                    self.thread_switcher.failure = Some(error);
                    if let Some(picker) = self.thread_switcher.picker.clone() {
                        picker.update(cx, |picker, pcx| picker.finish_command(&command, pcx));
                    }
                }
            }
            _ => {}
        }
    }

    fn switcher_activate(
        &mut self,
        key: PickerRowKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.thread_switcher.command.is_some()
            || self.thread_switcher.activation.is_some()
            || self.switcher_disabled_reason(cx).is_some()
        {
            return;
        }
        if matches!(self.thread_switcher.mode, SwitcherMode::Roots(_)) {
            if let Some(row) = self
                .thread_switcher
                .roots
                .iter()
                .find(|(resident, _)| resident == &key)
                .map(|(_, row)| row.clone())
            {
                if let Some(picker) = self.thread_switcher.picker.clone() {
                    picker.update(cx, |picker, pcx| picker.finish_activation(pcx));
                }
                self.switcher_change_mode(
                    SwitcherMode::Root {
                        runtime: row.runtime().runtime_id(),
                        root: row.root().root_id(),
                        path: row.root().display_path().as_str().into(),
                    },
                    window,
                    cx,
                );
            }
            return;
        }
        let Some((_, thread, reason)) = self
            .thread_switcher
            .threads
            .iter()
            .find(|(resident, _, _)| resident == &key)
        else {
            return;
        };
        if reason.is_some() {
            return;
        }
        let thread = *thread;
        let Some(fence) = self.thread_switcher.fence() else {
            return;
        };
        self.thread_switcher.activation = Some(key);
        if let Some(picker) = self.thread_switcher.picker.clone() {
            picker.update(cx, |picker, pcx| {
                picker.finish_activation(pcx);
                picker.set_external_command_reason(
                    Some("This thread activation is waiting for its exact admission.".into()),
                    pcx,
                );
            });
        }
        let result = self.request_ordinary_thread_activation(
            thread,
            window,
            cx,
            move |result, window, cx| {
                cx.defer_in(window, move |root, window, cx| {
                    if !root.thread_switcher.matches(&fence) {
                        return;
                    }
                    root.thread_switcher.activation = None;
                    match result {
                        Ok(_) => root.close_thread_switcher(window, cx),
                        Err(error) => {
                            root.thread_switcher.failure = Some(error);
                            if let Some(picker) = root.thread_switcher.picker.clone() {
                                picker.update(cx, |picker, pcx| {
                                    picker.set_external_command_reason(None, pcx);
                                    picker.finish_activation(pcx);
                                });
                            }
                            cx.notify();
                        }
                    }
                });
            },
        );
        if let Err(error) = result {
            self.thread_switcher.activation = None;
            self.thread_switcher.failure = Some(error);
            if let Some(picker) = self.thread_switcher.picker.clone() {
                picker.update(cx, |picker, pcx| {
                    picker.set_external_command_reason(None, pcx);
                    picker.finish_activation(pcx);
                });
            }
        }
    }
}

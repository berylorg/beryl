use super::*;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn finish_switcher_configuration(
        &mut self,
        changed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.thread_switcher.picker.clone() else {
            return;
        };
        if !changed {
            if let Some(command) = self.thread_switcher.command.take() {
                picker.update(cx, |picker, pcx| picker.finish_command(&command, pcx));
            }
            self.thread_switcher.refresh_proven = false;
            cx.notify();
            return;
        }
        self.thread_switcher.cancel_jobs();
        self.thread_switcher.refined = None;
        self.thread_switcher.anchor = None;
        self.thread_switcher.home_revision = None;
        self.thread_switcher.opening = Arc::new(());
        self.thread_switcher.primary_ready = false;
        self.thread_switcher.runtime_ready = false;
        self.thread_switcher.refresh_proven = false;
        self.thread_switcher.runtime_requests.clear();
        let revision = picker
            .read(cx)
            .diagnostics()
            .query_revision
            .checked_add(1)
            .expect("switcher revision");
        self.thread_switcher.revision = revision;
        let key = self.thread_switcher.mode.key();
        picker.update(cx, |picker, pcx| {
            picker.set_pending_page_retry_allowed(true, pcx);
            picker.replace_collection(key, revision, 0, None, window, pcx);
            picker.replace_runtime_collection(
                PickerCollectionKey("switcher-runtimes".into()),
                revision,
                0,
                pcx,
            );
            picker.request_initial_page(pcx);
            picker.request_runtime_initial_page(pcx);
        });
        self.switcher_open_anchor(window, cx);
    }

    pub(super) fn switcher_prove_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((runtime, root, minimum)) = self.switcher_setup_refresh_target() else {
            self.switcher_fail_pending(
                "The original configuration refresh target is unavailable.".into(),
                window,
                cx,
            );
            return;
        };
        let Some(revision) = self.thread_switcher.home_revision else {
            return;
        };
        if minimum.is_some_and(|minimum| revision < minimum) {
            self.switcher_fail_pending(
                "The catalog is waiting for the original configuration receipt.".into(),
                window,
                cx,
            );
            return;
        }
        let cancellation = CommandCancellation::new();
        let request = self
            .thread_switcher
            .anchor
            .as_ref()
            .expect("refresh anchor")
            .runtime_position(
                CatalogNormalizedQuery::new("").expect("empty query"),
                runtime,
                cancellation.clone(),
            )
            .map_err(|error| error.to_string());
        let result = request.and_then(|request| {
            self.switcher_spawn(
                request,
                cancellation,
                window,
                cx,
                move |this, result, window, cx| match result {
                    Ok(PublishedCatalogQueryResult::OptionPosition(Some(_))) => {
                        if let Some(root) = root {
                            this.switcher_prove_root(runtime, root, window, cx);
                        } else {
                            this.thread_switcher.refresh_proven = true;
                            this.switcher_refine(window, cx);
                        }
                    }
                    Ok(_) => this.switcher_fail_pending(
                        "The new frozen catalog does not contain the acknowledged runtime.".into(),
                        window,
                        cx,
                    ),
                    Err(error) => this.switcher_fail_pending(error, window, cx),
                },
            )
        });
        if let Err(error) = result {
            self.switcher_fail_pending(error, window, cx);
        }
    }
    fn switcher_prove_root(
        &mut self,
        runtime: RuntimeId,
        root: RootId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cancellation = CommandCancellation::new();
        let request = self
            .thread_switcher
            .anchor
            .as_ref()
            .expect("refresh anchor")
            .root_position(
                runtime,
                CatalogNormalizedQuery::new("").expect("empty query"),
                root,
                cancellation.clone(),
            )
            .map_err(|error| error.to_string());
        let result = request.and_then(|request| {
            self.switcher_spawn(
                request,
                cancellation,
                window,
                cx,
                |this, result, window, cx| match result {
                    Ok(PublishedCatalogQueryResult::OptionPosition(Some(_))) => {
                        this.thread_switcher.refresh_proven = true;
                        this.switcher_refine(window, cx);
                    }
                    Ok(_) => this.switcher_fail_pending(
                        "The new frozen catalog does not contain the acknowledged root.".into(),
                        window,
                        cx,
                    ),
                    Err(error) => this.switcher_fail_pending(error, window, cx),
                },
            )
        });
        if let Err(error) = result {
            self.switcher_fail_pending(error, window, cx);
        }
    }

    pub(super) fn switcher_complete_refresh(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.thread_switcher.command.is_none()
            || !self.thread_switcher.refresh_proven
            || !self.thread_switcher.primary_ready
            || !self.thread_switcher.runtime_ready
        {
            return;
        }
        let Some(revision) = self.thread_switcher.home_revision else {
            return;
        };
        if let Err(error) = self.complete_switcher_setup_refresh(revision, window, cx) {
            self.thread_switcher.failure = Some(error);
        }
    }
}

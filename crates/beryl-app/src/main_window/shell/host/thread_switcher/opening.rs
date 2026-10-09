use super::*;

impl MainWindowShellRoot {
    pub(crate) fn open_thread_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.switcher_disabled_reason(cx).is_some() || self.thread_switcher.picker.is_some() {
            return;
        }
        let Some(reader) = self.switcher_reader(cx).filter(|reader| reader.is_ready()) else {
            return;
        };
        let Some(controller) = self.controller() else {
            return;
        };
        let appearance = controller.appearance().clone();
        let scrollbar_style = controller.appearance.scrollbar.clone();
        self.thread_switcher.discard();
        self.thread_switcher.reader = Some(reader);
        self.thread_switcher.mode = SwitcherMode::All;
        self.thread_switcher.query = CatalogNormalizedQuery::new("").expect("empty query");
        self.thread_switcher.revision = self
            .thread_switcher
            .revision
            .checked_add(1)
            .expect("switcher revision");
        self.thread_switcher.home_revision = None;
        self.thread_switcher.failure = None;
        self.thread_switcher.focus_pending = true;
        let config = ThreadRootPickerConfig {
            title: "Switch thread".into(),
            helper: "Choose a thread from any configured root.".into(),
            heading: "THREADS FOR ALL ROOTS".into(),
            empty_text: "No matching threads.".into(),
            search_placeholder: "Search threads, roots and runtimes".into(),
            owner_focus: self.thread_switcher.focus.clone(),
            appearance: Some(appearance.clone()),
            style: ThreadRootPickerStyle::default(),
            scrollbar_style,
        };
        let key = self.thread_switcher.mode.key();
        let revision = self.thread_switcher.revision;
        let picker = cx.new(|pcx| ThreadRootPicker::new(config, key, revision, 0, window, pcx));
        self.thread_switcher.subscription = Some(cx.subscribe_in(
            &picker,
            window,
            |root, picker, event: &PickerEvent, window, cx| {
                if root
                    .thread_switcher
                    .picker
                    .as_ref()
                    .is_some_and(|current| current == picker)
                {
                    root.switcher_event(event.clone(), window, cx);
                }
            },
        ));
        self.thread_switcher.picker = Some(picker.clone());
        picker.update(cx, |picker, pcx| {
            picker.configure_selection(PickerSelectionMode::Immediate, pcx);
            picker.configure_runtime_section(
                PickerRuntimeSectionConfig {
                    heading: "RUNTIMES & ROOTS".into(),
                    empty_text: "No matching runtimes.".into(),
                    add_runtime: PickerCommandState::enabled("Add runtime"),
                },
                PickerCollectionKey("switcher-runtimes".into()),
                revision,
                0,
                window,
                pcx,
            );
            picker.set_retry_commands(
                Some(PickerCommandState::enabled("Retry")),
                Some(PickerCommandState::enabled("Retry")),
                pcx,
            );
            picker.request_initial_page(pcx);
            picker.request_runtime_initial_page(pcx);
            picker.set_external_command_reason(
                Some("The catalog is preparing its first coherent rows.".into()),
                pcx,
            );
        });
        self.switcher_open_anchor(window, cx);
        cx.notify();
    }

    pub(super) fn switcher_open_anchor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cancellation = CommandCancellation::new();
        let request = self
            .thread_switcher
            .reader
            .as_ref()
            .ok_or_else(|| "The catalog is unavailable.".to_owned())
            .and_then(|reader| {
                reader
                    .open(
                        CatalogQueryCriteria::new(
                            CatalogQueryScope::All,
                            CatalogNormalizedQuery::new("").expect("empty query"),
                        ),
                        CatalogQueryPageLimit::maximum(),
                        cancellation.clone(),
                    )
                    .map_err(|error| error.to_string())
            });
        self.thread_switcher.opening_pending = true;
        let result = request.and_then(|request| {
            self.switcher_spawn(
                request,
                cancellation,
                window,
                cx,
                |root, result, window, cx| {
                    root.thread_switcher.opening_pending = false;
                    match result {
                        Ok(PublishedCatalogQueryResult::Opened(opened)) => {
                            let (collection, metadata) = opened.into_parts();
                            if metadata.criteria().scope() != CatalogQueryScope::All
                                || !metadata.criteria().search().as_str().is_empty()
                            {
                                root.switcher_fail_pending(
                                    "The opening catalog did not match its request.".into(),
                                    window,
                                    cx,
                                );
                                return;
                            }
                            root.thread_switcher.home_revision = Some(metadata.home_revision());
                            root.thread_switcher.anchor = Some(collection);
                            root.thread_switcher.refresh_proven =
                                root.thread_switcher.command.is_none();
                            if root.thread_switcher.command.is_some() {
                                root.switcher_prove_refresh(window, cx);
                            } else {
                                root.switcher_refine(window, cx);
                            }
                        }
                        Ok(_) => root.switcher_fail_pending(
                            "The opening catalog response was invalid.".into(),
                            window,
                            cx,
                        ),
                        Err(error) => root.switcher_fail_pending(error, window, cx),
                    }
                },
            )
        });
        if let Err(error) = result {
            self.thread_switcher.opening_pending = false;
            self.switcher_fail_pending(error, window, cx);
        }
    }

    pub(super) fn switcher_refine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.thread_switcher.anchor.is_none() {
            return;
        }
        self.thread_switcher.refined = None;
        self.thread_switcher.threads.clear();
        self.thread_switcher.roots.clear();
        if matches!(self.thread_switcher.mode, SwitcherMode::Roots(_)) {
            self.thread_switcher.refinement_pending = false;
            self.switcher_resume_pages(window, cx);
            return;
        }
        let cancellation = CommandCancellation::new();
        let criteria = CatalogQueryCriteria::new(
            self.thread_switcher.mode.scope(),
            self.thread_switcher.query.clone(),
        );
        let request = self
            .thread_switcher
            .anchor
            .as_ref()
            .expect("anchor")
            .refine(
                criteria.clone(),
                CatalogQueryPageLimit::maximum(),
                cancellation.clone(),
            )
            .map_err(|error| error.to_string());
        self.thread_switcher.refinement_pending = true;
        let result = request.and_then(|request| {
            self.switcher_spawn(
                request,
                cancellation,
                window,
                cx,
                move |root, result, window, cx| {
                    root.thread_switcher.refinement_pending = false;
                    match result {
                        Ok(PublishedCatalogQueryResult::Opened(opened)) => {
                            let (collection, metadata) = opened.into_parts();
                            if metadata.criteria() != &criteria
                                || Some(metadata.home_revision())
                                    != root.thread_switcher.home_revision
                            {
                                root.switcher_fail_pending(
                                    "The refined catalog belongs to another snapshot.".into(),
                                    window,
                                    cx,
                                );
                                return;
                            }
                            let Ok(count) = usize::try_from(metadata.count()) else {
                                root.switcher_fail_pending(
                                    "The catalog count cannot be represented.".into(),
                                    window,
                                    cx,
                                );
                                return;
                            };
                            root.thread_switcher.count = count;
                            root.thread_switcher.refined = Some(collection);
                            root.switcher_resume_pages(window, cx);
                        }
                        Ok(_) => root.switcher_fail_pending(
                            "The refined catalog response was invalid.".into(),
                            window,
                            cx,
                        ),
                        Err(error) => root.switcher_fail_pending(error, window, cx),
                    }
                },
            )
        });
        if let Err(error) = result {
            self.thread_switcher.refinement_pending = false;
            self.switcher_fail_pending(error, window, cx);
        }
    }

    fn switcher_resume_pages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.thread_switcher.picker.clone() else {
            return;
        };
        let requests = picker.read(cx).pending_requests().to_vec();
        for request in requests {
            self.switcher_request_page(request, false, window, cx);
        }
        let runtime_requests = self.thread_switcher.runtime_requests.clone();
        for request in runtime_requests {
            self.switcher_request_page(request, true, window, cx);
        }
    }

    pub(super) fn switcher_fail_pending(
        &mut self,
        error: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.thread_switcher.failure = Some(error.clone());
        if let Some(picker) = self.thread_switcher.picker.clone() {
            let pending = picker.read(cx).pending_requests().to_vec();
            let runtime = std::mem::take(&mut self.thread_switcher.runtime_requests);
            picker.update(cx, |picker, pcx| {
                if self.thread_switcher.command.is_none() {
                    picker.set_external_command_reason(None, pcx);
                }
                for request in pending {
                    picker.settle_page(
                        PickerPageOutcome::Failed {
                            request,
                            message: error.clone(),
                        },
                        window,
                        pcx,
                    );
                }
                for request in runtime {
                    picker.settle_runtime_page(
                        PickerRuntimePageOutcome::Failed {
                            request,
                            message: error.clone(),
                        },
                        window,
                        pcx,
                    );
                }
            });
        }
        cx.notify();
    }
}

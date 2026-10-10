use super::*;
mod publication;
use crate::thread_lineage::{LineageEvent, LineagePageRequest, LineageQuery, ThreadLineage};

#[derive(Default)]
pub(super) struct SelectedLineage {
    selected: Option<crate::main_window::MainWindowComposerSelectionIdentity>,
    head: Option<syndic_storage::ThreadLineageHead>,
    title: Option<beryl_state::CatalogResolvedTitle>,
    revision: u64,
    pub(super) observation: Option<crate::app_services::RunningThreadsObservation>,
    pub(super) widget: Option<Entity<ThreadLineage>>,
    pub(super) subscription: Option<gpui::Subscription>,
    head_job: Option<(ProjectionCancellationToken, gpui::Task<()>)>,
    page_jobs: Vec<(
        LineagePageRequest,
        ProjectionCancellationToken,
        gpui::Task<()>,
    )>,
    #[cfg(all(test, feature = "test-faults"))]
    head_gate: Option<Arc<test_access::HeadGate>>,
}

impl SelectedLineage {
    pub(super) fn matches_selection(
        &self,
        selected: crate::main_window::MainWindowComposerSelectionIdentity,
    ) -> bool {
        self.selected == Some(selected)
    }

    pub(super) fn cancel(&mut self) {
        if let Some((cancel, _)) = &self.head_job {
            cancel.cancel();
        }
        for (_, cancel, _) in &self.page_jobs {
            cancel.cancel();
        }
    }
}

impl MainWindowShellRoot {
    pub(super) fn cancel_lineage_reads(&mut self, cx: &mut Context<Self>) {
        let lineage = &mut self.running_threads.lineage;
        lineage.cancel();
        lineage.head_job = None;
        lineage.page_jobs.clear();
        lineage.observation = None;
        if let Some(widget) = lineage.widget.clone() {
            widget.update(cx, |widget, cx| {
                widget.set_inert(true, cx);
                widget.cancel_requests(cx);
            });
        }
    }

    pub(in crate::main_window::shell::host) fn sync_selected_lineage(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.running_threads_enabled()
            || !self.running_read_capacity_available()
            || self.running_threads.has_activation_custody()
        {
            return;
        }
        let lineage = &self.running_threads.lineage;
        let Some(selected) = lineage.selected else {
            return;
        };
        if self
            .cached_running_selection(cx)
            .map(|(selection, _)| selection)
            != Some(selected)
            || self.running_threads.transcript_claim != Some(selected.claim())
        {
            return;
        }
        let Some(head) = lineage
            .head
            .clone()
            .filter(|head| head.total_parent_count() != 0)
        else {
            return;
        };
        let Some(reader) = self.running_threads.reader.clone() else {
            return;
        };
        if lineage.head_job.is_some() {
            return;
        }
        if lineage
            .observation
            .as_ref()
            .is_some_and(|observation| reader.elect(observation, || ()).is_ok())
        {
            if let Some(widget) = lineage.widget.clone() {
                widget.update(cx, |widget, cx| widget.resume(cx));
            }
            return;
        }
        self.cancel_lineage_reads(cx);
        let generation = self.running_threads.generation.load(Ordering::Acquire);
        let native = window.window_handle();
        let cancel = ProjectionCancellationToken::new();
        let cancelled = cancel.clone();
        let source = reader.clone();
        let expected_head = head.clone();
        #[cfg(all(test, feature = "test-faults"))]
        let head_gate = self.running_threads.lineage.head_gate.clone();
        let release = self
            .running_threads
            .workers
            .track(release_running_read_output as fn());
        let work = self.running_threads.workers.track(move || {
            #[cfg(all(test, feature = "test-faults"))]
            if let Some(gate) = head_gate {
                gate.wait(&cancelled);
            }
            RunningReadOutput {
                result: source.lineage_head_current(selected, &expected_head, &cancelled),
                _release: release,
            }
        });
        let job = cx.background_executor().spawn(async move { work.run() });
        let task_cancel = cancel.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let output = job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                let RunningReadOutput { result, _release } = output;
                if task_cancel.is_cancelled()
                    || root.running_threads.generation.load(Ordering::Acquire) != generation
                    || window.window_handle() != native
                    || !root.running_threads_enabled()
                    || root.running_threads.lineage.selected != Some(selected)
                    || root.running_threads.lineage.head.as_ref() != Some(&head)
                    || root
                        .cached_running_selection(cx)
                        .map(|(selection, _)| selection)
                        != Some(selected)
                    || !root
                        .running_threads
                        .reader
                        .as_ref()
                        .is_some_and(|current| current.same_publication(&reader))
                {
                    return;
                }
                root.running_threads.lineage.head_job = None;
                let Ok(observed) = result else {
                    return;
                };
                if reader
                    .elect(&observed, || {
                        let lineage = &mut root.running_threads.lineage;
                        let Some(revision) = lineage.revision.checked_add(1) else {
                            return;
                        };
                        lineage.revision = revision;
                        let query = LineageQuery {
                            revision,
                            selected: selected.claim().thread_id(),
                            parent_count: head.total_parent_count(),
                        };
                        let title = lineage
                            .title
                            .as_ref()
                            .and_then(|title| title.text())
                            .unwrap_or("Untitled")
                            .to_owned();
                        if let Some(widget) = lineage.widget.clone() {
                            widget.update(cx, |widget, cx| {
                                widget.set_inert(false, cx);
                                widget.replace(query, title, window, cx);
                                widget.resume(cx);
                            });
                        } else {
                            let widget = cx.new(|cx| ThreadLineage::new(query, title, window, cx));
                            lineage.subscription = Some(cx.subscribe_in(
                                &widget,
                                window,
                                |root, widget, event, window, cx| {
                                    root.lineage_event(&widget, event.clone(), window, cx);
                                },
                            ));
                            lineage.widget = Some(widget.clone());
                            widget.update(cx, |widget, cx| widget.resume(cx));
                        }
                        cx.notify();
                    })
                    .is_ok()
                {
                    root.running_threads.lineage.observation = Some(observed);
                }
            });
        });
        self.running_threads.lineage.head_job = Some((cancel, task));
    }

    fn lineage_event(
        &mut self,
        widget: &Entity<ThreadLineage>,
        event: LineageEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.running_threads.lineage.widget.as_ref() != Some(widget) {
            return;
        }
        match event {
            LineageEvent::RequestPage(request) => self.request_lineage_page(request, window, cx),
            LineageEvent::Activate { query, thread } => {
                if !self.lineage_current(query, cx)
                    || self.running_threads.has_activation_custody()
                    || self.switcher_disabled_reason(cx).is_some()
                {
                    return;
                }
                let _ = self
                    .request_ordinary_thread_activation(thread, window, cx, |_, _, cx| cx.notify());
            }
        }
    }

    fn lineage_current(&self, query: LineageQuery, cx: &gpui::App) -> bool {
        let lineage = &self.running_threads.lineage;
        self.running_threads_enabled()
            && lineage.revision == query.revision
            && lineage.selected.is_some_and(|selected| {
                selected.claim().thread_id() == query.selected
                    && self
                        .cached_running_selection(cx)
                        .map(|(current, _)| current)
                        == Some(selected)
                    && self.running_threads.transcript_claim == Some(selected.claim())
            })
            && lineage
                .head
                .as_ref()
                .is_some_and(|head| head.total_parent_count() == query.parent_count)
            && lineage.observation.as_ref().is_some_and(|observed| {
                self.running_threads
                    .reader
                    .as_ref()
                    .is_some_and(|reader| reader.elect(observed, || ()).is_ok())
            })
    }

    fn request_lineage_page(
        &mut self,
        request: LineagePageRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let reject = !self.lineage_current(request.query, cx)
            || !self.running_read_capacity_available()
            || self.running_threads.has_activation_custody()
            || self.running_threads.lineage.page_jobs.len() >= 2;
        if reject {
            if let Some(widget) = self.running_threads.lineage.widget.clone() {
                widget.update(cx, |widget, cx| {
                    widget.settle_page(request, None, false, window, cx)
                });
            }
            return;
        }
        if self
            .running_threads
            .lineage
            .page_jobs
            .iter()
            .any(|(pending, _, _)| *pending == request)
        {
            return;
        }
        let selected = self.running_threads.lineage.selected.unwrap();
        let head = self.running_threads.lineage.head.clone().unwrap();
        let reader = self.running_threads.reader.clone().unwrap();
        let source = reader.clone();
        let generation = self.running_threads.generation.load(Ordering::Acquire);
        let native = window.window_handle();
        let cancel = ProjectionCancellationToken::new();
        let cancelled = cancel.clone();
        let release = self
            .running_threads
            .workers
            .track(release_running_read_output as fn());
        let work = self
            .running_threads
            .workers
            .track(move || RunningReadOutput {
                result: source.lineage_page(selected, &head, request, &cancelled),
                _release: release,
            });
        let job = cx.background_executor().spawn(async move { work.run() });
        let task_cancel = cancel.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let output = job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                let RunningReadOutput { result, _release } = output;
                if !root
                    .running_threads
                    .lineage
                    .page_jobs
                    .iter()
                    .any(|(pending, _, _)| *pending == request)
                {
                    return;
                }
                root.running_threads
                    .lineage
                    .page_jobs
                    .retain(|(pending, _, _)| *pending != request);
                if task_cancel.is_cancelled()
                    || window.window_handle() != native
                    || root.running_threads.generation.load(Ordering::Acquire) != generation
                    || !root.lineage_current(request.query, cx)
                    || root.running_threads.lineage.selected != Some(selected)
                    || !root
                        .running_threads
                        .reader
                        .as_ref()
                        .is_some_and(|current| current.same_publication(&reader))
                {
                    if let Some(widget) = root.running_threads.lineage.widget.clone() {
                        widget.update(cx, |widget, cx| {
                            widget.settle_page(request, None, false, window, cx)
                        });
                    }
                    return;
                }
                if let Some(widget) = root.running_threads.lineage.widget.clone() {
                    match result {
                        Ok((observed, page)) => {
                            if reader
                                .elect(&observed, || {
                                    widget.update(cx, |widget, cx| {
                                        widget.settle_page(request, Some(page), false, window, cx)
                                    })
                                })
                                .is_err()
                            {
                                widget.update(cx, |widget, cx| {
                                    widget.settle_page(request, None, false, window, cx)
                                });
                            }
                        }
                        Err(_) => widget.update(cx, |widget, cx| {
                            widget.settle_page(request, None, true, window, cx)
                        }),
                    }
                }
                cx.notify();
            });
        });
        self.running_threads
            .lineage
            .page_jobs
            .push((request, cancel, task));
    }

    pub(in crate::main_window::shell) fn focused_thread_lineage_owner(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> Option<gpui::FocusHandle> {
        self.running_threads
            .lineage
            .widget
            .as_ref()?
            .read(cx)
            .focused_owner(window)
    }

    pub(in crate::main_window::shell::host) fn render_selected_lineage(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<Entity<ThreadLineage>> {
        let widget = self.running_threads.lineage.widget.clone()?;
        let selected = self.running_threads.lineage.selected?;
        if self.running_threads.transcript_claim != Some(selected.claim())
            || self
                .running_threads
                .selected_title
                .as_ref()
                .map(|(identity, _)| *identity)
                != Some(selected)
        {
            return None;
        }
        let query = widget.read(cx).query();
        let reason = if self.running_threads.has_activation_custody() {
            Some("This window is settling its original thread selection.".into())
        } else if let Some(reason) = self.switcher_disabled_reason(cx) {
            Some(reason)
        } else if !self.lineage_current(query, cx) {
            Some("Checking this thread's current ancestry.".into())
        } else {
            None
        };
        let appearance = self.controller.as_ref()?.appearance().clone();
        widget.update(cx, |widget, cx| {
            widget.set_appearance(appearance, cx);
            widget.set_inert_reason(reason, cx);
        });
        Some(widget)
    }
}

#[cfg(all(test, feature = "test-faults"))]
mod test_access {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/running_threads_lineage.rs"
    ));
}

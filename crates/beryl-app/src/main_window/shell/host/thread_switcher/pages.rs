use super::*;

enum PageRows {
    Threads(Vec<beryl_state::CatalogQueryRow>),
    Roots(Vec<beryl_state::CatalogRootRow>),
    Runtimes(Vec<beryl_state::CatalogRuntimeRow>),
}
impl PageRows {
    fn len(&self) -> usize {
        match self {
            Self::Threads(rows) => rows.len(),
            Self::Roots(rows) => rows.len(),
            Self::Runtimes(rows) => rows.len(),
        }
    }
}
struct PageAssembly {
    request: PickerPageRequest,
    runtime: bool,
    chunks: usize,
    count: Option<usize>,
    rows: PageRows,
}
impl MainWindowShellRoot {
    pub(super) fn switcher_request_page(
        &mut self,
        request: PickerPageRequest,
        runtime: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = if runtime {
            PickerCollectionKey("switcher-runtimes".into())
        } else {
            self.thread_switcher.mode.key()
        };
        if request.collection_key != key || request.query_revision != self.thread_switcher.revision
        {
            return;
        }
        if self.thread_switcher.opening_pending
            || self.thread_switcher.refinement_pending
            || self.thread_switcher.anchor.is_none()
        {
            return;
        }
        if self
            .thread_switcher
            .page_requests
            .iter()
            .any(|(kind, pending)| *kind == runtime && pending == &request)
        {
            return;
        }
        if self.thread_switcher.page_requests.len() >= 4 {
            self.switcher_settle_page_error(
                request,
                runtime,
                "The catalog is waiting for its pending pages.".into(),
                window,
                cx,
            );
            return;
        }
        let rows = if runtime {
            PageRows::Runtimes(Vec::new())
        } else if matches!(self.thread_switcher.mode, SwitcherMode::Roots(_)) {
            PageRows::Roots(Vec::new())
        } else {
            PageRows::Threads(Vec::new())
        };
        self.thread_switcher
            .page_requests
            .push((runtime, request.clone()));
        self.switcher_read_chunk(
            PageAssembly {
                request,
                runtime,
                chunks: 0,
                count: None,
                rows,
            },
            window,
            cx,
        );
    }

    fn switcher_read_chunk(
        &mut self,
        mut page: PageAssembly,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = page.request.range.start.saturating_add(page.rows.len());
        let cancellation = CommandCancellation::new();
        let limit = CatalogQueryPageLimit::maximum();
        let query = self.thread_switcher.query.clone();
        let request = match &page.rows {
            PageRows::Runtimes(_) => self.thread_switcher.anchor.as_ref().map(|anchor| {
                anchor.runtime_page(query, offset as u64, limit, cancellation.clone())
            }),
            PageRows::Roots(_) => {
                if let SwitcherMode::Roots(runtime) = &self.thread_switcher.mode {
                    self.thread_switcher.anchor.as_ref().map(|anchor| {
                        anchor.root_page(
                            runtime.runtime_id(),
                            query,
                            offset as u64,
                            limit,
                            cancellation.clone(),
                        )
                    })
                } else {
                    None
                }
            }
            PageRows::Threads(_) => self
                .thread_switcher
                .collection()
                .map(|collection| collection.page_at(offset as u64, limit, cancellation.clone())),
        }
        .ok_or_else(|| "The exact frozen catalog collection is unavailable.".to_owned())
        .and_then(|request| request.map_err(|error| error.to_string()));
        let failed = (page.request.clone(), page.runtime);
        let result = request.and_then(|request| {
            self.switcher_spawn(
                request,
                cancellation,
                window,
                cx,
                move |root, result, window, cx| {
                    let added = (|| -> Result<usize, String> {
                        let result = result?;
                        let (actual_offset, count, added) = match (&mut page.rows, result) {
                            (PageRows::Threads(rows), PublishedCatalogQueryResult::Page(chunk)) => {
                                let added = chunk.rows().len();
                                rows.extend_from_slice(chunk.rows());
                                (chunk.offset(), root.thread_switcher.count, added)
                            }
                            (PageRows::Roots(rows), PublishedCatalogQueryResult::Roots(chunk)) => {
                                if Some(chunk.home_revision()) != root.thread_switcher.home_revision
                                {
                                    return Err("The root page belongs to another snapshot.".into());
                                }
                                let count = usize::try_from(chunk.count())
                                    .map_err(|_| "The root count cannot be represented.")?;
                                let added = chunk.rows().len();
                                rows.extend_from_slice(chunk.rows());
                                (chunk.offset(), count, added)
                            }
                            (
                                PageRows::Runtimes(rows),
                                PublishedCatalogQueryResult::Runtimes(chunk),
                            ) => {
                                if Some(chunk.home_revision()) != root.thread_switcher.home_revision
                                {
                                    return Err(
                                        "The runtime page belongs to another snapshot.".into()
                                    );
                                }
                                let count = usize::try_from(chunk.count())
                                    .map_err(|_| "The runtime count cannot be represented.")?;
                                let added = chunk.rows().len();
                                rows.extend_from_slice(chunk.rows());
                                (chunk.offset(), count, added)
                            }
                            _ => {
                                return Err(
                                    "The catalog page kind did not match its request.".into()
                                );
                            }
                        };
                        if actual_offset != offset as u64
                            || added > 16
                            || page.count.is_some_and(|prior| prior != count)
                            || (added == 0 && offset < count)
                        {
                            return Err("The catalog page range was invalid.".into());
                        }
                        page.count = Some(count);
                        Ok(added)
                    })();
                    match added {
                        Ok(added) => {
                            page.chunks += 1;
                            let end = page.request.range.start.saturating_add(page.rows.len());
                            if page.chunks < 2
                                && added > 0
                                && end < page.count.unwrap_or(0)
                                && end < page.request.range.end
                            {
                                root.switcher_read_chunk(page, window, cx);
                            } else {
                                root.switcher_publish_page(page, window, cx);
                            }
                        }
                        Err(error) => root.switcher_settle_page_error(
                            page.request,
                            page.runtime,
                            error,
                            window,
                            cx,
                        ),
                    }
                },
            )
        });
        if let Err(error) = result {
            self.switcher_settle_page_error(failed.0, failed.1, error, window, cx);
        }
    }

    fn switcher_publish_page(
        &mut self,
        page: PageAssembly,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.thread_switcher.picker.clone() else {
            return;
        };
        let total_count = page.count.unwrap_or(0);
        let initial_focus = !page.runtime
            && self.thread_switcher.focus_pending
            && self.thread_switcher.command.is_none();
        self.thread_switcher
            .page_requests
            .retain(|(runtime, request)| *runtime != page.runtime || request != &page.request);
        self.thread_switcher.failure = None;
        match page.rows {
            PageRows::Runtimes(rows) => {
                self.thread_switcher
                    .runtime_requests
                    .retain(|request| request != &page.request);
                let rows = rows
                    .iter()
                    .map(|row| self.switcher_runtime_row(row))
                    .collect();
                picker.update(cx, |picker, pcx| {
                    picker.settle_runtime_page(
                        PickerRuntimePageOutcome::Success(PickerRuntimePage {
                            request: page.request,
                            total_count,
                            rows,
                        }),
                        window,
                        pcx,
                    )
                });
                self.thread_switcher.runtime_ready = true;
            }
            PageRows::Threads(rows) => {
                let rows = rows
                    .iter()
                    .map(|row| self.switcher_thread_row(row, cx))
                    .collect();
                picker.update(cx, |picker, pcx| {
                    picker.settle_page(
                        PickerPageOutcome::Success(PickerPage {
                            request: page.request,
                            total_count,
                            rows,
                        }),
                        window,
                        pcx,
                    )
                });
                self.thread_switcher.primary_ready = true;
            }
            PageRows::Roots(rows) => {
                let rows = rows.iter().map(|row| self.switcher_root_row(row)).collect();
                picker.update(cx, |picker, pcx| {
                    picker.settle_page(
                        PickerPageOutcome::Success(PickerPage {
                            request: page.request,
                            total_count,
                            rows,
                        }),
                        window,
                        pcx,
                    )
                });
                self.thread_switcher.primary_ready = true;
            }
        }
        if initial_focus {
            self.thread_switcher.focus_pending = false;
            picker.update(cx, |picker, pcx| {
                picker.set_external_command_reason(None, pcx);
                picker.focus_search(window, pcx);
            });
        }
        self.switcher_complete_refresh(window, cx);
        cx.notify();
    }

    fn switcher_settle_page_error(
        &mut self,
        request: PickerPageRequest,
        runtime: bool,
        error: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.thread_switcher
            .page_requests
            .retain(|(kind, pending)| *kind != runtime || pending != &request);
        self.thread_switcher
            .runtime_requests
            .retain(|pending| pending != &request);
        self.thread_switcher.failure = Some(error.clone());
        if let Some(picker) = self.thread_switcher.picker.clone() {
            picker.update(cx, |picker, pcx| {
                if runtime {
                    picker.settle_runtime_page(
                        PickerRuntimePageOutcome::Failed {
                            request,
                            message: error,
                        },
                        window,
                        pcx,
                    );
                } else {
                    picker.settle_page(
                        PickerPageOutcome::Failed {
                            request,
                            message: error,
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

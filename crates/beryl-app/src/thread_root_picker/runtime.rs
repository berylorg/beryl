use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerRuntimeRow {
    pub row: PickerRow,
    pub browse_roots: PickerCommandState,
    pub add_root: PickerCommandState,
    pub active_scope: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerRuntimePage {
    pub request: PickerPageRequest,
    pub total_count: usize,
    pub rows: Vec<PickerRuntimeRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PickerRuntimePageOutcome {
    Success(PickerRuntimePage),
    Failed {
        request: PickerPageRequest,
        message: String,
    },
    Cancelled(PickerPageRequest),
}

pub struct PickerRuntimeCollection {
    pub(super) collection: PickerCollection,
    pages: Vec<PickerRuntimePage>,
}

impl PickerRuntimeCollection {
    pub fn new(key: PickerCollectionKey, revision: u64, total: usize) -> Self {
        Self {
            collection: PickerCollection::new(key, revision, total),
            pages: Vec::new(),
        }
    }

    pub fn replace(&mut self, key: PickerCollectionKey, revision: u64, total: usize) {
        if key != self.collection.key {
            self.pages.clear();
        }
        self.collection.replace(key, revision, total);
    }

    pub fn request(&mut self, position: usize) -> Option<PickerPageRequest> {
        self.collection.request(position)
    }

    pub fn row(&self, position: usize) -> Option<&PickerRuntimeRow> {
        self.pages.iter().find_map(|page| {
            position
                .checked_sub(page.request.range.start)
                .and_then(|index| page.rows.get(index))
        })
    }

    pub fn pending_requests(&self) -> &[PickerPageRequest] {
        self.collection.pending_requests()
    }
    pub fn total_count(&self) -> usize {
        self.collection.total_count()
    }
    pub fn resident_row_count(&self) -> usize {
        self.pages.iter().map(|page| page.rows.len()).sum()
    }
    pub fn focused_key(&self) -> Option<&PickerRowKey> {
        self.collection.focused_key()
    }
    pub fn focus_position(&mut self, position: usize) -> bool {
        self.collection.focus_position(position)
    }
    pub fn navigate(
        &mut self,
        direction: PickerNavigation,
        visible_rows: usize,
    ) -> (Option<usize>, Option<PickerPageRequest>) {
        self.collection.navigate(direction, visible_rows)
    }

    pub fn retain_window(&mut self, range: std::ops::Range<usize>) {
        self.collection.retain_window(range);
        self.prune_pages();
    }

    pub fn settle(
        &mut self,
        outcome: PickerRuntimePageOutcome,
        traversal_owns_focus: bool,
    ) -> Option<usize> {
        let (outcome, runtime_page) = match outcome {
            PickerRuntimePageOutcome::Success(page) => {
                if !self.collection.pending_requests().contains(&page.request) {
                    return None;
                }
                if page.rows.len() > PICKER_PAGE_ROWS {
                    return self.collection.settle(
                        PickerPageOutcome::Failed {
                            request: page.request,
                            message: "The runtime page was invalid.".into(),
                        },
                        traversal_owns_focus,
                    );
                }
                (
                    PickerPageOutcome::Success(PickerPage {
                        request: page.request.clone(),
                        total_count: page.total_count,
                        rows: page
                            .rows
                            .iter()
                            .map(|runtime| runtime.row.clone())
                            .collect(),
                    }),
                    Some(page),
                )
            }
            PickerRuntimePageOutcome::Failed { request, message } => {
                (PickerPageOutcome::Failed { request, message }, None)
            }
            PickerRuntimePageOutcome::Cancelled(request) => {
                (PickerPageOutcome::Cancelled(request), None)
            }
        };
        let position = self.collection.settle(outcome, traversal_owns_focus);
        self.prune_pages();
        if let Some(page) = runtime_page {
            if self
                .collection
                .pages
                .iter()
                .any(|resident| resident.request == page.request)
            {
                self.pages
                    .retain(|resident| resident.request.range.start != page.request.range.start);
                self.pages.push(page);
            }
        }
        position
    }

    fn prune_pages(&mut self) {
        self.pages.retain(|page| {
            self.collection
                .pages
                .iter()
                .any(|resident| resident.request == page.request)
        });
    }

    pub(super) fn update_row_command(
        &mut self,
        key: &PickerRowKey,
        browse: bool,
        state: PickerCommandState,
    ) {
        for row in self.pages.iter_mut().flat_map(|page| &mut page.rows) {
            if &row.row.key == key {
                if browse {
                    row.browse_roots = state.clone();
                } else {
                    row.add_root = state.clone();
                }
            }
        }
    }
}

pub(super) struct RuntimeViewport {
    pub config: PickerRuntimeSectionConfig,
    pub rows: PickerRuntimeCollection,
    pub focus: FocusHandle,
    pub scroll: ScrollHandle,
    pub owner: ScrollbarOwnerKey,
    pub state: ScrollbarState,
    pub visibility: ScrollbarVisibilityPolicy,
    pub interaction: ScrollbarInteraction,
    pub last_coherent_scroll: f32,
    pub last_realized: usize,
}

impl ThreadRootPicker {
    pub fn configure_runtime_section(
        &mut self,
        config: PickerRuntimeSectionConfig,
        key: PickerCollectionKey,
        revision: u64,
        total: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        if let Some(runtime) = &mut self.full.runtime {
            runtime.config = config;
            if runtime.rows.collection.key != key {
                runtime.scroll.set_offset(point(px(0.), px(0.)));
                runtime.last_coherent_scroll = 0.;
            }
            runtime.rows.replace(key, revision, total);
            cx.notify();
            return;
        }
        let focus = cx.focus_handle();
        self._subscriptions
            .push(cx.on_blur(&focus, window, |this, _, cx| {
                if let Some(runtime) = &mut this.full.runtime {
                    runtime.rows.collection.cancel_navigation();
                }
                cx.notify();
            }));
        let owner = ScrollbarOwnerKey {
            owner_id: ScrollbarOwnerId::new(cx.entity_id().as_u64()),
            mount_generation: ScrollbarMountGeneration::new(2),
        };
        let state = ScrollbarState::new(owner);
        let visibility = state.managed(Rc::new(|_, window, _| window.refresh()));
        let scroll = ScrollHandle::new();
        let entity = cx.weak_entity();
        let interaction = ScrollbarInteraction::for_scroll_handle_with_owner_update(
            owner,
            scroll.clone(),
            Axis::Vertical,
            move |_, _, cx| {
                let _ = entity.update(cx, |this, cx| {
                    if this.full.native_dialog_open || this.full.in_flight.is_some() {
                        if let Some(runtime) = &mut this.full.runtime {
                            runtime
                                .scroll
                                .set_offset(point(px(0.), px(-runtime.last_coherent_scroll)));
                        }
                        cx.notify();
                        return;
                    }
                    if let Some(runtime) = &mut this.full.runtime {
                        runtime.rows.collection.cancel_navigation();
                    }
                    this.request_runtime_viewport(cx);
                    cx.notify();
                });
            },
        );
        self.full.runtime = Some(RuntimeViewport {
            config,
            rows: PickerRuntimeCollection::new(key, revision, total),
            focus,
            scroll,
            owner,
            state,
            visibility,
            interaction,
            last_coherent_scroll: 0.,
            last_realized: 0,
        });
        cx.notify();
    }

    pub fn request_runtime_initial_page(&mut self, cx: &mut Context<Self>) {
        if self.dismissed {
            return;
        }
        if let Some(request) = self
            .full
            .runtime
            .as_mut()
            .and_then(|runtime| runtime.rows.request(0))
        {
            cx.emit(PickerEvent::RequestRuntimePage(request));
        }
    }

    pub fn replace_runtime_collection(
        &mut self,
        key: PickerCollectionKey,
        revision: u64,
        total: usize,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        if let Some(runtime) = &mut self.full.runtime {
            if runtime.rows.collection.key != key {
                runtime.scroll.set_offset(point(px(0.), px(0.)));
                runtime.last_coherent_scroll = 0.;
            }
            runtime.rows.replace(key, revision, total);
        }
        self.request_runtime_viewport(cx);
        cx.notify();
    }

    pub fn settle_runtime_page(
        &mut self,
        outcome: PickerRuntimePageOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        if let Some(runtime) = &mut self.full.runtime {
            let position = runtime
                .rows
                .settle(outcome, runtime.focus.is_focused(window));
            if runtime.rows.collection.failure.is_some() {
                runtime
                    .scroll
                    .set_offset(point(px(0.), px(-runtime.last_coherent_scroll)));
            }
            if let Some(position) = position {
                self.reveal_runtime(position);
            }
        }
        cx.notify();
    }

    pub fn runtime_diagnostics(&self) -> Option<PickerDiagnostics> {
        let runtime = self.full.runtime.as_ref()?;
        let (visible, realized) = self.runtime_ranges();
        Some(PickerDiagnostics {
            collection_key: runtime.rows.collection.key.clone(),
            query_revision: runtime.rows.collection.revision,
            total_count: runtime.rows.total_count(),
            resident_page_count: runtime.rows.collection.resident_page_count(),
            resident_row_count: runtime.rows.resident_row_count(),
            pending_page_count: runtime.rows.pending_requests().len(),
            visible_range: visible,
            realized_range: realized,
            realized_row_count: runtime.last_realized,
            focused_key: runtime.rows.focused_key().cloned(),
            selected_key: None,
            pending_navigation: runtime.rows.collection.target.clone(),
            scroll_offset: -f32::from(runtime.scroll.offset().y),
            collection_failed: runtime.rows.collection.failure.is_some(),
        })
    }

    pub(super) fn runtime_ranges(&self) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let Some(runtime) = &self.full.runtime else {
            return (0..0, 0..0);
        };
        bounded_ranges(
            runtime.rows.total_count(),
            -f32::from(runtime.scroll.offset().y),
            self.config.style.runtime_viewport_height,
            self.config.style.runtime_row_stride(),
        )
    }

    pub(super) fn request_runtime_viewport(&mut self, cx: &mut Context<Self>) {
        if self.dismissed {
            return;
        }
        let (_, range) = self.runtime_ranges();
        let Some(runtime) = &mut self.full.runtime else {
            return;
        };
        if runtime.rows.collection.failure.is_some() {
            return;
        }
        runtime.rows.retain_window(range.clone());
        let position = range
            .clone()
            .find(|position| {
                !runtime.rows.collection.is_current() || runtime.rows.row(*position).is_none()
            })
            .or_else(|| range.is_empty().then_some(0));
        if let Some(request) = position.and_then(|position| runtime.rows.request(position)) {
            cx.emit(PickerEvent::RequestRuntimePage(request));
        }
    }

    pub(super) fn reveal_runtime(&mut self, position: usize) {
        let style = self.config.style;
        if let Some(runtime) = &mut self.full.runtime {
            reveal_position(
                &runtime.scroll,
                position,
                style.runtime_row_stride(),
                style.runtime_row_height,
                style.runtime_viewport_height,
            );
        }
    }

    pub(super) fn release_runtime(&mut self, window: &mut Window, cx: &mut gpui::App) {
        if let Some(runtime) = &mut self.full.runtime {
            runtime.rows.collection.cancel_navigation();
            runtime.rows.collection.requests.clear();
            runtime.state.unmount_viewport(runtime.owner, window, cx);
        }
    }
}

pub(super) fn bounded_ranges(
    total: usize,
    offset: f32,
    height: f32,
    stride: f32,
) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
    let offset = offset.max(0.);
    let start = ((offset / stride).floor() as usize).min(total);
    let end = (((offset + height) / stride).ceil() as usize).min(total);
    let realized_start = start.saturating_sub(PICKER_OVERSCAN_ROWS);
    let realized_end = end
        .saturating_add(PICKER_OVERSCAN_ROWS)
        .min(total)
        .min(realized_start.saturating_add(PICKER_MAX_REALIZED_ROWS));
    (start..end, realized_start..realized_end)
}

pub(super) fn reveal_position(
    scroll: &ScrollHandle,
    position: usize,
    stride: f32,
    row_height: f32,
    height: f32,
) {
    let offset = (-f32::from(scroll.offset().y)).max(0.);
    let top = position as f32 * stride;
    let bottom = top + row_height;
    let next = if top < offset {
        top
    } else if bottom > offset + height {
        bottom - height
    } else {
        offset
    };
    scroll.set_offset(point(px(0.), px(-next.max(0.))));
}

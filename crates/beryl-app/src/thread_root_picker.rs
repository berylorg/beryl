mod model;
mod render;
mod style;

pub use model::*;
pub use style::ThreadRootPickerStyle;

use gpui::{
    AppContext, Context, Entity, FocusHandle, Focusable, KeyDownEvent, ScrollHandle, Subscription,
    Window, point, px,
};
use gpui_scrollbar::{
    Axis, ScrollbarInteraction, ScrollbarMountGeneration, ScrollbarOwnerId, ScrollbarOwnerKey,
    ScrollbarState, ScrollbarStyle, ScrollbarVisibilityPolicy,
};
use gpui_text_input::{TextInput, TextInputEvent, TextInputOptions};
use std::{rc::Rc, sync::Arc};

pub struct ThreadRootPickerConfig {
    pub title: String,
    pub helper: String,
    pub heading: String,
    pub empty_text: String,
    pub search_placeholder: String,
    pub owner_focus: FocusHandle,
    pub appearance: Option<Arc<crate::theme_runtime::AppearanceGeneration>>,
    pub style: ThreadRootPickerStyle,
    pub scrollbar_style: ScrollbarStyle,
}

pub struct ThreadRootPicker {
    config: ThreadRootPickerConfig,
    collection: PickerCollection,
    search: Entity<TextInput>,
    collection_focus: FocusHandle,
    scroll: ScrollHandle,
    scrollbar_owner: ScrollbarOwnerKey,
    scrollbar_state: ScrollbarState,
    visibility: ScrollbarVisibilityPolicy,
    _subscriptions: Vec<Subscription>,
    dismissed: bool,
    activation_in_flight: Option<PickerRowKey>,
    last_realized: usize,
    last_coherent_scroll: f32,
    scrollbar_interaction: ScrollbarInteraction,
    query: String,
}

impl gpui::EventEmitter<PickerEvent> for ThreadRootPicker {}

impl ThreadRootPicker {
    pub fn new(
        config: ThreadRootPickerConfig,
        key: PickerCollectionKey,
        revision: u64,
        total: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        gpui_text_input::ensure_text_input_bindings(cx);
        let placeholder = config.search_placeholder.clone();
        let search = cx.new(|cx| {
            TextInput::new_with_options(
                "",
                placeholder,
                TextInputOptions::single_line()
                    .with_undo_limit(16)
                    .with_undo_byte_limit(64 * 1024),
                cx,
            )
        });
        let collection_focus = cx.focus_handle();
        let subscriptions = vec![
            cx.subscribe_in(&search, window, |this, input, event, window, cx| {
                if matches!(event, TextInputEvent::Changed(_)) {
                    let mut query = input.read(cx).text().to_owned();
                    if query.len() > PICKER_QUERY_BYTES {
                        let mut end = PICKER_QUERY_BYTES;
                        while !query.is_char_boundary(end) {
                            end -= 1;
                        }
                        query.truncate(end);
                        input.update(cx, |input, cx| {
                            input.set_text(query.clone(), cx);
                        });
                    }
                    this.change_query(query, window, cx);
                }
            }),
            cx.on_blur(&collection_focus, window, |this, _, cx| {
                this.collection.cancel_navigation();
                cx.notify();
            }),
            cx.on_release_in(window, |this, window, cx| {
                this.collection.cancel_navigation();
                this.collection.requests.clear();
                this.scrollbar_state
                    .unmount_viewport(this.scrollbar_owner, window, cx);
            }),
        ];
        let scrollbar_owner = ScrollbarOwnerKey {
            owner_id: ScrollbarOwnerId::new(cx.entity_id().as_u64()),
            mount_generation: ScrollbarMountGeneration::new(1),
        };
        let scrollbar_state = ScrollbarState::new(scrollbar_owner);
        let visibility = scrollbar_state.managed(Rc::new(|_, window, _| window.refresh()));
        let scroll = ScrollHandle::new();
        let entity = cx.weak_entity();
        let scrollbar_interaction = ScrollbarInteraction::for_scroll_handle_with_owner_update(
            scrollbar_owner,
            scroll.clone(),
            Axis::Vertical,
            move |_, _, cx| {
                let _ = entity.update(cx, |this, cx| {
                    this.collection.cancel_navigation();
                    this.request_viewport(cx);
                    cx.notify();
                });
            },
        );
        Self {
            config,
            collection: PickerCollection::new(key, revision, total),
            search,
            collection_focus,
            scroll,
            scrollbar_owner,
            scrollbar_state,
            visibility,
            _subscriptions: subscriptions,
            dismissed: false,
            activation_in_flight: None,
            last_realized: 0,
            last_coherent_scroll: 0.,
            scrollbar_interaction,
            query: String::new(),
        }
    }

    pub fn search_input(&self) -> Entity<TextInput> {
        self.search.clone()
    }
    pub fn collection_focus(&self) -> FocusHandle {
        self.collection_focus.clone()
    }
    pub fn focused_key(&self) -> Option<&PickerRowKey> {
        self.collection.focused_key()
    }
    pub fn focused_position(&self) -> Option<usize> {
        self.collection.focused_position()
    }
    pub fn resolve_removed_focus(
        &mut self,
        position: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        let (reveal, request) = self.collection.resolve_removed_focus(position);
        if let Some(request) = request {
            cx.emit(PickerEvent::RequestPage(request));
        }
        if let Some(position) = reveal {
            if self.collection_focus.is_focused(window) {
                self.reveal(position);
            }
        }
        cx.notify();
    }
    pub fn pending_requests(&self) -> &[PickerPageRequest] {
        self.collection.pending_requests()
    }

    pub fn request_initial_page(&mut self, cx: &mut Context<Self>) {
        if !self.dismissed {
            if let Some(request) = self.collection.request(0) {
                cx.emit(PickerEvent::RequestPage(request));
            }
        }
    }

    pub fn replace_collection(
        &mut self,
        key: PickerCollectionKey,
        revision: u64,
        total: usize,
        focus_position: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        let changed_key = self.collection.key != key;
        self.collection.replace(key, revision, total);
        self.activation_in_flight = None;
        if changed_key {
            self.scroll.set_offset(point(px(0.), px(0.)));
        }
        if let Some(position) = focus_position {
            self.restore_focus_position(position, window, cx);
        }
        self.request_viewport(cx);
        cx.notify();
    }

    pub fn restore_focus_position(
        &mut self,
        position: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        let (reveal, request) = self.collection.restore_focus_position(position);
        if let Some(request) = request {
            cx.emit(PickerEvent::RequestPage(request));
        }
        if let Some(position) = reveal {
            if self.collection_focus.is_focused(window) {
                self.reveal(position);
            }
        }
        cx.notify();
    }

    pub fn settle_page(
        &mut self,
        outcome: PickerPageOutcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }
        let position = self
            .collection
            .settle(outcome, self.collection_focus.is_focused(window));
        if self.collection.failure.is_some() {
            self.scroll
                .set_offset(point(px(0.), px(-self.last_coherent_scroll)));
        }
        if let Some(position) = position {
            self.reveal(position);
        }
        cx.notify();
    }

    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.collection.cancel_navigation();
        self.search.update(cx, |input, cx| input.focus(window, cx));
    }

    pub fn focus_row(&mut self, position: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dismissed && self.collection.focus_position(position) {
            self.collection_focus.focus(window);
            self.reveal(position);
            cx.notify();
        }
    }

    pub fn navigate(
        &mut self,
        direction: PickerNavigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed || !self.collection_focus.is_focused(window) {
            return;
        }
        let rows =
            (self.config.style.viewport_height() / self.config.style.row_stride()).floor() as usize;
        let (position, request) = self.collection.navigate(direction, rows);
        if let Some(position) = position {
            self.reveal(position);
        }
        if let Some(request) = request {
            cx.emit(PickerEvent::RequestPage(request));
        }
        cx.notify();
    }

    pub fn activate(&mut self, key: &PickerRowKey, cx: &mut Context<Self>) {
        if self.dismissed || self.activation_in_flight.is_some() {
            return;
        }
        if let Some(event) = self.collection.activation(key) {
            self.activation_in_flight = Some(key.clone());
            cx.emit(event);
            cx.notify();
        }
    }

    pub fn finish_activation(&mut self, cx: &mut Context<Self>) {
        self.activation_in_flight = None;
        cx.notify();
    }

    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dismissed {
            return;
        }
        self.dismissed = true;
        self.collection.cancel_navigation();
        self.collection.requests.clear();
        self.scrollbar_state
            .unmount_viewport(self.scrollbar_owner, window, cx);
        self.config.owner_focus.focus(window);
        cx.emit(PickerEvent::Dismiss);
        cx.notify();
    }

    pub fn set_appearance(
        &mut self,
        appearance: Arc<crate::theme_runtime::AppearanceGeneration>,
        cx: &mut Context<Self>,
    ) {
        self.config.appearance = Some(appearance);
        cx.notify();
    }
    pub fn set_style(&mut self, style: ThreadRootPickerStyle, cx: &mut Context<Self>) {
        self.config.style = style;
        self.request_viewport(cx);
        cx.notify();
    }

    pub fn diagnostics(&self) -> PickerDiagnostics {
        let (visible, realized) = self.ranges();
        PickerDiagnostics {
            collection_key: self.collection.key.clone(),
            query_revision: self.collection.revision,
            total_count: self.collection.total,
            resident_page_count: self.collection.pages.len(),
            resident_row_count: self
                .collection
                .pages
                .iter()
                .map(|page| page.rows.len())
                .sum(),
            pending_page_count: self.collection.requests.len(),
            visible_range: visible,
            realized_range: realized,
            realized_row_count: self.last_realized,
            focused_key: self.collection.focused_key().cloned(),
            pending_navigation: self.collection.target.clone(),
            scroll_offset: -f32::from(self.scroll.offset().y),
            collection_failed: self.collection.failure.is_some(),
        }
    }

    fn change_query(&mut self, query: String, _: &mut Window, cx: &mut Context<Self>) {
        if self.dismissed || self.query == query {
            return;
        }
        let Some(revision) = self.collection.revision.checked_add(1) else {
            self.collection.failure = Some("Search revision capacity was exhausted.".into());
            cx.notify();
            return;
        };
        self.collection
            .replace(self.collection.key.clone(), revision, self.collection.total);
        self.query = query.clone();
        self.activation_in_flight = None;
        cx.emit(PickerEvent::QueryChanged {
            collection_key: self.collection.key.clone(),
            query_revision: revision,
            query,
        });
        self.request_initial_page(cx);
        cx.notify();
    }

    fn ranges(&self) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let stride = self.config.style.row_stride();
        let offset = (-f32::from(self.scroll.offset().y)).max(0.);
        let start = ((offset / stride).floor() as usize).min(self.collection.total);
        let end = (((offset + self.config.style.viewport_height()) / stride).ceil() as usize)
            .min(self.collection.total);
        let realized_start = start.saturating_sub(PICKER_OVERSCAN_ROWS);
        let realized_end = end
            .saturating_add(PICKER_OVERSCAN_ROWS)
            .min(self.collection.total)
            .min(realized_start.saturating_add(PICKER_MAX_REALIZED_ROWS));
        (start..end, realized_start..realized_end)
    }

    fn request_viewport(&mut self, cx: &mut Context<Self>) {
        if self.dismissed || self.collection.failure.is_some() {
            return;
        }
        let (_, mut range) = self.ranges();
        if range.is_empty() {
            self.request_initial_page(cx);
            return;
        }
        self.collection.retain_window(range.clone());
        if self.collection.is_current() && self.collection.pages.len() == PICKER_MAX_RESIDENT_PAGES
        {
            return;
        }
        if let Some(position) = range.find(|position| {
            !self.collection.is_current() || self.collection.row(*position).is_none()
        }) {
            if let Some(request) = self.collection.request(position) {
                cx.emit(PickerEvent::RequestPage(request));
            }
        }
    }

    fn reveal(&mut self, position: usize) {
        let stride = self.config.style.row_stride();
        let viewport = self.config.style.viewport_height();
        let offset = (-f32::from(self.scroll.offset().y)).max(0.);
        let top = position as f32 * stride;
        let bottom = top + self.config.style.row_height;
        let next = if top < offset {
            top
        } else if bottom > offset + viewport {
            bottom - viewport
        } else {
            offset
        };
        self.scroll.set_offset(point(px(0.), px(-next.max(0.))));
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                self.dismiss(window, cx);
            }
            "tab" => {
                self.collection.cancel_navigation();
                if self.collection_focus.is_focused(window) {
                    self.focus_search(window, cx);
                } else {
                    self.collection_focus.focus(window);
                    if self.collection.focused_key().is_none() {
                        let first = self.ranges().0.start;
                        self.collection.focus_position(first);
                    }
                }
                cx.stop_propagation();
                cx.notify();
            }
            key if self.collection_focus.is_focused(window) => {
                let direction = match key {
                    "up" => Some(PickerNavigation::Up),
                    "down" => Some(PickerNavigation::Down),
                    "home" => Some(PickerNavigation::Home),
                    "end" => Some(PickerNavigation::End),
                    "pageup" => Some(PickerNavigation::PageUp),
                    "pagedown" => Some(PickerNavigation::PageDown),
                    _ => None,
                };
                if let Some(direction) = direction {
                    cx.stop_propagation();
                    self.navigate(direction, window, cx);
                } else if key == "enter" && !event.is_held {
                    cx.stop_propagation();
                    if let Some(key) = self.collection.focused_key().cloned() {
                        self.activate(&key, cx);
                    }
                }
            }
            _ => {}
        }
    }
}

impl Focusable for ThreadRootPicker {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.search.read(cx).focus_handle(cx)
    }
}

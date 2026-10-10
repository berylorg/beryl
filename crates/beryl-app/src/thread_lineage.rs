mod model;
mod render;
mod style;
pub use model::*;

use gpui::{AppContext, Context, FocusHandle, Window, point, px, size};
use gpui_scrollbar::{
    Axis, ScrollbarInteraction, ScrollbarMountGeneration, ScrollbarOwnerId, ScrollbarOwnerKey,
    ScrollbarScrollState, ScrollbarState, ScrollbarStyle, ScrollbarVisibilityPolicy,
};
use std::{
    cell::RefCell, collections::hash_map::RandomState, hash::BuildHasher, rc::Rc, sync::Arc,
};

pub struct ThreadLineage {
    model: model::LineageModel,
    current_title: String,
    proxy: FocusHandle,
    handles: Vec<(beryl_model::SyndicThreadId, FocusHandle)>,
    appearance: Option<Arc<crate::theme_runtime::AppearanceGeneration>>,
    owner: ScrollbarOwnerKey,
    scrollbar: ScrollbarState,
    visibility: ScrollbarVisibilityPolicy,
    interaction: ScrollbarInteraction,
    scroll_state: Rc<RefCell<Option<ScrollbarScrollState>>>,
    bounds: gpui::Bounds<gpui::Pixels>,
    first_ordinal: u64,
    fraction: f32,
    auto_reveal: bool,
    inert: bool,
    inert_reason: Option<String>,
    realized: usize,
    tooltip: Option<beryl_model::SyndicThreadId>,
    hovered: Option<beryl_model::SyndicThreadId>,
    diagnostic_hash: RandomState,
    _release: gpui::Subscription,
}

#[derive(Clone, Debug)]
pub struct LineageDiagnostics {
    pub widget_instance: u64,
    pub selected_key: u64,
    pub query_key: u64,
    pub parent_count: u64,
    pub resident_pages: usize,
    pub pending_pages: usize,
    pub realized_items: usize,
    pub visible_key_range: Option<(u64, u64)>,
    pub overscan_items: usize,
    pub item_stride: f32,
    pub viewport_width: f32,
    pub content_width: f64,
    pub scroll_offset: f64,
    pub clamp_direction: i8,
    pub logical_focus_present: bool,
    pub viewport_focus_proxy_present: bool,
    pub current_endpoint_present: bool,
    pub tooltip_anchor_present: bool,
}

impl gpui::EventEmitter<LineageEvent> for ThreadLineage {}

impl ThreadLineage {
    pub fn new(
        query: LineageQuery,
        current_title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner = ScrollbarOwnerKey {
            owner_id: ScrollbarOwnerId::new(cx.entity_id().as_u64()),
            mount_generation: ScrollbarMountGeneration::new(1),
        };
        let scrollbar = ScrollbarState::new(owner);
        let visibility = scrollbar.managed(Rc::new(|_, window, _| window.refresh()));
        let scroll_state: Rc<RefCell<Option<ScrollbarScrollState>>> = Rc::new(RefCell::new(None));
        let state_reader = scroll_state.clone();
        let state_writer = scroll_state.clone();
        let weak = cx.weak_entity();
        let interaction = ScrollbarInteraction::new(
            move || *state_reader.borrow(),
            move |_, offset| {
                if let Some(state) = state_writer.borrow_mut().as_mut() {
                    state.scroll_offset.x = offset;
                }
            },
            |_, _, _| {},
            |_| {},
            |_| {},
            move |snapshot, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.scroll_from_bar(f32::from(snapshot.scroll_offset), window, cx);
                });
            },
        );
        let release = cx.on_release_in(window, |this, window, cx| {
            this.scrollbar.unmount_viewport(this.owner, window, cx);
            this.model.requests.clear();
            this.handles.clear();
            this.tooltip = None;
            this.hovered = None;
        });
        Self {
            model: model::LineageModel::new(query),
            current_title,
            proxy: cx.focus_handle(),
            handles: Vec::new(),
            appearance: None,
            owner,
            scrollbar,
            visibility,
            interaction,
            scroll_state,
            bounds: Default::default(),
            first_ordinal: 0,
            fraction: 0.,
            auto_reveal: true,
            inert: false,
            inert_reason: None,
            realized: 0,
            tooltip: None,
            hovered: None,
            diagnostic_hash: RandomState::new(),
            _release: release,
        }
    }

    pub fn replace(
        &mut self,
        query: LineageQuery,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected_changed = self.model.query.selected != query.selected;
        if self.model.query == query {
            self.current_title = title;
            return;
        }
        self.retire_realized_focus(window);
        self.model.replace(query);
        self.current_title = title;
        self.auto_reveal |= selected_changed;
        self.tooltip = None;
        self.hovered = None;
        let replacement = ScrollbarOwnerKey {
            owner_id: self.owner.owner_id,
            mount_generation: ScrollbarMountGeneration::new(query.revision),
        };
        self.scrollbar
            .replace_owner(self.owner, replacement, window, cx);
        self.owner = replacement;
        self.request_viewport(cx);
        cx.notify();
    }

    pub fn query(&self) -> LineageQuery {
        self.model.query
    }
    pub fn focused_owner(&self, window: &Window) -> Option<FocusHandle> {
        (self.proxy.is_focused(window)
            || self
                .handles
                .iter()
                .any(|(_, handle)| handle.is_focused(window)))
        .then(|| self.proxy.clone())
    }
    pub fn focus_proxy(&self) -> FocusHandle {
        self.proxy.clone()
    }

    pub fn settle_page(
        &mut self,
        request: LineagePageRequest,
        result: Option<LineagePage>,
        failed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let supplied = result.is_some();
        let pending = request.query == self.model.query && self.model.requests.contains(&request);
        let target = self.model.target;
        let accepted = self.model.settle(request, result, failed);
        if pending {
            if let Some(focus) = self
                .model
                .focus
                .filter(|focus| focus.query == self.model.query)
            {
                if self.proxy.is_focused(window)
                    && target.is_some_and(|(ordinal, _)| ordinal == focus.ordinal)
                {
                    self.reveal(focus.ordinal);
                }
            }
            if supplied && accepted {
                self.request_viewport(cx);
            }
            cx.notify();
        }
    }

    pub fn cancel_requests(&mut self, cx: &mut Context<Self>) {
        self.model.requests.clear();
        self.model.paused = true;
        cx.notify();
    }
    pub fn resume(&mut self, cx: &mut Context<Self>) {
        self.model.failure = false;
        self.model.paused = false;
        self.request_viewport(cx);
    }

    fn request_viewport(&mut self, cx: &mut Context<Self>) {
        if self.inert {
            return;
        }
        if let Some((ordinal, _)) = self.model.target {
            if let Some(request) = self.model.request(ordinal) {
                cx.emit(LineageEvent::RequestPage(request));
            }
        }
        let (_, realized) = self.ranges();
        for ordinal in realized {
            if let Some(request) = self.model.request(ordinal) {
                cx.emit(LineageEvent::RequestPage(request));
            }
        }
    }

    fn viewport_width(&self) -> f32 {
        f32::from(self.bounds.size.width).max(0.)
    }
    fn content_width(&self) -> f64 {
        self.model.query.parent_count as f64 * self.stride() as f64 + self.current_width() as f64
    }
    fn offset(&self) -> f64 {
        self.first_ordinal as f64 * self.stride() as f64 + self.fraction as f64
    }
    fn ranges(&self) -> (std::ops::Range<u64>, std::ops::Range<u64>) {
        let start = self.first_ordinal.min(self.model.query.parent_count);
        let count = ((self.viewport_width() + self.fraction) / self.stride())
            .ceil()
            .max(0.) as u64;
        let end = start
            .saturating_add(count)
            .min(self.model.query.parent_count);
        (
            start..end,
            start.saturating_sub(2)..end.saturating_add(2).min(self.model.query.parent_count),
        )
    }

    fn place_trailing(&mut self) {
        let visible_parent_width = (self.viewport_width() - self.current_width()).max(0.);
        let slots = (visible_parent_width / self.stride()).ceil() as u64;
        self.first_ordinal = self.model.query.parent_count.saturating_sub(slots);
        self.fraction =
            if self.first_ordinal == 0 && self.content_width() <= self.viewport_width() as f64 {
                0.
            } else {
                (slots as f32 * self.stride() - visible_parent_width).max(0.)
            };
    }

    fn reveal(&mut self, ordinal: u64) {
        if ordinal < self.first_ordinal {
            self.first_ordinal = ordinal;
            self.fraction = 0.;
        }
        let distance = ordinal.saturating_sub(self.first_ordinal) as f64 * self.stride() as f64
            - self.fraction as f64;
        if distance < 0. {
            self.first_ordinal = ordinal;
            self.fraction = 0.;
        } else if distance + self.breadcrumb_width() as f64 > self.viewport_width() as f64 {
            let slots = ((self.viewport_width() - self.breadcrumb_width()).max(0.) / self.stride())
                .floor() as u64;
            self.first_ordinal = ordinal.saturating_sub(slots);
            self.fraction = 0.;
        }
    }

    fn retire_realized_focus(&mut self, window: &mut Window) {
        if self
            .handles
            .iter()
            .any(|(_, handle)| handle.is_focused(window))
        {
            self.proxy.focus(window);
        }
        self.handles.clear();
    }

    fn scroll_by(&mut self, distance: f32, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !distance.is_finite()
            || distance == 0.
            || self.content_width() <= self.viewport_width() as f64
        {
            return false;
        }
        let before = (self.first_ordinal, self.fraction);
        let local = self.fraction as f64 + distance as f64;
        let slots = (local / self.stride() as f64).floor();
        if slots < 0. {
            self.first_ordinal = self.first_ordinal.saturating_sub((-slots) as u64);
        } else {
            self.first_ordinal = self.first_ordinal.saturating_add(slots as u64);
        }
        self.fraction = local.rem_euclid(self.stride() as f64) as f32;
        if slots < 0. && (-slots) as u64 > before.0 {
            self.fraction = 0.;
        }
        if self.offset() + self.viewport_width() as f64 > self.content_width() {
            self.place_trailing();
        }
        let moved = before != (self.first_ordinal, self.fraction);
        if moved {
            self.auto_reveal = false;
            self.reconcile_focus(window);
            self.tooltip = None;
            self.request_viewport(cx);
            self.visibility
                .record_viewport_activity(self.owner, window, cx);
            cx.notify();
        }
        moved
    }

    fn scroll_from_bar(&mut self, offset: f32, window: &mut Window, cx: &mut Context<Self>) {
        if !offset.is_finite() {
            return;
        }
        let position = offset as f64 / self.stride() as f64;
        self.first_ordinal = (position.floor() as u64).min(self.model.query.parent_count);
        self.fraction = (position.fract() * self.stride() as f64) as f32;
        if offset >= (self.content_width() - self.viewport_width() as f64).max(0.) as f32 {
            self.place_trailing();
        }
        self.auto_reveal = false;
        self.tooltip = None;
        self.reconcile_focus(window);
        self.request_viewport(cx);
        cx.notify();
    }

    fn reconcile_focus(&mut self, window: &mut Window) {
        let range = self.ranges().1;
        let departing = self.handles.iter().any(|(key, handle)| {
            handle.is_focused(window)
                && self
                    .model
                    .focus
                    .is_some_and(|focus| focus.thread == *key && !range.contains(&focus.ordinal))
        });
        if departing {
            self.proxy.focus(window);
        }
        self.handles.retain(|(key, _)| {
            range.clone().any(|ordinal| {
                self.model
                    .row(ordinal)
                    .is_some_and(|row| row.thread == *key)
            })
        });
        if self
            .tooltip
            .is_some_and(|key| !self.handles.iter().any(|(thread, _)| *thread == key))
        {
            self.tooltip = None;
            self.hovered = None;
        }
    }

    fn activate(
        &mut self,
        query: LineageQuery,
        thread: beryl_model::SyndicThreadId,
        cx: &mut Context<Self>,
    ) {
        if self.inert || self.model.failure || self.model.query != query {
            return;
        }
        let range = self.ranges().1;
        if range.into_iter().any(|ordinal| {
            self.model
                .row(ordinal)
                .is_some_and(|row| row.thread == thread && row.reason.is_none())
        }) {
            cx.emit(LineageEvent::Activate { query, thread });
        }
    }

    fn key_down(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.focused_owner(window).is_none() {
            return;
        }
        let movement = match event.keystroke.key.as_str() {
            "left" => Some(LineageMovement::Left),
            "right" => Some(LineageMovement::Right),
            "home" => Some(LineageMovement::Home),
            "end" => Some(LineageMovement::End),
            _ => None,
        };
        if let Some(movement) = movement {
            if let Some(ordinal) = self.model.movement(movement) {
                self.auto_reveal = false;
                self.proxy.focus(window);
                self.reveal(ordinal);
                self.request_viewport(cx);
                self.visibility
                    .record_viewport_activity(self.owner, window, cx);
                cx.notify();
            }
            cx.stop_propagation();
        } else if matches!(event.keystroke.key.as_str(), "enter" | "space") && !event.is_held {
            if let Some(focus) = self.model.focus.filter(|_| self.model.target.is_none()) {
                self.activate(focus.query, focus.thread, cx);
            }
            cx.stop_propagation();
        }
    }

    pub fn diagnostics(&self, window: &Window, instance: u64) -> LineageDiagnostics {
        let (visible, realized) = self.ranges();
        let key = |id: beryl_model::SyndicThreadId| self.diagnostic_hash.hash_one(id);
        let first = visible
            .clone()
            .find_map(|ordinal| self.model.row(ordinal))
            .map(|row| key(row.thread));
        let last = visible
            .clone()
            .rev()
            .find_map(|ordinal| self.model.row(ordinal))
            .map(|row| key(row.thread));
        LineageDiagnostics {
            widget_instance: instance,
            selected_key: key(self.model.query.selected),
            query_key: self.diagnostic_hash.hash_one(self.model.query.revision),
            parent_count: self.model.query.parent_count,
            resident_pages: self.model.resident_count(),
            pending_pages: self.model.requests.len(),
            realized_items: self.realized,
            visible_key_range: first.zip(last),
            overscan_items: (realized.end - realized.start)
                .saturating_sub(visible.end - visible.start) as usize,
            item_stride: self.stride(),
            viewport_width: self.viewport_width(),
            content_width: self.content_width(),
            scroll_offset: self.offset(),
            clamp_direction: if self.offset() == 0. {
                -1
            } else if self.offset() + self.viewport_width() as f64 >= self.content_width() {
                1
            } else {
                0
            },
            logical_focus_present: self.model.focus.is_some(),
            viewport_focus_proxy_present: self.proxy.is_focused(window),
            current_endpoint_present: self.first_ordinal <= self.model.query.parent_count
                && (self.model.query.parent_count - self.first_ordinal) as f64
                    * self.stride() as f64
                    - (self.fraction as f64)
                    < self.viewport_width() as f64,
            tooltip_anchor_present: self.tooltip.is_some(),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/thread_lineage.rs"]
mod tests;

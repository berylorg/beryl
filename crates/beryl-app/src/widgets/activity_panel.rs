pub mod model;

use std::{
    cell::RefCell, collections::hash_map::RandomState, hash::BuildHasher, rc::Rc, sync::Arc,
};

use gpui::{AppContext, Context, FocusHandle, Window, point, px, size};
use gpui_scrollbar::{
    Axis, ScrollbarInteraction, ScrollbarMountGeneration, ScrollbarOwnerId, ScrollbarOwnerKey,
    ScrollbarScrollState, ScrollbarState, ScrollbarStyle, ScrollbarVisibilityPolicy,
};
use model::{Demand, Feedback, Model, Page, Query};

mod interaction;
mod render;
mod style;
#[cfg(feature = "test-faults")]
pub mod test_support;
mod tooltip;

#[derive(Clone, Debug)]
pub(crate) enum Event {
    Demand(Demand),
    Viewport(Demand),
    Retry {
        query: Query,
        feedback: u64,
    },
    Height {
        query: Query,
        value: f32,
        committed: bool,
    },
    ResizeCancelled {
        query: Query,
        value: f32,
    },
}

#[derive(Clone, Debug)]
pub struct Diagnostics {
    pub instance: u64,
    pub query: u64,
    pub height: f32,
    pub resizing: bool,
    pub total: u64,
    pub resident_pages: usize,
    pub pending_pages: usize,
    pub realized_rows: usize,
    pub visible_keys: Option<(u64, u64)>,
    pub overscan: u64,
    pub row_height: f32,
    pub offset: f64,
    pub top_attached: bool,
    pub reconciliations: u64,
    pub tooltip: bool,
    pub inert: bool,
}

struct Resize {
    y: f32,
    height: f32,
    released: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Gate {
    Current,
    Busy,
    Retired,
}

pub(crate) type InputGate = Rc<dyn Fn(&mut dyn FnMut()) -> Gate>;
pub(crate) fn probe(gate: &InputGate) -> Gate {
    gate(&mut || {})
}

pub(crate) struct ActivityPanel {
    model: Model,
    height: f32,
    minimum: f32,
    maximum: f32,
    handle_focus: FocusHandle,
    viewport_focus: FocusHandle,
    retry_focus: FocusHandle,
    safe_focus: FocusHandle,
    gate: Option<InputGate>,
    resizing: Option<Resize>,
    handle_keyboard: bool,
    resize_step: f32,
    resize_page: f32,
    tooltip: tooltip::State,
    realized: usize,
    reconciliations: u64,
    hash: RandomState,
    appearance: Option<Arc<crate::theme_runtime::AppearanceGeneration>>,
    bounds: gpui::Bounds<gpui::Pixels>,
    owner: ScrollbarOwnerKey,
    scrollbar: ScrollbarState,
    visibility: ScrollbarVisibilityPolicy,
    interaction: ScrollbarInteraction,
    scroll_state: Rc<RefCell<Option<ScrollbarScrollState>>>,
    _release: gpui::Subscription,
    _keyboard: gpui::Subscription,
    _activation: gpui::Subscription,
}

impl gpui::EventEmitter<Event> for ActivityPanel {}

impl ActivityPanel {
    #[cfg(feature = "test-faults")]
    pub(crate) fn test_focus_resize(&self, window: &mut Window) {
        self.handle_focus.focus(window);
    }

    fn input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        action: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>),
    ) -> Gate {
        if self.model.inert {
            return Gate::Retired;
        }
        let Some(gate) = self.gate.clone() else {
            return Gate::Retired;
        };
        let mut action = Some(action);
        let result = gate(&mut || (action.take().expect("one input election"))(self, window, cx));
        if result == Gate::Retired {
            self.set_inert(window, cx);
        }
        result
    }

    pub(crate) fn new(
        safe_focus: FocusHandle,
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
        let reader = scroll_state.clone();
        let writer = scroll_state.clone();
        let weak = cx.weak_entity();
        let interaction = ScrollbarInteraction::new(
            move || *reader.borrow(),
            move |_, offset| {
                if let Some(state) = writer.borrow_mut().as_mut() {
                    state.scroll_offset.y = offset;
                }
            },
            |_, _, _| {},
            |_| {},
            |_| {},
            move |snapshot, window, app| {
                let _ = weak.update(app, |panel, cx| {
                    if panel.current(window, cx) {
                        panel.scroll_from_bar(f32::from(snapshot.scroll_offset), window, cx);
                    }
                });
            },
        );
        let release = cx.on_release_in(window, |panel, window, app| {
            panel.teardown(window, app);
        });
        let native = window.window_handle();
        let weak = cx.weak_entity();
        let keyboard = cx.intercept_keystrokes(move |_, window, app| {
            if window.window_handle() == native {
                let _ = weak.update(app, |panel, cx| {
                    if !panel.model.inert && !panel.handle_keyboard {
                        panel.handle_keyboard = true;
                        cx.notify();
                    }
                });
            }
        });
        let activation = cx.observe_window_activation(window, |panel, _, cx| {
            if let Some(resize) = panel.resizing.take() {
                panel.height = resize.height.clamp(panel.minimum, panel.maximum);
                panel.set_viewport_height();
                cx.emit(Event::ResizeCancelled {
                    query: panel.model.query,
                    value: panel.height,
                });
                cx.notify();
            }
        });
        let mut model = Model::new(Query {
            owner: 0,
            period: 0,
            revision: 0,
            total: 0,
        });
        model.set_viewport(model::DEFAULT_HEIGHT - model::HANDLE_HEIGHT - 1.);
        Self {
            model,
            height: model::DEFAULT_HEIGHT,
            minimum: model::MIN_HEIGHT,
            maximum: 320.,
            handle_focus: cx.focus_handle(),
            viewport_focus: cx.focus_handle(),
            retry_focus: cx.focus_handle(),
            safe_focus,
            gate: None,
            resizing: None,
            handle_keyboard: false,
            resize_step: 8.,
            resize_page: 32.,
            tooltip: Default::default(),
            realized: 0,
            reconciliations: 0,
            hash: RandomState::new(),
            appearance: None,
            bounds: Default::default(),
            owner,
            scrollbar,
            visibility,
            interaction,
            scroll_state,
            _release: release,
            _keyboard: keyboard,
            _activation: activation,
        }
    }

    pub(crate) fn replace(
        &mut self,
        query: Query,
        pages: Vec<Arc<Page>>,
        gate: InputGate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), &'static str> {
        let changed = self.model.query != query;
        let same_scope =
            self.model.query.owner == query.owner && self.model.query.period == query.period;
        let was_inert = self.model.inert;
        if self.model.query.owner != query.owner || self.model.query.period != query.period {
            self.teardown(window, cx);
        }
        self.model.replace(query, pages)?;
        if was_inert {
            let generation = self
                .owner
                .mount_generation
                .get()
                .checked_add(1)
                .ok_or("Activity scrollbar generations are exhausted")?;
            let replacement = ScrollbarOwnerKey {
                owner_id: self.owner.owner_id,
                mount_generation: ScrollbarMountGeneration::new(generation),
            };
            self.scrollbar
                .replace_owner(self.owner, replacement, window, cx);
            self.owner = replacement;
        }
        self.gate = Some(gate);
        self.model.inert = false;
        if same_scope && self.resizing.as_ref().is_some_and(|resize| resize.released) {
            self.resizing = None;
            cx.emit(Event::Height {
                query: self.model.query,
                value: self.height,
                committed: true,
            });
        }
        if changed {
            self.reconciliations = self.reconciliations.saturating_add(1);
            if self.retry_focus.is_focused(window) {
                self.viewport_focus.focus(window);
            }
        }
        self.request(cx);
        cx.notify();
        Ok(())
    }

    pub(crate) fn feedback(
        &mut self,
        feedback: Option<Feedback>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.feedback == feedback {
            return;
        }
        let removed = self.model.feedback.is_some() && feedback.is_none();
        if self.model.set_feedback(feedback).is_err() {
            return;
        }
        if removed && self.retry_focus.is_focused(window) {
            if self.model.inert {
                self.safe_focus.focus(window);
            } else {
                self.viewport_focus.focus(window);
            }
        }
        self.set_viewport_height();
        self.request(cx);
        cx.notify();
    }

    pub(crate) fn allocation(&mut self, height: f32, maximum: f32, cx: &mut Context<Self>) {
        if !height.is_finite() || !maximum.is_finite() || maximum < model::MIN_HEIGHT {
            return;
        }
        if self.maximum == maximum && self.height == height.clamp(self.minimum, maximum) {
            return;
        }
        self.maximum = maximum;
        self.height = height.clamp(self.minimum, self.maximum);
        self.set_viewport_height();
        self.request(cx);
        cx.notify();
    }

    pub(crate) fn height(&self) -> f32 {
        self.height
    }

    pub(crate) fn resize_origin(&self) -> Option<f32> {
        self.resizing.as_ref().map(|resize| resize.height)
    }

    pub(crate) fn set_inert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.model.inert {
            self.teardown(window, cx);
            self.model.inert = true;
            self.gate = None;
            cx.notify();
        }
    }

    fn teardown(&mut self, window: &mut Window, cx: &mut gpui::App) {
        if let Some(resize) = self.resizing.take() {
            self.height = resize.height.clamp(self.minimum, self.maximum);
        }
        self.tooltip.clear();
        self.model.pending = None;
        *self.scroll_state.borrow_mut() = None;
        self.scrollbar.unmount_viewport(self.owner, window, cx);
        if self.handle_focus.is_focused(window)
            || self.viewport_focus.is_focused(window)
            || self.retry_focus.is_focused(window)
        {
            self.safe_focus.focus(window);
        }
    }

    fn current(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.model.inert {
            return false;
        }
        match self.gate.as_ref().map(probe).unwrap_or(Gate::Retired) {
            Gate::Current => true,
            Gate::Busy => false,
            Gate::Retired => {
                self.set_inert(window, cx);
                false
            }
        }
    }

    fn set_viewport_height(&mut self) {
        self.model.set_viewport(
            (self.height
                - 1.
                - model::HANDLE_HEIGHT
                - if self.model.feedback.is_some() {
                    model::FEEDBACK_HEIGHT
                } else {
                    0.
                })
            .max(0.),
        );
    }

    fn request(&mut self, cx: &mut Context<Self>) {
        if let Some(request) = self.model.request() {
            cx.emit(Event::Demand(request));
        }
    }

    pub(crate) fn diagnostics(&self) -> Diagnostics {
        let range = self.model.visible();
        let mut keys = self
            .model
            .pages
            .iter()
            .flat_map(|page| {
                page.rows
                    .iter()
                    .enumerate()
                    .map(move |(index, row)| (page.start + index as u64, row))
            })
            .filter(|(ordinal, _)| range.contains(ordinal))
            .map(|(_, row)| self.hash.hash_one(row.id));
        let first = keys.next();
        let last = keys.last().or(first);
        Diagnostics {
            instance: self.owner.owner_id.get(),
            query: self.hash.hash_one((
                self.model.query.owner,
                self.model.query.period,
                self.model.query.revision,
            )),
            height: self.height,
            resizing: self.resizing.is_some(),
            total: self.model.query.total,
            resident_pages: self.model.pages.len(),
            pending_pages: usize::from(self.model.pending.is_some()),
            realized_rows: self.realized,
            visible_keys: first.zip(last),
            overscan: model::OVERSCAN,
            row_height: model::ROW_HEIGHT,
            offset: self.model.offset(),
            top_attached: self.model.top_attached,
            reconciliations: self.reconciliations,
            tooltip: self.tooltip.ready
                && self.tooltip.anchor.is_some()
                && self
                    .gate
                    .as_ref()
                    .is_some_and(|gate| probe(gate) == Gate::Current),
            inert: self.model.inert,
        }
    }
}

use std::{cell::RefCell, rc::Rc};

use gpui::{
    AnyElement, App, Bounds, Context, ElementId, MouseButton, Pixels, Point, Render, ScrollHandle,
    Size, Window, div, point, prelude::*, px, rgb,
};

#[path = "scrollbars/geometry.rs"]
mod geometry;

#[allow(unused_imports)]
pub(crate) use geometry::{
    ScrollbarAxisHit, ScrollbarMetrics, classify_scrollbar_axis_hit, scroll_offset_from_thumb_drag,
    scrollbar_metrics, scrollbar_thumb_grab_offset, scrollbar_track_geometry,
};

const SCROLLBAR_THICKNESS: f32 = 4.0;
const SCROLLBAR_INSET: f32 = 6.0;
const SCROLLBAR_MIN_THUMB_LENGTH: f32 = 24.0;
const SCROLLBAR_HIT_LANE_THICKNESS: f32 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScrollbarAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy)]
pub(super) struct ScrollbarScrollState {
    pub viewport_bounds: Bounds<Pixels>,
    pub max_offset: Size<Pixels>,
    pub offset: Point<Pixels>,
}

pub(super) type ScrollbarActivityCallback = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone)]
pub(super) struct ScrollbarInteraction {
    state: Rc<dyn Fn() -> Option<ScrollbarScrollState>>,
    set_scroll_offset: Rc<dyn Fn(Pixels)>,
    page_scroll: Rc<dyn Fn(Pixels)>,
    drag_started: Rc<dyn Fn()>,
    drag_ended: Rc<dyn Fn()>,
    on_activity: Option<ScrollbarActivityCallback>,
}

impl ScrollbarInteraction {
    pub(super) fn new(
        state: impl Fn() -> Option<ScrollbarScrollState> + 'static,
        set_scroll_offset: impl Fn(Pixels) + 'static,
        page_scroll: impl Fn(Pixels) + 'static,
        drag_started: impl Fn() + 'static,
        drag_ended: impl Fn() + 'static,
        on_activity: Option<ScrollbarActivityCallback>,
    ) -> Self {
        Self {
            state: Rc::new(state),
            set_scroll_offset: Rc::new(set_scroll_offset),
            page_scroll: Rc::new(page_scroll),
            drag_started: Rc::new(drag_started),
            drag_ended: Rc::new(drag_ended),
            on_activity,
        }
    }

    fn for_scroll_handle(
        scroll_handle: ScrollHandle,
        axis: ScrollbarAxis,
        on_activity: Option<ScrollbarActivityCallback>,
    ) -> Self {
        Self::new(
            {
                let scroll_handle = scroll_handle.clone();
                move || {
                    Some(ScrollbarScrollState {
                        viewport_bounds: scroll_handle.bounds(),
                        max_offset: scroll_handle.max_offset(),
                        offset: scroll_handle.offset(),
                    })
                }
            },
            {
                let scroll_handle = scroll_handle.clone();
                move |scroll_offset| {
                    let max_offset = scrollbar_axis_max_offset(axis, scroll_handle.max_offset());
                    let scroll_offset = scroll_offset.clamp(px(0.0), max_offset);
                    let current_offset = scroll_handle.offset();
                    scroll_handle.set_offset(match axis {
                        ScrollbarAxis::Horizontal => point(-scroll_offset, current_offset.y),
                        ScrollbarAxis::Vertical => point(current_offset.x, -scroll_offset),
                    });
                }
            },
            {
                let scroll_handle = scroll_handle.clone();
                move |distance| {
                    let current_scroll_offset =
                        scrollbar_axis_scroll_offset(axis, scroll_handle.offset());
                    let max_offset = scrollbar_axis_max_offset(axis, scroll_handle.max_offset());
                    let next_scroll_offset =
                        (current_scroll_offset + distance).clamp(px(0.0), max_offset);
                    let current_offset = scroll_handle.offset();
                    scroll_handle.set_offset(match axis {
                        ScrollbarAxis::Horizontal => point(-next_scroll_offset, current_offset.y),
                        ScrollbarAxis::Vertical => point(current_offset.x, -next_scroll_offset),
                    });
                }
            },
            || {},
            || {},
            on_activity,
        )
    }

    fn current_state(&self) -> Option<ScrollbarScrollState> {
        (self.state)()
    }

    fn set_scroll_offset(&self, offset: Pixels) {
        (self.set_scroll_offset)(offset);
    }

    fn page_scroll(&self, distance: Pixels) {
        (self.page_scroll)(distance);
    }

    fn drag_started(&self) {
        (self.drag_started)();
    }

    fn drag_ended(&self) {
        (self.drag_ended)();
    }

    fn record_activity(&self, window: &mut Window, cx: &mut App) {
        if let Some(on_activity) = self.on_activity.as_ref() {
            on_activity(window, cx);
        }
        window.refresh();
    }
}

pub(super) fn render_div_scrollbar(
    id: impl Into<ElementId>,
    scroll_handle: &ScrollHandle,
    axis: ScrollbarAxis,
    opacity: f32,
    on_activity: Option<ScrollbarActivityCallback>,
) -> Option<AnyElement> {
    if opacity <= 0.0 {
        return None;
    }
    let bounds = scroll_handle.bounds();
    let max_offset = scroll_handle.max_offset();
    let offset = scroll_handle.offset();
    let metrics = match axis {
        ScrollbarAxis::Horizontal => {
            scrollbar_metrics(bounds.size.width, max_offset.width, -offset.x)?
        }
        ScrollbarAxis::Vertical => {
            scrollbar_metrics(bounds.size.height, max_offset.height, -offset.y)?
        }
    };
    Some(render_scrollbar(
        Some(id.into()),
        axis,
        metrics,
        opacity,
        Some(ScrollbarInteraction::for_scroll_handle(
            scroll_handle.clone(),
            axis,
            on_activity,
        )),
    ))
}

pub(super) fn render_interactive_vertical_scrollbar(
    id: impl Into<ElementId>,
    viewport_length: Pixels,
    overflow_length: Pixels,
    scroll_offset: Pixels,
    opacity: f32,
    interaction: ScrollbarInteraction,
) -> Option<AnyElement> {
    if opacity <= 0.0 {
        return None;
    }
    let metrics = scrollbar_metrics(viewport_length, overflow_length, scroll_offset)?;
    Some(render_scrollbar(
        Some(id.into()),
        ScrollbarAxis::Vertical,
        metrics,
        opacity,
        Some(interaction),
    ))
}

fn render_scrollbar(
    id: Option<ElementId>,
    axis: ScrollbarAxis,
    metrics: ScrollbarMetrics,
    opacity: f32,
    interaction: Option<ScrollbarInteraction>,
) -> AnyElement {
    let Some(interaction) = interaction else {
        return render_scrollbar_thumb(axis, metrics, opacity).into_any_element();
    };
    let Some(id) = id else {
        return render_scrollbar_thumb(axis, metrics, opacity).into_any_element();
    };

    let pending_drag = Rc::new(RefCell::new(None));
    let active_drag = Rc::new(RefCell::new(None));
    let thumb = render_scrollbar_thumb(axis, metrics, opacity).into_any_element();
    let mut lane = match axis {
        ScrollbarAxis::Horizontal => div()
            .absolute()
            .left_0()
            .bottom_0()
            .w_full()
            .h(px(SCROLLBAR_HIT_LANE_THICKNESS)),
        ScrollbarAxis::Vertical => div()
            .absolute()
            .top_0()
            .right_0()
            .h_full()
            .w(px(SCROLLBAR_HIT_LANE_THICKNESS)),
    }
    .id(id)
    .child(thumb);

    let drag_value = ScrollbarDragValue {
        axis,
        interaction: interaction.clone(),
        pending_drag: pending_drag.clone(),
        active_drag: active_drag.clone(),
    };
    lane = lane
        .on_mouse_down(MouseButton::Left, {
            let interaction = interaction.clone();
            let pending_drag = pending_drag.clone();
            move |event, window, cx| {
                handle_scrollbar_mouse_down(
                    axis,
                    &interaction,
                    &pending_drag,
                    event.position,
                    window,
                    cx,
                );
                cx.stop_propagation();
            }
        })
        .on_drag(drag_value, |drag: &ScrollbarDragValue, _, window, cx| {
            drag.start(window, cx);
            cx.new(|_| ScrollbarDragPreview)
        })
        .on_drag_move::<ScrollbarDragValue>(
            move |event: &gpui::DragMoveEvent<ScrollbarDragValue>, window, cx| {
                let (axis, interaction, active_drag) = {
                    let drag = event.drag(cx);
                    (
                        drag.axis,
                        drag.interaction.clone(),
                        drag.active_drag.clone(),
                    )
                };
                update_scrollbar_drag(
                    axis,
                    &interaction,
                    &active_drag,
                    event.event.position,
                    window,
                    cx,
                );
                cx.stop_propagation();
            },
        );

    lane.into_any_element()
}

fn render_scrollbar_thumb(
    axis: ScrollbarAxis,
    metrics: ScrollbarMetrics,
    opacity: f32,
) -> impl IntoElement {
    match axis {
        ScrollbarAxis::Horizontal => div()
            .absolute()
            .left(px(SCROLLBAR_INSET) + metrics.thumb_offset)
            .bottom(px(SCROLLBAR_INSET))
            .h(px(SCROLLBAR_THICKNESS))
            .w(metrics.thumb_length)
            .rounded_full()
            .bg(rgb(0x94a3b8))
            .opacity(opacity),
        ScrollbarAxis::Vertical => div()
            .absolute()
            .top(px(SCROLLBAR_INSET) + metrics.thumb_offset)
            .right(px(SCROLLBAR_INSET))
            .w(px(SCROLLBAR_THICKNESS))
            .h(metrics.thumb_length)
            .rounded_full()
            .bg(rgb(0x94a3b8))
            .opacity(opacity),
    }
}

#[derive(Clone, Copy)]
enum PendingScrollbarDrag {
    Thumb { grab_offset: Pixels },
    Ignore,
}

#[derive(Clone, Copy)]
struct ScrollbarActiveDrag {
    grab_offset: Pixels,
}

struct ScrollbarDragValue {
    axis: ScrollbarAxis,
    interaction: ScrollbarInteraction,
    pending_drag: Rc<RefCell<Option<PendingScrollbarDrag>>>,
    active_drag: Rc<RefCell<Option<ScrollbarActiveDrag>>>,
}

impl ScrollbarDragValue {
    fn start(&self, window: &mut Window, cx: &mut App) {
        let pending = self.pending_drag.borrow_mut().take();
        let grab_offset = match pending {
            Some(PendingScrollbarDrag::Thumb { grab_offset }) => Some(grab_offset),
            Some(PendingScrollbarDrag::Ignore) => None,
            None => self.thumb_grab_offset_at(window.mouse_position()),
        };
        let Some(grab_offset) = grab_offset else {
            return;
        };

        self.interaction.drag_started();
        *self.active_drag.borrow_mut() = Some(ScrollbarActiveDrag { grab_offset });
        self.interaction.record_activity(window, cx);
    }

    fn thumb_grab_offset_at(&self, pointer_position: Point<Pixels>) -> Option<Pixels> {
        let state = self.interaction.current_state()?;
        let viewport_length = scrollbar_axis_length(self.axis, state.viewport_bounds.size);
        let overflow_length = scrollbar_axis_max_offset(self.axis, state.max_offset);
        let scroll_offset = scrollbar_axis_scroll_offset(self.axis, state.offset);
        let metrics = scrollbar_metrics(viewport_length, overflow_length, scroll_offset)?;
        let axis_position =
            scrollbar_axis_position_in_viewport(self.axis, pointer_position, state.viewport_bounds);
        matches!(
            classify_scrollbar_axis_hit(viewport_length, metrics, axis_position),
            Some(ScrollbarAxisHit::Thumb)
        )
        .then(|| scrollbar_thumb_grab_offset(viewport_length, metrics, axis_position))
        .flatten()
    }
}

impl Drop for ScrollbarDragValue {
    fn drop(&mut self) {
        if self.active_drag.borrow_mut().take().is_some() {
            self.interaction.drag_ended();
        }
    }
}

fn update_scrollbar_drag(
    axis: ScrollbarAxis,
    interaction: &ScrollbarInteraction,
    active_drag: &Rc<RefCell<Option<ScrollbarActiveDrag>>>,
    pointer_position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(active_drag) = *active_drag.borrow() else {
        return;
    };
    let Some(state) = interaction.current_state() else {
        return;
    };
    let viewport_length = scrollbar_axis_length(axis, state.viewport_bounds.size);
    let overflow_length = scrollbar_axis_max_offset(axis, state.max_offset);
    let scroll_offset = scrollbar_axis_scroll_offset(axis, state.offset);
    let Some(metrics) = scrollbar_metrics(viewport_length, overflow_length, scroll_offset) else {
        return;
    };
    let pointer_axis_position =
        scrollbar_axis_position_in_viewport(axis, pointer_position, state.viewport_bounds);
    let Some(next_scroll_offset) = scroll_offset_from_thumb_drag(
        viewport_length,
        overflow_length,
        metrics,
        pointer_axis_position,
        active_drag.grab_offset,
    ) else {
        return;
    };

    interaction.set_scroll_offset(next_scroll_offset);
    interaction.record_activity(window, cx);
}

struct ScrollbarDragPreview;

impl Render for ScrollbarDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(0.0)).h(px(0.0))
    }
}

fn handle_scrollbar_mouse_down(
    axis: ScrollbarAxis,
    interaction: &ScrollbarInteraction,
    pending_drag: &Rc<RefCell<Option<PendingScrollbarDrag>>>,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(state) = interaction.current_state() else {
        *pending_drag.borrow_mut() = Some(PendingScrollbarDrag::Ignore);
        return;
    };
    let viewport_length = scrollbar_axis_length(axis, state.viewport_bounds.size);
    let overflow_length = scrollbar_axis_max_offset(axis, state.max_offset);
    let scroll_offset = scrollbar_axis_scroll_offset(axis, state.offset);
    let Some(metrics) = scrollbar_metrics(viewport_length, overflow_length, scroll_offset) else {
        *pending_drag.borrow_mut() = Some(PendingScrollbarDrag::Ignore);
        return;
    };
    let axis_position = scrollbar_axis_position_in_viewport(axis, position, state.viewport_bounds);

    match classify_scrollbar_axis_hit(viewport_length, metrics, axis_position) {
        Some(ScrollbarAxisHit::Thumb) => {
            *pending_drag.borrow_mut() =
                scrollbar_thumb_grab_offset(viewport_length, metrics, axis_position)
                    .map(|grab_offset| PendingScrollbarDrag::Thumb { grab_offset });
        }
        Some(ScrollbarAxisHit::LaneBeforeThumb) if axis == ScrollbarAxis::Vertical => {
            *pending_drag.borrow_mut() = Some(PendingScrollbarDrag::Ignore);
            interaction.page_scroll(-viewport_length);
            interaction.record_activity(window, cx);
        }
        Some(ScrollbarAxisHit::LaneAfterThumb) if axis == ScrollbarAxis::Vertical => {
            *pending_drag.borrow_mut() = Some(PendingScrollbarDrag::Ignore);
            interaction.page_scroll(viewport_length);
            interaction.record_activity(window, cx);
        }
        Some(ScrollbarAxisHit::LaneBeforeThumb | ScrollbarAxisHit::LaneAfterThumb) | None => {
            *pending_drag.borrow_mut() = Some(PendingScrollbarDrag::Ignore);
        }
    }
}

fn scrollbar_axis_length(axis: ScrollbarAxis, size: Size<Pixels>) -> Pixels {
    match axis {
        ScrollbarAxis::Horizontal => size.width,
        ScrollbarAxis::Vertical => size.height,
    }
}

fn scrollbar_axis_max_offset(axis: ScrollbarAxis, max_offset: Size<Pixels>) -> Pixels {
    match axis {
        ScrollbarAxis::Horizontal => max_offset.width,
        ScrollbarAxis::Vertical => max_offset.height,
    }
}

fn scrollbar_axis_scroll_offset(axis: ScrollbarAxis, offset: Point<Pixels>) -> Pixels {
    match axis {
        ScrollbarAxis::Horizontal => -offset.x,
        ScrollbarAxis::Vertical => -offset.y,
    }
}

fn scrollbar_axis_position_in_viewport(
    axis: ScrollbarAxis,
    position: Point<Pixels>,
    viewport_bounds: Bounds<Pixels>,
) -> Pixels {
    match axis {
        ScrollbarAxis::Horizontal => position.x - viewport_bounds.left(),
        ScrollbarAxis::Vertical => position.y - viewport_bounds.top(),
    }
}

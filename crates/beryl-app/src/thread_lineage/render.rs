use super::*;
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, AnyView, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, canvas, div, prelude::FluentBuilder,
};

impl Render for ThreadLineage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.viewport_width() == 0. {
            self.bounds.size.width = (window.viewport_size().width - px(92.)).max(px(0.));
            self.bounds.size.height = px(32.);
        }
        if self.auto_reveal {
            self.place_trailing();
        } else if self.offset() + self.viewport_width() as f64 > self.content_width() {
            self.place_trailing();
        }
        self.reconcile_focus(window);
        if self.proxy.is_focused(window) && self.model.target.is_none() {
            if let Some(focus) = self.model.focus.filter(|focus| {
                self.ranges().1.contains(&focus.ordinal)
                    && self
                        .model
                        .row(focus.ordinal)
                        .is_some_and(|row| row.thread == focus.thread)
            }) {
                self.reveal(focus.ordinal);
            }
        }
        self.tooltip = self.hovered;
        self.request_viewport(cx);
        let query = self.model.query;
        let range = self.ranges().1;
        let stride = self.stride();
        let mut rows = Vec::new();
        for ordinal in range {
            let Some(row) = self.model.row(ordinal).cloned() else {
                continue;
            };
            let x = if ordinal >= self.first_ordinal {
                (ordinal - self.first_ordinal) as f32 * stride - self.fraction
            } else {
                -((self.first_ordinal - ordinal) as f32 * stride) - self.fraction
            };
            let focus = if let Some((_, focus)) = self
                .handles
                .iter()
                .find(|(thread, _)| *thread == row.thread)
            {
                focus.clone()
            } else {
                let focus = cx.focus_handle();
                self.handles.push((row.thread, focus.clone()));
                focus
            };
            if self.proxy.is_focused(window)
                && self.model.target.is_none()
                && self
                    .model
                    .focus
                    .is_some_and(|logical| logical.query == query && logical.thread == row.thread)
            {
                focus.focus(window);
            }
            let focused = focus.is_focused(window);
            if focused {
                self.tooltip = Some(row.thread);
            }
            rows.push(self.breadcrumb(ordinal, x, row, focus, focused, cx));
        }
        self.realized = rows.len();
        let current_distance = self
            .model
            .query
            .parent_count
            .saturating_sub(self.first_ordinal);
        let current_x = current_distance as f64 * stride as f64 - self.fraction as f64;
        let current_visible = current_x < self.viewport_width() as f64
            && current_x + self.current_width() as f64 > 0.;
        let endpoint_font = self.font("thread-lineage.current", 13., 600.);
        if current_visible {
            let title = self.current_title.clone();
            let tooltip = self.tooltip_presentation(title.clone());
            rows.push(
                div()
                    .id("thread-lineage-current")
                    .debug_selector(|| "thread-lineage-current".into())
                    .absolute()
                    .left(px(current_x as f32))
                    .top_0()
                    .h(px(31.))
                    .w(px(self.current_width()))
                    .flex()
                    .items_center()
                    .text_color(self.color(
                        "thread-lineage.current",
                        Property::Foreground,
                        0xe2e8f0,
                    ))
                    .text_size(px(endpoint_font.0))
                    .font_weight(endpoint_font.1)
                    .when_some(endpoint_font.2, |el, family| el.font_family(family))
                    .child(div().w_full().truncate().child(title.clone()))
                    .tooltip(move |_, cx| cx.new(|_| tooltip.clone()).into())
                    .into_any_element(),
            );
        }
        *self.scroll_state.borrow_mut() = Some(ScrollbarScrollState {
            owner: self.owner,
            viewport_bounds: self.bounds,
            content_size: size(px(self.content_width() as f32), px(32.)),
            scroll_offset: point(px(self.offset() as f32), px(0.)),
            page_distance: self.bounds.size,
        });
        let weak = cx.weak_entity();
        let measure = canvas(
            move |bounds, window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    if this.model.query != query {
                        return;
                    }
                    if this.bounds != bounds {
                        this.bounds = bounds;
                        cx.notify();
                    }
                    if this.auto_reveal && bounds.size.width > px(0.) {
                        this.place_trailing();
                        this.auto_reveal = false;
                        this.request_viewport(cx);
                        cx.notify();
                    }
                    if let Some(state) = this.scroll_state.borrow_mut().as_mut() {
                        state.viewport_bounds = bounds;
                        state.page_distance = bounds.size;
                    }
                    this.reconcile_focus(window);
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let mut viewport = div()
            .id("thread-lineage-viewport")
            .debug_selector(|| "thread-lineage-viewport".into())
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .track_focus(&self.proxy)
            .tab_stop(true)
            .on_key_down(cx.listener(Self::key_down))
            .on_scroll_wheel(
                cx.listener(|this, event: &gpui::ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(px(20.));
                    let horizontal = if event.modifiers.shift && delta.x == px(0.) {
                        delta.y
                    } else {
                        delta.x
                    };
                    if this.scroll_by(-f32::from(horizontal), window, cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, _, window, cx| {
                this.visibility
                    .record_viewport_activity(this.owner, window, cx)
            }))
            .child(measure)
            .children(rows);
        if let Some(bar) = gpui_scrollbar::render_scrollbar(
            "thread-lineage-scrollbar",
            self.scrollbar.clone(),
            Axis::Horizontal,
            self.scrollbar_style(),
            self.visibility.clone(),
            self.interaction.clone(),
        ) {
            viewport = viewport.child(bar);
        }
        let heading = self.font("thread-lineage.heading", 10., 700.);
        div()
            .id("thread-lineage")
            .debug_selector(|| "thread-lineage".into())
            .w_full()
            .min_w_0()
            .h(px(32.))
            .flex_none()
            .flex()
            .items_center()
            .px(px(12.))
            .gap(px(10.))
            .border_b_1()
            .border_color(self.color("thread-lineage", Property::Border, 0x334155))
            .bg(self.color("thread-lineage", Property::Background, 0x0f172a))
            .text_color(self.color("thread-lineage", Property::Foreground, 0xcbd5e1))
            .when(self.inert, |el| el.opacity(0.55))
            .child(
                div()
                    .flex_none()
                    .text_color(self.color(
                        "thread-lineage.heading",
                        Property::Foreground,
                        0x64748b,
                    ))
                    .text_size(px(heading.0))
                    .font_weight(heading.1)
                    .when_some(heading.2, |el, family| el.font_family(family))
                    .child("LINEAGE"),
            )
            .child(viewport)
    }
}

impl ThreadLineage {
    fn breadcrumb(
        &self,
        ordinal: u64,
        x: f32,
        row: LineageBreadcrumb,
        focus: FocusHandle,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = self.model.query;
        let thread = row.thread;
        let enabled = row.reason.is_none() && !self.inert && !self.model.failure;
        let key = self.diagnostic_hash.hash_one(thread);
        let id = gpui::SharedString::from(format!("thread-lineage-parent-{key:016x}"));
        let reason = self
            .inert_reason
            .as_deref()
            .or_else(|| {
                self.model.failure.then_some(
                    "The current ancestry could not be read. Waiting for a fresh source check.",
                )
            })
            .or(row.reason.as_deref());
        let tooltip_text = reason.map_or_else(
            || row.title.clone(),
            |reason| format!("{}\n{reason}", row.title),
        );
        let font = self.font("command-button", 13., 500.);
        let focus_ring = self.color("command-button.focused", Property::Color, 0x38bdf8);
        let hover = self.color("command-button.hover", Property::Background, 0x1e293b);
        let pressed = self.color("command-button.pressed", Property::Background, 0x263449);
        let focused_tooltip = self.tooltip_presentation(tooltip_text.clone());
        let hovered_tooltip = focused_tooltip.clone();
        let weak = cx.weak_entity();
        let tooltip_anchor = canvas(
            move |bounds, window, app| {
                if focused {
                    let visible = weak.clone();
                    window.set_tooltip(gpui::AnyTooltip {
                        view: app.new(|_| focused_tooltip).into(),
                        mouse_position: bounds.bottom_right(),
                        check_visible_and_update: Rc::new(move |_, window, app| {
                            visible.upgrade().is_some_and(|entity| {
                                entity.read(app).model.query == query
                                    && entity.read(app).handles.iter().any(|(key, focus)| {
                                        *key == thread && focus.is_focused(window)
                                    })
                            })
                        }),
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let element = div()
            .id(gpui::SharedString::from(format!(
                "thread-lineage-parent-{thread:?}"
            )))
            .debug_selector(move || id.to_string())
            .h(px(28.))
            .w(px(self.breadcrumb_width()))
            .track_focus(&focus)
            .tab_stop(false)
            .flex_none()
            .flex()
            .items_center()
            .px(px(8.))
            .rounded(px(6.))
            .border_1()
            .border_color(if focused {
                focus_ring
            } else {
                self.color("command-button", Property::Border, 0x334155)
            })
            .bg(self.color("command-button", Property::Background, 0x172033))
            .text_color(self.color(
                if enabled {
                    "thread-lineage.breadcrumb"
                } else {
                    "thread-lineage.breadcrumb.unavailable"
                },
                Property::Foreground,
                if enabled { 0x7dd3fc } else { 0x64748b },
            ))
            .text_size(px(font.0))
            .font_weight(font.1)
            .when_some(font.2, |el, family| el.font_family(family))
            .when(enabled, |el| {
                el.cursor_pointer()
                    .hover(move |el| el.bg(hover))
                    .active(move |el| el.bg(pressed))
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if this.model.query == query
                        && this
                            .model
                            .row(ordinal)
                            .is_some_and(|row| row.thread == thread)
                    {
                        this.model.focus_position(ordinal);
                        if let Some((_, handle)) =
                            this.handles.iter().find(|(key, _)| *key == thread)
                        {
                            handle.focus(window);
                        }
                        window.prevent_default();
                        cx.notify();
                    }
                }),
            )
            .on_click(cx.listener(move |this, event, _, cx| {
                if matches!(event, gpui::ClickEvent::Mouse(_)) {
                    this.activate(query, thread, cx);
                }
            }))
            .on_key_down(cx.listener(Self::key_down))
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if this.model.query == query {
                    if *hovered {
                        this.tooltip = Some(thread);
                        this.hovered = Some(thread);
                    } else if this.tooltip == Some(thread) {
                        this.tooltip = None;
                        this.hovered = None;
                    }
                    cx.notify();
                }
            }))
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| hovered_tooltip.clone()).into() })
            .relative()
            .child(tooltip_anchor)
            .child(div().w_full().truncate().child(row.title));
        div()
            .absolute()
            .left(px(x))
            .top_0()
            .h(px(31.))
            .w(px(self.stride()))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(element)
            .child(
                div()
                    .w(px(12.))
                    .flex_none()
                    .text_color(self.color(
                        "thread-lineage.separator",
                        Property::Foreground,
                        0x64748b,
                    ))
                    .child("›"),
            )
            .into_any_element()
    }
}

#[derive(Clone)]
pub(super) struct LineageTooltip {
    pub(super) text: String,
    background: gpui::Rgba,
    foreground: gpui::Rgba,
    font: (f32, gpui::FontWeight, Option<gpui::SharedString>),
}
impl ThreadLineage {
    fn tooltip_presentation(&self, text: String) -> LineageTooltip {
        LineageTooltip {
            text,
            background: self.color("tooltip", Property::Background, 0x172033),
            foreground: self.color("tooltip", Property::Foreground, 0xf1f5f9),
            font: self.font("tooltip", 12., 400.),
        }
    }
}
impl Render for LineageTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("thread-lineage-tooltip")
            .debug_selector(|| "thread-lineage-tooltip".into())
            .max_w(px(420.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .bg(self.background)
            .text_color(self.foreground)
            .text_size(px(self.font.0))
            .font_weight(self.font.1)
            .when_some(self.font.2.clone(), |el, family| el.font_family(family))
            .child(self.text.clone())
    }
}

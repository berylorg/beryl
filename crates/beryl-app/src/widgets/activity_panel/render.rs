use super::*;
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, canvas, div, prelude::FluentBuilder,
};

type Font = (f32, gpui::FontWeight, Option<gpui::SharedString>);

#[derive(Clone)]
struct Paint {
    key: gpui::Rgba,
    agent: gpui::Rgba,
    value: gpui::Rgba,
    markers: [gpui::Rgba; 3],
    key_font: Font,
    agent_font: Font,
    value_font: Font,
    tooltip: Tooltip,
}

impl Render for ActivityPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.current(window, cx);
        let live = !self.model.inert;
        let query = self.model.query;
        let paint = Paint {
            key: self.color("activity-panel.key", Property::Foreground, 0x94a3b8),
            agent: self.color("activity-panel.agent-value", Property::Foreground, 0xcbd5e1),
            value: self.color(
                "activity-panel.activity-value",
                Property::Foreground,
                0xf1f5f9,
            ),
            markers: [
                self.color(
                    "activity-panel.status-marker.running",
                    Property::Background,
                    0x38bdf8,
                ),
                self.color(
                    "activity-panel.status-marker.finished-ok",
                    Property::Background,
                    0x22c55e,
                ),
                self.color(
                    "activity-panel.status-marker.finished-error",
                    Property::Background,
                    0xef4444,
                ),
            ],
            key_font: self.font("activity-panel.key", 11., 500.),
            agent_font: self.font("activity-panel.agent-value", 12., 400.),
            value_font: self.font("activity-panel.activity-value", 12., 400.),
            tooltip: Tooltip {
                text: "".into(),
                background: self.color("tooltip", Property::Background, 0x172033),
                foreground: self.color("tooltip", Property::Foreground, 0xf1f5f9),
                font: self.font("tooltip", 12., 400.),
            },
        };
        let range = self.model.realized_range();
        let rows = self
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
            .map(|(ordinal, row)| self.row_element(ordinal, row, &paint, live, cx))
            .collect::<Vec<_>>();
        self.realized = rows.len();
        if self.tooltip.anchor.is_some_and(|anchor| {
            let tooltip::Anchor::Row(id) = anchor else {
                return false;
            };
            !self.model.pages.iter().any(|page| {
                page.rows.iter().enumerate().any(|(index, row)| {
                    row.id == id && range.contains(&(page.start + index as u64))
                })
            })
        }) {
            self.tooltip.clear();
        }
        if live {
            *self.scroll_state.borrow_mut() = Some(ScrollbarScrollState {
                owner: self.owner,
                viewport_bounds: self.bounds,
                content_size: size(
                    self.bounds.size.width,
                    px(self.model.content_height() as f32),
                ),
                scroll_offset: point(px(0.), px(self.model.offset() as f32)),
                page_distance: self.bounds.size,
            });
        }
        let weak = cx.weak_entity();
        let listeners = cx.weak_entity();
        let measure = canvas(
            move |bounds, _, app| {
                let _ = weak.update(app, |panel, cx| {
                    if panel.model.query != query {
                        return;
                    }
                    if panel.bounds != bounds {
                        panel.bounds = bounds;
                        panel.model.set_viewport(f32::from(bounds.size.height));
                        panel.request(cx);
                        cx.notify();
                    }
                    if let Some(state) = panel.scroll_state.borrow_mut().as_mut() {
                        state.viewport_bounds = bounds;
                        state.page_distance = bounds.size;
                    }
                });
            },
            move |_, _, window, app| {
                if !listeners
                    .upgrade()
                    .is_some_and(|panel| !panel.read(app).model.inert)
                {
                    return;
                }
                let weak = listeners.clone();
                window.on_mouse_event(move |_: &gpui::MouseDownEvent, phase, _, app| {
                    if phase.capture() {
                        let _ = weak.update(app, |panel, cx| {
                            if panel.handle_keyboard {
                                panel.handle_keyboard = false;
                                cx.notify();
                            }
                        });
                    }
                });
                let weak = listeners.clone();
                window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, app| {
                    if phase.capture() {
                        let _ =
                            weak.update(app, |panel, cx| panel.resize_motion(event, window, cx));
                    }
                });
                let weak = listeners.clone();
                window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, window, app| {
                    if phase.capture() && event.button == gpui::MouseButton::Left {
                        let _ = weak.update(app, |panel, cx| panel.finish_resize(window, cx));
                    }
                });
            },
        )
        .absolute()
        .inset_0();
        let mut viewport = div()
            .id("activity-panel-viewport")
            .debug_selector(|| "activity-panel-viewport".into())
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .track_focus(&self.viewport_focus)
            .tab_stop(live)
            .on_key_down(cx.listener(Self::viewport_key))
            .on_scroll_wheel(cx.listener(Self::wheel))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|panel, _, window, cx| {
                    if panel.current(window, cx) {
                        panel.viewport_focus.focus(window);
                    }
                }),
            )
            .on_mouse_move(cx.listener(|panel, _, window, cx| {
                if panel.current(window, cx) {
                    panel
                        .visibility
                        .record_viewport_activity(panel.owner, window, cx);
                }
            }))
            .child(measure)
            .children(rows);
        if live {
            let mut style = ScrollbarStyle::default();
            let color = self.color("scrollbar.thumb", Property::Background, 0x94a3b8);
            style.thumb_color = ((color.r * 255.) as u32) << 16
                | ((color.g * 255.) as u32) << 8
                | (color.b * 255.) as u32;
            if let Some(bar) = gpui_scrollbar::render_scrollbar(
                "activity-panel-scrollbar",
                self.scrollbar.clone(),
                Axis::Vertical,
                style,
                self.visibility.clone(),
                self.interaction.clone(),
            ) {
                viewport = viewport.child(bar);
            }
        }
        let handle_color = self.color(
            "activity-panel.resize-handle",
            Property::Background,
            0x334155,
        );
        let ring = self.color(
            "activity-panel.resize-handle.focus-visible",
            Property::Color,
            0x38bdf8,
        );
        let handle_tooltip = Tooltip {
            text: "Resize activity panel".into(),
            ..paint.tooltip.clone()
        };
        let handle = div()
            .id("activity-panel-resize-handle")
            .relative()
            .debug_selector(|| "activity-panel-resize-handle".into())
            .h(px(model::HANDLE_HEIGHT))
            .flex_none()
            .w_full()
            .bg(handle_color)
            .track_focus(&self.handle_focus)
            .tab_stop(live)
            .when(
                self.handle_keyboard && self.handle_focus.is_focused(window),
                |handle| handle.border_2().border_color(ring),
            )
            .when(live, |handle| {
                handle
                    .cursor(gpui::CursorStyle::ResizeUpDown)
                    .on_hover(cx.listener(|panel, hovered: &bool, window, cx| {
                        panel.tooltip_hover(tooltip::Anchor::Handle, *hovered, window, cx);
                    }))
                    .child(self.tooltip_element(tooltip::Anchor::Handle, handle_tooltip, cx))
            })
            .on_key_down(cx.listener(Self::handle_key))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::begin_resize));
        let feedback = self.model.feedback.clone().map(|feedback| {
            let enabled = live && feedback.retry_available && !feedback.pending;
            let background = self.color("activity-panel.feedback", Property::Background, 0x172033);
            let foreground = self.color("activity-panel.feedback", Property::Foreground, 0xfecaca);
            let feedback_font = self.font("activity-panel.feedback", 12., 400.);
            let button_font = self.font("command-button", 13., 500.);
            let role = if enabled {
                "command-button"
            } else {
                "command-button.disabled"
            };
            let button_background = self.color(
                role,
                Property::Background,
                if enabled { 0xf8fafc } else { 0xf1f5f9 },
            );
            let button_foreground = self.color(
                role,
                Property::Foreground,
                if enabled { 0x1f2937 } else { 0x94a3b8 },
            );
            let button_border = self.color(role, Property::Border, 0xcbd5e1);
            let hover = self.color("command-button.hover", Property::Background, 0xeef2f7);
            let hover_border = self.color("command-button.hover", Property::Border, 0x94a3b8);
            let pressed = self.color("command-button.pressed", Property::Background, 0xe2e8f0);
            let pressed_border = self.color("command-button.pressed", Property::Border, 0x64748b);
            let focus_ring = self.color("command-button.focused", Property::Color, 0x2563eb);
            let button = div()
                .id("activity-panel-retry")
                .relative()
                .debug_selector(|| "activity-panel-retry".into())
                .track_focus(&self.retry_focus)
                .tab_stop(enabled)
                .px(px(12.))
                .h(px(32.))
                .flex_none()
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(button_border)
                .bg(button_background)
                .text_color(button_foreground)
                .text_size(px(button_font.0))
                .font_weight(button_font.1)
                .when_some(button_font.2, |button, family| button.font_family(family))
                .when(
                    self.handle_keyboard && self.retry_focus.is_focused(window),
                    |button| {
                        button.child(
                            div()
                                .absolute()
                                .inset(px(-4.))
                                .border_2()
                                .border_color(focus_ring)
                                .rounded(px(10.)),
                        )
                    },
                )
                .when(enabled, |button| {
                    button
                        .cursor_pointer()
                        .hover(|button| button.bg(hover).border_color(hover_border))
                        .active(|button| button.bg(pressed).border_color(pressed_border))
                })
                .on_click(cx.listener(move |panel, _, window, cx| {
                    if panel.model.query == query
                        && panel
                            .model
                            .feedback
                            .as_ref()
                            .is_some_and(|current| current.identity == feedback.identity)
                    {
                        panel.retry(window, cx);
                    }
                }))
                .on_key_down(
                    cx.listener(|panel, event: &gpui::KeyDownEvent, window, cx| {
                        if panel.retry_focus.is_focused(window)
                            && !event.is_held
                            && matches!(event.keystroke.key.as_str(), "enter" | "space")
                        {
                            panel.retry(window, cx);
                            cx.stop_propagation();
                        }
                    }),
                )
                .child(if feedback.pending {
                    "Retrying…"
                } else {
                    "Retry"
                });
            div()
                .id("activity-panel-feedback")
                .debug_selector(|| "activity-panel-feedback".into())
                .flex()
                .items_center()
                .h(px(model::FEEDBACK_HEIGHT))
                .flex_none()
                .px(px(10.))
                .gap(px(8.))
                .bg(background)
                .text_color(foreground)
                .text_size(px(feedback_font.0))
                .font_weight(feedback_font.1)
                .when_some(feedback_font.2, |feedback, family| {
                    feedback.font_family(family)
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(feedback.explanation.to_string()),
                )
                .child(button)
        });
        div()
            .id("activity-panel")
            .debug_selector(|| "activity-panel".into())
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(self.height))
            .overflow_hidden()
            .border_t_1()
            .border_color(self.color("activity-panel", Property::Border, 0x334155))
            .bg(self.color("activity-panel", Property::Background, 0x0f172a))
            .when(!live, |panel| panel.opacity(0.55))
            .child(handle)
            .children(feedback)
            .child(viewport)
    }
}

impl ActivityPanel {
    fn row_element(
        &self,
        ordinal: u64,
        row: &model::Row,
        paint: &Paint,
        live: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let distance = if ordinal >= self.model.first {
            (ordinal - self.model.first) as f32
        } else {
            -((self.model.first - ordinal) as f32)
        };
        let top = distance * model::ROW_HEIGHT - self.model.fraction;
        let color = paint.markers[match row.status {
            model::Status::Running => 0,
            model::Status::FinishedOk => 1,
            model::Status::FinishedError => 2,
        }];
        let id = row.id;
        let key = self
            .hash
            .hash_one((self.model.query.owner, self.model.query.period, id));
        let element = gpui::SharedString::from(format!("activity-panel-row-{key:016x}"));
        let tooltip = Tooltip {
            text: row.tooltip.clone(),
            ..paint.tooltip.clone()
        };
        let text = |value: String, foreground, font: &Font| {
            div()
                .min_w_0()
                .truncate()
                .text_color(foreground)
                .text_size(px(font.0))
                .font_weight(font.1)
                .when_some(font.2.clone(), |text, family| text.font_family(family))
                .child(value)
        };
        div()
            .id(element)
            .absolute()
            .top(px(top))
            .left_0()
            .w_full()
            .h(px(model::ROW_HEIGHT))
            .flex()
            .items_center()
            .px(px(10.))
            .gap(px(8.))
            .overflow_hidden()
            .child(div().size(px(8.)).rounded_full().flex_none().bg(color))
            .child(text("Agent".into(), paint.key, &paint.key_font).flex_none())
            .child(
                text(row.agent.to_string(), paint.agent, &paint.agent_font)
                    .w(px(220.))
                    .flex_shrink(),
            )
            .child(text("Activity".into(), paint.key, &paint.key_font).flex_none())
            .child(text(row.value.to_string(), paint.value, &paint.value_font).flex_1())
            .when(live, |element| {
                element
                    .on_hover(cx.listener(move |panel, hovered: &bool, window, cx| {
                        panel.tooltip_hover(tooltip::Anchor::Row(id), *hovered, window, cx);
                    }))
                    .child(self.tooltip_element(tooltip::Anchor::Row(id), tooltip, cx))
            })
            .into_any_element()
    }

    fn tooltip_element(
        &self,
        anchor: tooltip::Anchor,
        recipe: Tooltip,
        cx: &Context<Self>,
    ) -> AnyElement {
        let weak = cx.weak_entity();
        let scope = (self.model.query.owner, self.model.query.period);
        canvas(
            move |bounds, window, app| {
                let Some(panel) = weak.upgrade() else {
                    return;
                };
                if !panel.read(app).tooltip_current(anchor, bounds, window) {
                    return;
                }
                let visible = weak.clone();
                window.set_tooltip(gpui::AnyTooltip {
                    view: app.new(|_| recipe).into(),
                    mouse_position: bounds.bottom_right(),
                    check_visible_and_update: Rc::new(move |_, window, app| {
                        visible.upgrade().is_some_and(|panel| {
                            let panel = panel.read(app);
                            (panel.model.query.owner, panel.model.query.period) == scope
                                && panel.tooltip_current(anchor, bounds, window)
                        })
                    }),
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }
}

#[derive(Clone)]
struct Tooltip {
    text: Arc<str>,
    background: gpui::Rgba,
    foreground: gpui::Rgba,
    font: Font,
}
impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(420.))
            .px(px(8.))
            .py(px(6.))
            .rounded(px(5.))
            .bg(self.background)
            .text_color(self.foreground)
            .text_size(px(self.font.0))
            .font_weight(self.font.1)
            .when_some(self.font.2.clone(), |text, family| text.font_family(family))
            .child(self.text.to_string())
    }
}

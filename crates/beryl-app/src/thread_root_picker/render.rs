use super::*;
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, div, prelude::FluentBuilder, rgb,
};
use gpui_scrollbar::render_scrollbar;

impl Render for ThreadRootPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.request_viewport(cx);
        let style = self.config.style;
        let (visible, realized) = self.ranges();
        if visible
            .clone()
            .all(|position| self.collection.row(position).is_some())
            && self.collection.is_current()
        {
            self.last_coherent_scroll = (-f32::from(self.scroll.offset().y)).max(0.);
        }
        let mut rows = Vec::with_capacity(realized.len());
        for position in realized.clone() {
            if let Some(row) = self.collection.row(position) {
                rows.push(self.render_row(position, row.clone(), cx));
            } else {
                rows.push(
                    div()
                        .h(px(style.row_height))
                        .flex_shrink_0()
                        .into_any_element(),
                );
            }
        }
        self.last_realized = rows.len();
        let top = realized.start as f32 * style.row_stride();
        let bottom = self.collection.total.saturating_sub(realized.end) as f32 * style.row_stride();
        let content = div()
            .id("thread-root-picker-collection")
            .debug_selector(|| "thread-root-picker-collection".to_owned())
            .track_focus(&self.collection_focus)
            .tab_stop(true)
            .h(px(style.viewport_height()))
            .w_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.collection.cancel_navigation();
                    this.collection_focus.focus(window);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, _, window, cx| {
                this.visibility
                    .record_viewport_activity(this.scrollbar_owner, window, cx);
            }))
            .on_scroll_wheel(cx.listener(|this, _, window, cx| {
                this.collection.cancel_navigation();
                this.visibility
                    .record_viewport_activity(this.scrollbar_owner, window, cx);
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(style.row_gap))
                    .when(top > 0., |body| {
                        body.child(div().h(px((top - style.row_gap).max(0.))).flex_shrink_0())
                    })
                    .children(rows)
                    .when(bottom > 0., |body| {
                        body.child(
                            div()
                                .h(px((bottom - style.row_gap).max(0.)))
                                .flex_shrink_0(),
                        )
                    }),
            );
        let mut viewport = div()
            .relative()
            .h(px(style.viewport_height()))
            .w_full()
            .child(content);
        if let Some(scrollbar) = render_scrollbar(
            "thread-root-picker-scrollbar",
            self.scrollbar_state.clone(),
            Axis::Vertical,
            self.config.scrollbar_style,
            self.visibility.clone(),
            self.scrollbar_interaction.clone(),
        ) {
            viewport = viewport.child(scrollbar);
        }
        if let Some(message) = &self.collection.failure {
            viewport = viewport.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(style.search_height))
                    .bg(self.color("thread-root-picker", Property::Background, 0x101827))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(message.clone()),
            );
        } else if self.collection.is_current() && self.collection.total == 0 {
            viewport = viewport.child(self.feedback(self.config.empty_text.clone()));
        } else if self.collection.pages.is_empty() {
            viewport = viewport.child(self.feedback("Loading…".to_owned()));
        }
        let title_font = self.font("thread-root-picker.title", 19., 650.);
        let helper_font = self.font("thread-root-picker.helper", 12., 400.);
        let heading_font = self.font("thread-root-picker.collection-heading", 11., 700.);
        let heading_color = self.color(
            "thread-root-picker.collection-heading",
            Property::Foreground,
            0x7f8ea3,
        );
        div()
            .id("thread-root-picker")
            .debug_selector(|| "thread-root-picker".to_owned())
            .w(px(style.width))
            .h(px(style.height))
            .px(px(style.padding_x))
            .py(px(style.padding_y))
            .flex()
            .flex_col()
            .gap(px(style.gap))
            .rounded(px(style.radius))
            .border(px(style.border_width))
            .border_color(self.color("thread-root-picker", Property::Border, 0x3a4860))
            .bg(self.color("thread-root-picker", Property::Background, 0x101827))
            .text_color(self.color("thread-root-picker", Property::Foreground, 0xf3f7fb))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::rgba(0x0000008c).into(),
                offset: point(px(0.), px(14.)),
                blur_radius: px(36.),
                spread_radius: px(0.),
            }])
            .overflow_hidden()
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.dismiss(window, cx)))
            .child(
                div()
                    .h(px(style.header_height))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap(px(style.header_gap))
                    .child(
                        div()
                            .text_size(px(title_font.0))
                            .font_weight(title_font.1)
                            .when_some(title_font.2, |el, font| el.font_family(font))
                            .text_color(self.color(
                                "thread-root-picker.title",
                                Property::Foreground,
                                0xf3f7fb,
                            ))
                            .child(self.config.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(helper_font.0))
                            .font_weight(helper_font.1)
                            .when_some(helper_font.2, |el, font| el.font_family(font))
                            .text_color(self.color(
                                "thread-root-picker.helper",
                                Property::Foreground,
                                0x92a2b7,
                            ))
                            .child(self.config.helper.clone()),
                    ),
            )
            .child(
                div()
                    .id("thread-root-picker-search")
                    .debug_selector(|| "thread-root-picker-search".to_owned())
                    .h(px(style.search_height))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(style.row_content_gap))
                    .child("⌕")
                    .child(div().flex_1().min_w_0().child(self.search.clone()))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.collection.cancel_navigation();
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .h(px(style.heading_height))
                    .flex_shrink_0()
                    .text_size(px(heading_font.0))
                    .font_weight(heading_font.1)
                    .when_some(heading_font.2, |el, font| el.font_family(font))
                    .text_color(heading_color)
                    .child(self.config.heading.clone()),
            )
            .child(viewport)
    }
}

impl ThreadRootPicker {
    fn feedback(&self, message: String) -> AnyElement {
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.))
            .text_color(self.color("thread-root-picker.helper", Property::Foreground, 0x92a2b7))
            .child(message)
            .into_any_element()
    }

    fn render_row(&self, position: usize, row: PickerRow, cx: &mut Context<Self>) -> AnyElement {
        let style = self.config.style;
        let unavailable = row.unavailable_reason.is_some();
        let pending =
            row.activation_pending || self.activation_in_flight.as_ref() == Some(&row.key);
        let focused = self.collection.focused_key() == Some(&row.key);
        let role = if unavailable {
            "thread-root-picker.row.unavailable"
        } else if row.current {
            "thread-root-picker.row.current"
        } else {
            "thread-root-picker.row"
        };
        let primary_font = self.font("thread-root-picker.row-primary", 14., 520.);
        let secondary_font = self.font("thread-root-picker.row-secondary", 12., 400.);
        let status_font = self.font("thread-root-picker.row-status", 10., 650.);
        let key = row.key.clone();
        let focus_key = row.key.clone();
        let revision = self.collection.revision;
        let collection_key = self.collection.key.clone();
        let focus_collection_key = collection_key.clone();
        let id = gpui::SharedString::from(format!("thread-root-picker-row-{}", row.key.0));
        let tooltip = row.unavailable_reason.clone().or(row.tooltip.clone());
        let mut element = div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .h(px(style.row_height))
            .flex_shrink_0()
            .w_full()
            .px(px(style.row_padding_x))
            .flex()
            .items_center()
            .gap(px(style.row_content_gap))
            .rounded(px(style.row_radius))
            .border(px(if focused {
                style.ring_width
            } else {
                style.row_border_width
            }))
            .border_color(if focused {
                self.color("thread-root-picker.row.focused", Property::Border, 0x38bdf8)
            } else {
                self.color(
                    role,
                    Property::Border,
                    if unavailable {
                        0x2b3547
                    } else if row.current {
                        0x3b475d
                    } else {
                        0x2f3b52
                    },
                )
            })
            .bg(self.color(
                role,
                Property::Background,
                if row.current { 0x131b29 } else { 0x111827 },
            ))
            .when(unavailable || !self.collection.is_current(), |el| {
                el.opacity(style.unavailable_opacity)
            })
            .hover(|el| {
                el.bg(self.color(
                    "thread-root-picker.row.hover",
                    Property::Background,
                    0x141d2d,
                ))
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if this.collection.revision == revision
                        && this.collection.key == focus_collection_key
                        && this
                            .collection
                            .row(position)
                            .is_some_and(|row| row.key == focus_key)
                    {
                        this.focus_row(position, window, cx);
                    }
                }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.collection.revision == revision && this.collection.key == collection_key {
                    this.activate(&key, cx);
                }
            }))
            .child(
                div()
                    .w(px(style.row_icon_size))
                    .h(px(style.row_icon_size))
                    .flex_shrink_0()
                    .relative()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .w_full()
                            .h(px(style.row_icon_size * 0.2))
                            .bg(self.color(
                                "thread-root-picker.row-icon",
                                Property::Foreground,
                                0x7dd3fc,
                            )),
                    )
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .w_full()
                            .h(px(style.row_icon_size * 0.2))
                            .bg(self.color(
                                "thread-root-picker.row-icon",
                                Property::Foreground,
                                0x7dd3fc,
                            )),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(style.row_icon_size * 0.2))
                            .left(px(style.row_icon_size * 0.25))
                            .w(px(style.row_icon_size * 0.5))
                            .h(px(style.row_icon_size * 0.6))
                            .border_l(px(style.row_border_width))
                            .border_r(px(style.row_border_width))
                            .border_color(self.color(
                                "thread-root-picker.row-icon",
                                Property::Foreground,
                                0x7dd3fc,
                            )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(primary_font.0))
                            .font_weight(primary_font.1)
                            .when_some(primary_font.2, |el, font| el.font_family(font))
                            .text_color(self.color(
                                "thread-root-picker.row-primary",
                                Property::Foreground,
                                if unavailable { 0x7f8ea3 } else { 0xe9eef5 },
                            ))
                            .child(row.primary),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(secondary_font.0))
                            .font_weight(secondary_font.1)
                            .when_some(secondary_font.2, |el, font| el.font_family(font))
                            .text_color(self.color(
                                "thread-root-picker.row-secondary",
                                Property::Foreground,
                                0x92a2b7,
                            ))
                            .child(row.secondary),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(status_font.0))
                    .font_weight(status_font.1)
                    .when_some(status_font.2, |el, font| el.font_family(font))
                    .text_color(self.color(
                        "thread-root-picker.row-status",
                        Property::Foreground,
                        0x7dd3fc,
                    ))
                    .child(if pending {
                        "OPENING…".to_owned()
                    } else {
                        row.status
                    }),
            );
        if let Some(tooltip) = tooltip {
            element =
                element.tooltip(move |_, cx| cx.new(|_| PickerTooltip(tooltip.clone())).into());
        }
        element.into_any_element()
    }
}

struct PickerTooltip(String);
impl Render for PickerTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(0x1e293b))
            .text_color(rgb(0xf3f7fb))
            .text_size(px(12.))
            .child(self.0.clone())
    }
}

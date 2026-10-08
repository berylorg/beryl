use super::*;
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, div, prelude::FluentBuilder, rgb,
};
use gpui_scrollbar::render_scrollbar;

impl Render for ThreadRootPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.request_viewport(cx);
        self.request_runtime_viewport(cx);
        self.full.rendered_commands.clear();
        let style = self.config.style;
        let collection_height = self.collection_height();
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
                        .h(px(self.collection_row_height()))
                        .flex_shrink_0()
                        .into_any_element(),
                );
            }
        }
        self.last_realized = rows.len();
        let top = realized.start as f32 * self.collection_stride();
        let bottom =
            self.collection.total.saturating_sub(realized.end) as f32 * self.collection_stride();
        let content = div()
            .id("thread-root-picker-collection")
            .debug_selector(|| "thread-root-picker-collection".to_owned())
            .track_focus(&self.collection_focus)
            .tab_stop(true)
            .h(px(collection_height))
            .flex_shrink_0()
            .w_full()
            .overflow_y_scroll()
            .when(
                self.full.in_flight.is_some() || self.full.native_dialog_open,
                |body| body.overflow_hidden(),
            )
            .track_scroll(&self.scroll)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.full.in_flight.is_some() || this.full.native_dialog_open {
                        cx.stop_propagation();
                        return;
                    }
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
            .h(px(collection_height))
            .flex_shrink_0()
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
        if let Some(message) = self.collection.failure.clone() {
            viewport =
                viewport.child(self.render_failure(message, PickerCommand::RetryCollection, cx));
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
        let return_command = self
            .full
            .return_command
            .clone()
            .map(|state| self.render_command(PickerCommand::Return, state, cx));
        let runtime = self.render_runtime_section(cx);
        let footer = self.render_footer(cx);
        self.full.command_focus.retain(|(command, focus)| {
            self.full.rendered_commands.contains(command)
                || self.full.in_flight.as_ref() == Some(command)
                || focus.is_focused(window)
        });
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
                            .flex()
                            .items_center()
                            .justify_between()
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
                            .when_some(return_command, |header, command| header.child(command)),
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
            .when_some(runtime, |body, runtime| body.child(runtime))
            .when_some(footer, |body, footer| body.child(footer))
    }
}

impl ThreadRootPicker {
    pub(super) fn feedback(&self, message: String) -> AnyElement {
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
}

pub(super) struct PickerTooltip(pub(super) String);
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

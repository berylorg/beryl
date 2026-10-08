use super::render::PickerTooltip;
use super::*;
use beryl_state::ThemePropertyId as Property;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, prelude::FluentBuilder,
};
use gpui_scrollbar::render_scrollbar;

impl ThreadRootPicker {
    pub(super) fn render_footer(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let PickerSelectionMode::Confirmed { confirm } = &self.full.selection else {
            return None;
        };
        let mut state = confirm.clone();
        let command = PickerCommand::Confirm(
            self.full
                .selected
                .clone()
                .unwrap_or_else(|| PickerRowKey(String::new())),
        );
        if self.full.selected.is_none() {
            state.unavailable_reason = Some("Choose a root before confirming.".into());
        } else if let Some(current) = self.command_state(&command) {
            state = current;
        }
        let button = self.render_command(command, state, cx);
        Some(
            div()
                .id("thread-root-picker-footer")
                .h(px(self.config.style.footer_height))
                .flex_shrink_0()
                .pt(px(12.))
                .border_t(px(1.))
                .border_color(self.color("thread-root-picker.footer", Property::Border, 0x2a3549))
                .flex()
                .items_center()
                .justify_end()
                .child(button)
                .into_any_element(),
        )
    }

    pub(super) fn render_runtime_section(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let style = self.config.style;
        let (visible, realized) = self.runtime_ranges();
        let runtime = self.full.runtime.as_mut()?;
        if runtime.rows.collection.is_current()
            && visible
                .clone()
                .all(|position| runtime.rows.row(position).is_some())
        {
            runtime.last_coherent_scroll = (-f32::from(runtime.scroll.offset().y)).max(0.);
        }
        runtime.last_realized = realized.len();
        let heading = runtime.config.heading.clone();
        let empty = runtime.config.empty_text.clone();
        let add = runtime.config.add_runtime.clone();
        let focus = runtime.focus.clone();
        let scroll = runtime.scroll.clone();
        let failure = runtime.rows.collection.failure.clone();
        let current = runtime.rows.collection.is_current();
        let total = runtime.rows.total_count();
        let loading = runtime.rows.collection.pages.is_empty();
        let scrollbar = render_scrollbar(
            "thread-root-picker-runtime-scrollbar",
            runtime.state.clone(),
            Axis::Vertical,
            self.config.scrollbar_style,
            runtime.visibility.clone(),
            runtime.interaction.clone(),
        );
        let projected: Vec<_> = realized
            .clone()
            .map(|position| (position, runtime.rows.row(position).cloned()))
            .collect();
        let rows: Vec<_> = projected
            .into_iter()
            .map(|(position, row)| match row {
                Some(row) => self.render_runtime_row(position, row, cx),
                None => div()
                    .h(px(style.runtime_row_height))
                    .flex_shrink_0()
                    .into_any_element(),
            })
            .collect();
        let top = realized.start as f32 * style.runtime_row_stride();
        let bottom = total.saturating_sub(realized.end) as f32 * style.runtime_row_stride();
        let content = div()
            .id("thread-root-picker-runtime-collection")
            .debug_selector(|| "thread-root-picker-runtime-collection".into())
            .track_focus(&focus)
            .tab_stop(true)
            .h(px(style.runtime_viewport_height))
            .w_full()
            .overflow_y_scroll()
            .when(
                self.full.in_flight.is_some() || self.full.native_dialog_open,
                |body| body.overflow_hidden(),
            )
            .track_scroll(&scroll)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if this.full.in_flight.is_some() || this.full.native_dialog_open {
                        cx.stop_propagation();
                        return;
                    }
                    this.collection.cancel_navigation();
                    if let Some(runtime) = &mut this.full.runtime {
                        runtime.rows.collection.cancel_navigation();
                        runtime.focus.focus(window);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, _, window, cx| {
                if let Some(runtime) = &mut this.full.runtime {
                    runtime
                        .visibility
                        .record_viewport_activity(runtime.owner, window, cx);
                }
            }))
            .on_scroll_wheel(cx.listener(|this, _, window, cx| {
                if let Some(runtime) = &mut this.full.runtime {
                    runtime.rows.collection.cancel_navigation();
                    runtime
                        .visibility
                        .record_viewport_activity(runtime.owner, window, cx);
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(style.runtime_row_gap))
                    .when(top > 0., |body| {
                        body.child(
                            div()
                                .h(px((top - style.runtime_row_gap).max(0.)))
                                .flex_shrink_0(),
                        )
                    })
                    .children(rows)
                    .when(bottom > 0., |body| {
                        body.child(
                            div()
                                .h(px((bottom - style.runtime_row_gap).max(0.)))
                                .flex_shrink_0(),
                        )
                    }),
            );
        let mut viewport = div()
            .relative()
            .h(px(style.runtime_viewport_height))
            .flex_shrink_0()
            .w_full()
            .child(content);
        if let Some(scrollbar) = scrollbar {
            viewport = viewport.child(scrollbar);
        }
        if let Some(failure) = failure {
            viewport =
                viewport.child(self.render_failure(failure, PickerCommand::RetryRuntime, cx));
        } else if current && total == 0 {
            viewport = viewport.child(self.feedback(empty));
        } else if loading {
            viewport = viewport.child(self.feedback("Loading…".into()));
        }
        let heading_font = self.font("thread-root-picker.runtime-heading", 11., 700.);
        let add = self.render_command(PickerCommand::AddRuntime, add, cx);
        Some(
            div()
                .id("thread-root-picker-runtime-section")
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(px(style.gap))
                .child(
                    div()
                        .h(px(style.heading_height))
                        .text_size(px(heading_font.0))
                        .font_weight(heading_font.1)
                        .when_some(heading_font.2, |el, font| el.font_family(font))
                        .text_color(self.color(
                            "thread-root-picker.runtime-heading",
                            Property::Foreground,
                            0x7f8ea3,
                        ))
                        .child(heading),
                )
                .child(viewport)
                .child(div().flex().child(add))
                .into_any_element(),
        )
    }

    pub(super) fn render_failure(
        &mut self,
        message: String,
        command: PickerCommand,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let retry = self
            .command_state(&command)
            .map(|state| self.render_command(command, state, cx));
        div()
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .h(px(self.config.style.search_height))
            .bg(self.color("thread-root-picker", Property::Background, 0x101827))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(self.config.style.gap))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(message),
            )
            .when_some(retry, |feedback, retry| feedback.child(retry))
            .into_any_element()
    }

    fn render_runtime_row(
        &mut self,
        position: usize,
        runtime_row: PickerRuntimeRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let style = self.config.style;
        let runtime = self
            .full
            .runtime
            .as_ref()
            .expect("mounted runtime viewport");
        let focused = runtime.rows.focused_key() == Some(&runtime_row.row.key);
        let revision = runtime.rows.collection.revision;
        let collection_key = runtime.rows.collection.key.clone();
        let current = runtime.rows.collection.is_current();
        let row = runtime_row.row;
        let exact_key = row.key.clone();
        let mut browse = runtime_row.browse_roots;
        let mut add = runtime_row.add_root;
        if !current {
            browse.unavailable_reason = Some("The runtime collection is loading.".into());
            add.unavailable_reason = Some("The runtime collection is loading.".into());
        }
        let browse = self.render_command(PickerCommand::BrowseRoots(row.key.clone()), browse, cx);
        let add = self.render_command(PickerCommand::AddRoot(row.key.clone()), add, cx);
        let role = if runtime_row.active_scope {
            "thread-root-picker.runtime-row.active-scope"
        } else {
            "thread-root-picker.runtime-row"
        };
        let label_font = self.font("thread-root-picker.runtime-label", 13., 600.);
        let metadata_font = self.font("thread-root-picker.runtime-metadata", 11., 400.);
        let tooltip = row.tooltip.or(row.unavailable_reason);
        let id = gpui::SharedString::from(format!("thread-root-picker-runtime-row-{}", row.key.0));
        let mut element = div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .h(px(style.runtime_row_height))
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
            .border_color(self.color(
                role,
                Property::Border,
                if focused || runtime_row.active_scope {
                    0x38bdf8
                } else {
                    0x2f3b52
                },
            ))
            .bg(self.color(
                role,
                Property::Background,
                if runtime_row.active_scope {
                    0x173a5e
                } else {
                    0x111827
                },
            ))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if this.full.in_flight.is_some() || this.full.native_dialog_open {
                        cx.stop_propagation();
                        return;
                    }
                    this.collection.cancel_navigation();
                    if let Some(runtime) = &mut this.full.runtime {
                        if runtime.rows.collection.key == collection_key
                            && runtime.rows.collection.revision == revision
                            && runtime
                                .rows
                                .row(position)
                                .is_some_and(|row| row.row.key == exact_key)
                        {
                            runtime.rows.collection.focus_position(position);
                            runtime.focus.focus(window);
                        }
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(px(label_font.0))
                            .font_weight(label_font.1)
                            .when_some(label_font.2, |el, font| el.font_family(font))
                            .text_color(self.color(
                                "thread-root-picker.runtime-label",
                                Property::Foreground,
                                0xe9eef5,
                            ))
                            .child(row.primary),
                    )
                    .child(
                        div()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(px(metadata_font.0))
                            .font_weight(metadata_font.1)
                            .when_some(metadata_font.2, |el, font| el.font_family(font))
                            .text_color(self.color(
                                "thread-root-picker.runtime-metadata",
                                Property::Foreground,
                                0x92a2b7,
                            ))
                            .child(row.secondary),
                    ),
            )
            .child(browse)
            .child(add);
        if let Some(tooltip) = tooltip {
            element =
                element.tooltip(move |_, cx| cx.new(|_| PickerTooltip(tooltip.clone())).into());
        }
        element.into_any_element()
    }
}

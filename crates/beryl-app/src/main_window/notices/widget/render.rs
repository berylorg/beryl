use super::*;

impl MainWindowNoticeWidget {
    pub(super) fn render_command(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let record = self.record.as_ref()?;
        let command = record.content.commands().nth(index)?;
        let focus = self.command_focuses.get(index)?.1.clone();
        let id = SharedString::from(format!(
            "main-window-notice-command-{}-{}-{index}",
            self.widget_instance_id, self.interaction_generation
        ));
        let token = record.token.clone();
        let generation = self.interaction_generation;
        let color =
            |role, property, fallback| theme_color(&self.appearance, role, property, fallback);
        let enabled = !self.inert && command.state() == NoticeCommandState::Enabled;
        let role = if enabled {
            "button.secondary.normal"
        } else {
            "button.secondary.disabled"
        };
        let font = theme_font(&self.appearance, "button.secondary.label", 13., 500.);
        let label = if command.state() == NoticeCommandState::Loading {
            format!("{}…", command.label().as_str())
        } else {
            command.label().as_str().to_owned()
        };
        let mut button = div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .when(!self.inert, |button| {
                button.track_focus(&focus).tab_stop(enabled)
            })
            .px_3()
            .h(px(32.))
            .min_w_0()
            .max_w_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .rounded_md()
            .border_1()
            .border_color(color(role, Property::Border, 0x475569))
            .bg(color(role, Property::Background, 0x1e293b))
            .text_size(px(font.0))
            .font_weight(gpui::FontWeight(font.1))
            .text_color(color(role, Property::Foreground, 0x94a3b8))
            .when_some(font.2, |button, family| button.font_family(family))
            .when(command.state() == NoticeCommandState::Loading, |button| {
                button.opacity(0.72)
            })
            .child(label);
        if enabled {
            let command_id = command.id;
            let down_token = token.clone();
            let up_token = token.clone();
            let down_focus = focus.clone();
            let hover = color("button.secondary.hover", Property::Background, 0x334155);
            let hover_border = color("button.secondary.hover", Property::Border, 0x64748b);
            let hover_foreground = color("button.secondary.hover", Property::Foreground, 0xf8fafc);
            let pressed = color("button.secondary.pressed", Property::Background, 0x0f172a);
            let pressed_border = color("button.secondary.pressed", Property::Border, 0x475569);
            button = button
                .cursor_pointer()
                .hover(move |style| {
                    style
                        .bg(hover)
                        .border_color(hover_border)
                        .text_color(hover_foreground)
                })
                .active(move |style| style.bg(pressed).border_color(pressed_border))
                .focus(|style| style.border_color(rgb(0xf59e0b)))
                .on_key_down(cx.listener(move |this, event, window, cx| {
                    if this.interaction_generation != generation {
                        return;
                    }
                    this.command_key_down(
                        down_token.clone(),
                        Some(command_id),
                        &down_focus,
                        event,
                        window,
                        cx,
                    );
                }))
                .on_key_up(cx.listener(move |this, event, _, cx| {
                    if this.interaction_generation != generation {
                        return;
                    }
                    this.command_key_up(up_token.clone(), Some(command_id), event, cx);
                }))
                .on_click(cx.listener(move |this, event, _, cx| {
                    if this.interaction_generation != generation {
                        return;
                    }
                    if !matches!(event, gpui::ClickEvent::Mouse(_)) {
                        return;
                    }
                    cx.stop_propagation();
                    this.command(token.clone(), command_id, cx);
                }));
        } else if !self.inert
            && let Some(reason) = command.disabled_reason()
        {
            let explanation = SharedString::from(reason.as_str().to_owned());
            button = button.tooltip(move |_, cx| {
                cx.new(|_| NoticeDisabledTooltip {
                    explanation: explanation.clone(),
                })
                .into()
            });
        }
        Some(button.into_any_element())
    }
}

impl Render for MainWindowNoticeWidget {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.detail_state.take_scroll_reset() {
            self.detail_scroll.set_offset(point(px(0.), px(0.)));
        }
        let Some(record) = self.record.as_ref() else {
            return div().into_any_element();
        };
        let content = record.content.clone();
        let interaction_token = record.token.clone();
        let interaction_generation = self.interaction_generation;
        let allocation = record.allocation;
        let width = NOTICE_WIDTH.min(allocation.available_inline.max(0.));
        let height = NOTICE_MAX_HEIGHT.min(allocation.available_block.max(0.));
        let variant = variant_role(content.variant);
        let color = |part: &str, property, fallback| {
            let role = if part.is_empty() {
                format!("main-window-notice{variant}")
            } else {
                format!("main-window-notice.{part}{variant}")
            };
            theme_color(&self.appearance, &role, property, fallback)
        };
        let title_font = theme_font(
            &self.appearance,
            &format!("main-window-notice.title{variant}"),
            13.,
            650.,
        );
        let close_font = theme_font(
            &self.appearance,
            &format!("main-window-notice.close{variant}"),
            13.,
            400.,
        );
        let frame_font = theme_font(
            &self.appearance,
            &format!("main-window-notice{variant}"),
            13.,
            400.,
        );
        let detail_font = theme_font(
            &self.appearance,
            &format!("main-window-notice.detail-viewport{variant}"),
            13.,
            400.,
        );
        let detail = content.detail().as_str().to_owned();
        let detail_region = (!detail.is_empty()).then(|| {
            let mut detail_viewport = div()
                .id("main-window-notice-detail")
                .debug_selector(|| "main-window-notice-detail".to_owned())
                .relative()
                .min_h(px(0.))
                .max_h(px(NOTICE_DETAIL_MAX_HEIGHT.min(height)))
                .flex_1()
                .overflow_hidden()
                .track_scroll(&self.detail_scroll)
                .when(!self.inert, |detail| {
                    detail.track_focus(&self.detail_focus).tab_stop(true)
                })
                .px_3()
                .pb(px(10.))
                .text_size(px(detail_font.0))
                .font_weight(gpui::FontWeight(detail_font.1))
                .text_color(color("detail-viewport", Property::Foreground, 0xcbd5e1))
                .text_bg(theme_color_with_fallback(
                    &self.appearance,
                    &format!("main-window-notice.detail-viewport{variant}"),
                    Property::TextBackground,
                    gpui::rgba(0),
                ))
                .when_some(detail_font.2.clone(), |element, family| {
                    element.font_family(family)
                })
                .child(NoticeDetailElement::new(
                    "main-window-notice-detail-text",
                    SharedString::from(detail),
                    self.detail_state.clone(),
                    !self.inert,
                    self.detail_focus.clone(),
                ));
            if !self.inert {
                detail_viewport = detail_viewport
                    .cursor_text()
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        if this.interaction_generation != interaction_generation {
                            return;
                        }
                        let key = event.keystroke.key.as_str();
                        if event.keystroke.modifiers.secondary() && key == "a" {
                            cx.stop_propagation();
                            this.select_all(window, cx);
                        } else if event.keystroke.modifiers.secondary() && key == "c" {
                            cx.stop_propagation();
                            this.copy_selection(cx);
                        } else if matches!(key, "left" | "right" | "up" | "down" | "home" | "end") {
                            cx.stop_propagation();
                            this.extend_detail_selection(
                                key,
                                event.keystroke.modifiers.shift,
                                window,
                                cx,
                            );
                        }
                    }))
                    .on_mouse_move({
                        let visibility = self.scrollbar_visibility.clone();
                        let owner = self.scrollbar_owner;
                        cx.listener(move |this, _: &gpui::MouseMoveEvent, window, cx| {
                            if this.inert || this.scrollbar_owner != owner {
                                return;
                            }
                            visibility.record_viewport_activity(owner, window, cx);
                        })
                    })
                    .on_scroll_wheel({
                        let visibility = self.scrollbar_visibility.clone();
                        let owner = self.scrollbar_owner;
                        move |_, window, cx| visibility.record_viewport_activity(owner, window, cx)
                    });
                detail_viewport = detail_viewport.overflow_y_scroll();
            }
            let mut detail_region = div()
                .id("main-window-notice-detail-region")
                .relative()
                .flex()
                .flex_col()
                .min_h(px(0.))
                .flex_1()
                .child(detail_viewport);
            if !self.inert {
                if let Some(scrollbar) = render_scroll_handle_scrollbar(
                    "main-window-notice-detail-scrollbar",
                    self.scrollbar_owner,
                    self.scrollbar_state.clone(),
                    &self.detail_scroll,
                    ScrollbarAxis::Vertical,
                    ScrollbarStyle::default(),
                    self.scrollbar_visibility.clone(),
                ) {
                    detail_region = detail_region.child(scrollbar);
                }
            }
            detail_region.into_any_element()
        });
        let root = div()
            .id("main-window-notice")
            .debug_selector(|| "main-window-notice".to_owned())
            .absolute()
            .top(px(
                allocation.origin_block_start + allocation.inset_block_start
            ))
            .right(px(allocation.inset_inline_end))
            .w(px(width))
            .max_h(px(height))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(8.))
            .border_1()
            .border_color(color("", Property::Border, 0x334155))
            .bg(color("", Property::Background, 0x111827))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::rgba(0x0000006b).into(),
                offset: point(px(0.), px(12.)),
                blur_radius: px(28.),
                spread_radius: px(0.),
            }])
            .text_color(color("", Property::Foreground, 0xe5e7eb))
            .text_size(px(frame_font.0))
            .font_weight(gpui::FontWeight(frame_font.1))
            .when_some(frame_font.2, |root, family| root.font_family(family))
            .when(self.inert, |root| root.opacity(0.62))
            .when(
                self.paint_phase != MainWindowNoticePaintPhase::Visible,
                |root| root.opacity(0.),
            )
            .child(
                div()
                    .id("main-window-notice-header")
                    .debug_selector(|| "main-window-notice-header".to_owned())
                    .h(px(40.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .px_3()
                    .gap_2()
                    .child(
                        div()
                            .id("main-window-notice-variant-marker")
                            .debug_selector(|| "main-window-notice-variant-marker".to_owned())
                            .size(px(8.))
                            .rounded_full()
                            .bg(color(
                                "variant-marker",
                                Property::Background,
                                variant_fallback(content.variant),
                            )),
                    )
                    .child(
                        div()
                            .id("main-window-notice-title")
                            .debug_selector(|| "main-window-notice-title".to_owned())
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(px(title_font.0))
                            .font_weight(gpui::FontWeight(title_font.1))
                            .text_color(color("title", Property::Foreground, 0xf8fafc))
                            .text_bg(theme_color_with_fallback(
                                &self.appearance,
                                &format!("main-window-notice.title{variant}"),
                                Property::TextBackground,
                                gpui::rgba(0),
                            ))
                            .when_some(title_font.2.clone(), |element, family| {
                                element.font_family(family)
                            })
                            .child(content.title().as_str().to_owned()),
                    )
                    .when(
                        content.dismissal == NoticeDismissal::Dismissible,
                        |header| {
                            let close_id = SharedString::from(format!(
                                "main-window-notice-close-{}-{}",
                                self.widget_instance_id, self.interaction_generation
                            ));
                            let token = interaction_token.clone();
                            let mut close = div()
                                .id(close_id)
                                .debug_selector(|| "main-window-notice-close".to_owned())
                                .when(!self.inert, |close| {
                                    close.track_focus(&self.close_focus).tab_stop(true)
                                })
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .border_1()
                                .bg(theme_color_with_fallback(
                                    &self.appearance,
                                    &format!("main-window-notice.close{variant}"),
                                    Property::Background,
                                    gpui::rgba(0),
                                ))
                                .border_color(theme_color_with_fallback(
                                    &self.appearance,
                                    &format!("main-window-notice.close{variant}"),
                                    Property::Border,
                                    gpui::rgba(0),
                                ))
                                .text_color(color("close", Property::Foreground, 0xcbd5e1))
                                .text_size(px(close_font.0))
                                .font_weight(gpui::FontWeight(close_font.1))
                                .when_some(close_font.2.clone(), |close, family| {
                                    close.font_family(family)
                                })
                                .child("×");
                            if !self.inert {
                                let down_token = token.clone();
                                let up_token = token.clone();
                                let down_focus = self.close_focus.clone();
                                close = close
                                    .tooltip(|_, cx| cx.new(|_| NoticeCloseTooltip).into())
                                    .cursor_pointer()
                                    .on_key_down(cx.listener(move |this, event, window, cx| {
                                        if this.interaction_generation != interaction_generation {
                                            return;
                                        }
                                        this.command_key_down(
                                            down_token.clone(),
                                            None,
                                            &down_focus,
                                            event,
                                            window,
                                            cx,
                                        );
                                    }))
                                    .on_key_up(cx.listener(move |this, event, _, cx| {
                                        if this.interaction_generation != interaction_generation {
                                            return;
                                        }
                                        this.command_key_up(up_token.clone(), None, event, cx);
                                    }))
                                    .on_click(cx.listener(move |this, event, _, cx| {
                                        if this.interaction_generation != interaction_generation {
                                            return;
                                        }
                                        if !matches!(event, gpui::ClickEvent::Mouse(_)) {
                                            return;
                                        }
                                        cx.stop_propagation();
                                        this.dismiss(token.clone(), cx);
                                    }));
                            }
                            header.child(close)
                        },
                    ),
            )
            .when_some(detail_region, |root, detail_region| {
                root.child(detail_region)
            });
        let mut commands = div()
            .id("main-window-notice-commands")
            .debug_selector(|| "main-window-notice-commands".to_owned())
            .flex_none()
            .flex()
            .flex_wrap()
            .justify_end()
            .px_3()
            .pb(px(10.))
            .gap_2();
        for index in 0..content.commands().count() {
            if let Some(command) = self.render_command(index, cx) {
                commands = commands.child(command);
            }
        }
        let root = root.when(content.commands().count() > 0, |root| root.child(commands));
        frame::MeasuredFrame {
            child: root.into_any_element(),
            measured: self.frame_size.clone(),
        }
        .into_any_element()
    }
}

struct NoticeDisabledTooltip {
    explanation: SharedString,
}

struct NoticeCloseTooltip;

impl Render for NoticeCloseTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("main-window-notice-close-tooltip")
            .debug_selector(|| "main-window-notice-close-tooltip".to_owned())
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(rgb(0x5f6875))
            .bg(rgb(0x20242b))
            .text_sm()
            .text_color(rgb(0xf2f4f7))
            .child("Dismiss notice")
    }
}

impl Render for NoticeDisabledTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("main-window-notice-command-disabled-tooltip")
            .debug_selector(|| "main-window-notice-command-disabled-tooltip".to_owned())
            .max_w(px(320.))
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(rgb(0x5f6875))
            .bg(rgb(0x20242b))
            .text_sm()
            .text_color(rgb(0xf2f4f7))
            .child(self.explanation.clone())
    }
}

fn variant_role(variant: NoticeVariant) -> &'static str {
    match variant {
        NoticeVariant::Warning => ".warning",
        NoticeVariant::Error => ".error",
        NoticeVariant::Info => ".info",
    }
}

fn variant_fallback(variant: NoticeVariant) -> u32 {
    match variant {
        NoticeVariant::Warning => 0xf59e0b,
        NoticeVariant::Error => 0xef4444,
        NoticeVariant::Info => 0x38bdf8,
    }
}

fn theme_color(
    generation: &crate::theme_runtime::AppearanceGeneration,
    role: &str,
    property: Property,
    fallback: u32,
) -> gpui::Rgba {
    theme_color_with_fallback(generation, role, property, rgb(fallback))
}

fn theme_color_with_fallback(
    generation: &crate::theme_runtime::AppearanceGeneration,
    role: &str,
    property: Property,
    fallback: gpui::Rgba,
) -> gpui::Rgba {
    let value = generation
        .prepared()
        .appearance()
        .roles()
        .iter()
        .find(|(id, _)| id.as_str() == role)
        .and_then(|(_, style)| style.property(property));
    if let Some(ThemeValue::Color(value)) = value {
        let [r, g, b] = value.rgb();
        rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
    } else {
        fallback
    }
}

fn theme_font(
    generation: &crate::theme_runtime::AppearanceGeneration,
    role: &str,
    size: f32,
    weight: f32,
) -> (f32, f32, Option<SharedString>) {
    let style = generation
        .prepared()
        .appearance()
        .roles()
        .iter()
        .find(|(id, _)| id.as_str() == role)
        .map(|(_, style)| style);
    let property = |property| style.and_then(|style| style.property(property));
    let family = match property(Property::FontFamily) {
        Some(ThemeValue::FontFamily(family)) => {
            Some(SharedString::from(family.as_str().to_owned()))
        }
        _ => None,
    };
    let size = match property(Property::FontSize) {
        Some(ThemeValue::LogicalPixels(value)) => value.get(),
        _ => size,
    };
    let weight = match property(Property::FontWeight) {
        Some(ThemeValue::FontWeight(value)) => f32::from(value.get()),
        _ => weight,
    };
    (size, weight, family)
}

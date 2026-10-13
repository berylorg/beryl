use super::*;
pub(in super::super) fn render(
    panel: &MountedCodePanel,
    host: &SyndicTranscriptHost,
    wheel: &Rc<Cell<Option<u64>>>,
    appearance: &super::super::code_appearance::CodeAppearance,
    commands_pending: bool,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> AnyElement {
    let pages = host.indexed_resource_pages(&panel.viewport.owner.resource_id);
    let response = pages.last();
    let mut label = response
        .as_ref()
        .and_then(|response| response.root.as_ref())
        .map(|root| {
            format!(
                "{} · {} lines",
                root.language.as_deref().unwrap_or("Plain text"),
                root.code_lines
            )
        })
        .unwrap_or_else(|| "Code loading".to_owned());
    match panel.layout.state(&panel.viewport) {
        "pending" => label.push_str(" · Layout loading"),
        "refused" => label.push_str(" · Layout unavailable"),
        _ => {}
    }
    let selected_source = (panel.actions.binding.as_ref() == panel.layout.source_binding()
        && panel.actions.root == panel.layout.source_root())
    .then(|| panel.actions.selected_range())
    .flatten();
    let mut text = div()
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .h(px(VIEWPORT_HEIGHT))
        .overflow_hidden();
    let scroll = panel.viewport.scroll.borrow();
    let prepared: Vec<_> = panel.layout.prepared().cloned().collect();
    let top_line = scroll.top_line;
    let line_height = scroll.line_height;
    let remainder = scroll.remainder;
    let color = appearance.body_text;
    #[cfg(test)]
    let painted_body_glyphs = panel.painted_body_glyphs.clone();
    #[cfg(test)]
    let painted_selection_quads = panel.painted_selection_quads.clone();
    text = text.child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                #[cfg(test)]
                painted_selection_quads.set(0);
                #[cfg(test)]
                let before = {
                    let paint = window.paint_snapshot_for_test();
                    paint.glyphs.len() + paint.color_glyphs.len()
                };
                window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                    for prepared in &prepared {
                        for fragment in &prepared.fragments {
                            let top = (fragment.logical_line as i128 - top_line as i128) as f32
                                * line_height
                                - remainder;
                            let origin = bounds.origin
                                + point(px(10.0) - prepared.target.inline_start, px(top));
                            for fragment in fragment.fragments.iter() {
                                if let Some(selected) = &selected_source {
                                    if let Some(bounds) = selection::highlight(
                                        fragment,
                                        selected,
                                        origin,
                                        px(line_height),
                                    ) {
                                        window
                                            .paint_quad(gpui::fill(bounds, gpui::rgba(0x38bdf840)));
                                        #[cfg(test)]
                                        painted_selection_quads
                                            .set(painted_selection_quads.get() + 1);
                                    }
                                }
                                let result = match fragment {
                                    gpui::StreamingLayoutFragment::Text(fragment) => {
                                        fragment.paint_with_color(origin, color.into(), window, cx)
                                    }
                                    gpui::StreamingLayoutFragment::OversizeAtom(fragment) => {
                                        fragment.paint_with_color(origin, color.into(), window, cx)
                                    }
                                    gpui::StreamingLayoutFragment::InlineObject(fragment) => {
                                        fragment.paint_with_color(origin, color.into(), window, cx)
                                    }
                                    gpui::StreamingLayoutFragment::Boundary(_) => Ok(()),
                                };
                                if result.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
                #[cfg(test)]
                {
                    let paint = window.paint_snapshot_for_test();
                    painted_body_glyphs.set(paint.glyphs.len() + paint.color_glyphs.len() - before);
                }
            },
        )
        .absolute()
        .size_full(),
    );
    drop(scroll);
    let id = panel.id;
    let bounds = panel.viewport.scroll.clone();
    let visibility = panel.visibility.clone();
    let owner = panel.owner;
    let scroll = panel.viewport.scroll.clone();
    let selected = wheel.get() == Some(id);
    let mut viewport = div()
        .id(SharedString::from(format!("code-viewport-{id}")))
        .track_focus(&panel.focus)
        .relative()
        .h(px(VIEWPORT_HEIGHT))
        .w_full()
        .bg(appearance.body)
        .text_color(appearance.body_text)
        .font_family(appearance.body_font.font.family.clone())
        .font_weight(appearance.body_font.font.weight)
        .text_size(px(appearance.body_font.size))
        .overflow_hidden()
        .child(text)
        .child(
            canvas(
                move |bounds_value, _, _| {
                    bounds.borrow_mut().bounds = bounds_value;
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .on_mouse_move(
            cx.listener(move |panel, event: &gpui::MouseMoveEvent, window, app| {
                if panel.captured_discussion_interaction_gated() {
                    return;
                }
                visibility.record_viewport_activity(owner, window, app);
                let Some(code) = panel
                    .code_panels
                    .iter_mut()
                    .find(|panel| panel.id == id && !panel.overridden)
                else {
                    return;
                };
                if event.pressed_button != Some(MouseButton::Left) {
                    code.actions.dragging = false;
                    return;
                }
                if code.actions.dragging
                    && code.actions.binding.as_ref() == code.layout.source_binding()
                    && code.actions.root == code.layout.source_root()
                {
                    if let Some(source) =
                        selection::hit(&code.layout, &code.viewport, event.position)
                    {
                        code.actions.select(source, true);
                        panel.keyboard_copy.gesture.reset();
                        app.notify();
                    }
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |panel, event: &gpui::MouseDownEvent, window, cx| {
                if panel.captured_discussion_interaction_gated() {
                    cx.stop_propagation();
                    return;
                }
                let Some(code) = panel
                    .code_panels
                    .iter_mut()
                    .find(|panel| panel.id == id && !panel.overridden)
                else {
                    return;
                };
                if scroll.borrow().overflow() > 0.0 {
                    panel.code_wheel_owner.set(Some(id));
                } else {
                    panel.code_wheel_owner.set(None);
                }
                window.focus(&code.focus);
                if code.actions.binding.as_ref() == code.layout.source_binding()
                    && code.actions.root == code.layout.source_root()
                {
                    if let Some(source) =
                        selection::hit(&code.layout, &code.viewport, event.position)
                    {
                        code.actions.select(source, event.modifiers.shift);
                        panel.keyboard_copy.gesture.reset();
                        code.actions.dragging = true;
                    }
                }
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |panel, event: &gpui::MouseDownEvent, window, cx| {
                panel.open_code_discussion_menu(id, Some(event.position), window, cx);
                cx.stop_propagation();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |panel, _, _, _| {
                if let Some(code) = panel.code_panels.iter_mut().find(|panel| panel.id == id) {
                    code.actions.dragging = false;
                }
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(move |panel, _, _, _| {
                if let Some(code) = panel.code_panels.iter_mut().find(|panel| panel.id == id) {
                    code.actions.dragging = false;
                }
            }),
        );
    let scroll = panel.viewport.scroll.clone();
    viewport = viewport.on_key_down(cx.listener(
        move |panel, event: &gpui::KeyDownEvent, window, cx| {
            if !panel
                .code_panels
                .iter()
                .any(|panel| panel.id == id && !panel.overridden)
            {
                return;
            }
            if super::super::copy_keyboard::is_copy(event) {
                panel.copy_code_key(id, event, window, cx);
                cx.stop_propagation();
                return;
            }
            if event.keystroke.key == "f10" && event.keystroke.modifiers.shift {
                panel.open_code_discussion_menu(id, None, window, cx);
                cx.stop_propagation();
                return;
            }
            if panel.captured_discussion_interaction_gated() {
                cx.stop_propagation();
                return;
            }
            let mut scroll = scroll.borrow_mut();
            let line_height = scroll.line_height;
            match event.keystroke.key.as_str() {
                "up" => scroll.scroll_by(-line_height),
                "down" => scroll.scroll_by(line_height),
                "pageup" => scroll.scroll_by(-VIEWPORT_HEIGHT),
                "pagedown" => scroll.scroll_by(VIEWPORT_HEIGHT),
                "home" => scroll.set_offset(0.0),
                "end" => {
                    let end = scroll.overflow();
                    scroll.set_offset(end);
                }
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
        },
    ));
    let scroll = panel.viewport.scroll.clone();
    let visibility = panel.visibility.clone();
    viewport = viewport.on_scroll_wheel(cx.listener(
        move |panel, event: &gpui::ScrollWheelEvent, window, cx| {
            if panel.captured_discussion_interaction_gated() {
                cx.stop_propagation();
                return;
            }
            if !panel
                .code_panels
                .iter()
                .any(|panel| panel.id == id && !panel.overridden)
            {
                return;
            }
            let delta = event.delta.pixel_delta(window.line_height());
            let selected = panel.code_wheel_owner.get() == Some(id);
            if selected || delta.x != px(0.0) {
                let mut scroll = scroll.borrow_mut();
                if selected {
                    scroll.scroll_by(-f32::from(delta.y));
                }
                scroll.scroll_horizontal_by(-f32::from(delta.x));
                visibility.record_viewport_activity(owner, window, cx);
                cx.notify();
                if selected || delta.y == px(0.0) {
                    cx.stop_propagation();
                }
            }
        },
    ));
    if let Some(bar) = render_scrollbar(
        SharedString::from(format!("code-scrollbar-{id}")),
        panel.scrollbar.clone(),
        Axis::Vertical,
        appearance.scrollbar,
        panel.visibility.clone(),
        panel.interaction.clone(),
    ) {
        viewport = viewport.child(bar);
    }
    let header = div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(appearance.header_height()))
        .px(px(8.0))
        .bg(appearance.header)
        .text_color(appearance.header_text)
        .font_family(appearance.header_font.font.family.clone())
        .font_weight(appearance.header_font.font.weight)
        .text_size(px(appearance.header_font.size))
        .line_height(px(appearance.header_line_height()))
        .border_b_1()
        .border_color(appearance.border)
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
        .child(copy_button::render(
            panel,
            host,
            appearance,
            commands_pending,
            cx,
        ))
        .child(copy_button::render_save(
            panel,
            host,
            appearance,
            commands_pending,
            cx,
        ));
    #[cfg(test)]
    let header = {
        let measured = panel.command_bounds.clone();
        let save_measured = panel.save_bounds.clone();
        header.on_children_prepainted(move |bounds, _, _| {
            measured.set(bounds.get(1).copied());
            save_measured.set(bounds.get(2).copied());
        })
    };
    let rendered = shell(appearance.header_height());
    #[cfg(test)]
    let rendered = {
        let measured = panel.feedback_bounds.clone();
        rendered.on_children_prepainted(move |bounds, window, _| {
            measured.set(
                bounds
                    .get(2)
                    .copied()
                    .map(|footer| (footer, window.content_mask().bounds)),
            );
        })
    };
    let rendered = rendered
        .id(SharedString::from(format!("code-panel-{id}")))
        .bg(appearance.container)
        .text_color(appearance.body_text)
        .border_1()
        .border_color(if selected {
            appearance.selected_border
        } else {
            appearance.border
        })
        .rounded(px(6.0))
        .child(header)
        .child(
            div()
                .py(px(super::super::code_viewport::BODY_PADDING))
                .child(viewport),
        )
        .child(
            div()
                .h(px(super::super::code_viewport::FEEDBACK_HEIGHT))
                .px(px(8.0))
                .py(px(6.0))
                .border_t_1()
                .border_color(appearance.border)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .bg(if panel.actions.feedback.contains("unavailable") {
                    gpui::rgb(0xfef2f2)
                } else {
                    gpui::rgb(0xf1f5f9)
                })
                .text_color(if panel.actions.feedback.contains("unavailable") {
                    gpui::rgb(0xb91c1c)
                } else {
                    gpui::rgb(0x475569)
                })
                .child(panel.actions.feedback),
        );
    rendered.into_any_element()
}

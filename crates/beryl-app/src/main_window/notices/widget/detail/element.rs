use super::*;

pub(in super::super) struct NoticeDetailElement {
    id: ElementId,
    text: SharedString,
    state: DetailState,
    enabled: bool,
    focus: gpui::FocusHandle,
}

impl NoticeDetailElement {
    pub(in super::super) fn new(
        id: impl Into<ElementId>,
        text: SharedString,
        state: DetailState,
        enabled: bool,
        focus: gpui::FocusHandle,
    ) -> Self {
        Self {
            id: id.into(),
            text,
            state,
            enabled,
            focus,
        }
    }
}

impl IntoElement for NoticeDetailElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NoticeDetailElement {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let text = self.text.clone();
        let state = self.state.clone();
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style
            .line_height
            .to_pixels(font_size.into(), window.rem_size());
        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color: style.color,
            background_color: style.background_color,
            underline: None,
            strikethrough: None,
        };
        let layout =
            window.request_measured_layout(Style::default(), move |known, available, window, _| {
                let width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    _ => None,
                });
                let lines = window
                    .text_system()
                    .shape_text(text.clone(), font_size, &[run.clone()], width, None)
                    .unwrap_or_default();
                let mut origin = point(px(0.), px(0.));
                let mut range_start = 0;
                let mut detail_lines = Vec::with_capacity(lines.len());
                let mut measured = size(px(0.), px(0.));
                for line in lines {
                    let line_size = line.size(line_height);
                    measured.width = measured.width.max(line_size.width);
                    measured.height += line_size.height;
                    let range_end = range_start + line.len();
                    detail_lines.push(DetailLine {
                        line,
                        range: range_start..range_end,
                        origin,
                    });
                    origin.y += line_size.height;
                    range_start = range_end.saturating_add(1);
                }
                let layout = DetailLayout {
                    text: text.clone(),
                    bounds: Bounds::new(point(px(0.), px(0.)), measured),
                    lines: detail_lines,
                    line_height,
                    text_len: text.len(),
                    font: run.font.clone(),
                    font_size,
                };
                let mut detail_state = state.0.borrow_mut();
                if let Some(anchor) = detail_state.pending_scroll_anchor.take() {
                    detail_state.reset_scroll =
                        layout
                            .scroll_anchor(anchor.visual_top)
                            .is_none_or(|candidate| {
                                candidate.start != anchor.start
                                    || candidate.end != anchor.end
                                    || candidate.source[..candidate.end]
                                        != anchor.source[..anchor.end]
                                    || candidate.visual_top != anchor.visual_top
                                    || candidate.line_height != anchor.line_height
                                    || candidate.font != anchor.font
                                    || candidate.font_size != anchor.font_size
                            });
                }
                detail_state.layout = Some(layout);
                if detail_state.reset_scroll {
                    window.refresh();
                }
                measured
            });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
        if let Some(layout) = self.state.0.borrow_mut().layout.as_mut() {
            layout.bounds = bounds;
        }
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.enabled {
            let generation = self.state.0.borrow().generation;
            let state = self.state.clone();
            let focus = self.focus.clone();
            let hitbox = hitbox.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
                if phase == gpui::DispatchPhase::Bubble
                    && state.accepts(generation)
                    && event.button == MouseButton::Left
                    && hitbox.is_hovered(window)
                {
                    let index = state.index_for_position(event.position);
                    state.begin_drag(index, event.modifiers.shift, event.click_count);
                    focus.focus(window);
                    window.refresh();
                }
            });
            let state = self.state.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _| {
                if phase == gpui::DispatchPhase::Bubble
                    && state.accepts(generation)
                    && event.pressed_button == Some(MouseButton::Left)
                    && state.extend_drag(state.index_for_position(event.position))
                {
                    window.refresh();
                }
            });
            let state = self.state.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, window, _| {
                if phase == gpui::DispatchPhase::Bubble
                    && state.accepts(generation)
                    && state.selection().is_some()
                {
                    state.end_drag();
                    window.refresh();
                }
            });
        }

        let state = self.state.0.borrow();
        let Some(layout) = state.layout.as_ref() else {
            return;
        };
        for line in &layout.lines {
            let _ = line.line.paint_background(
                layout.bounds.origin + line.origin,
                layout.line_height,
                gpui::TextAlign::Left,
                Some(layout.bounds),
                window,
                cx,
            );
        }
        if let Some(selection) = state.selection {
            for quad in selection_quads(layout, selection.range(), gpui::rgba(0x38bdf84d).into()) {
                window.paint_quad(quad);
            }
        }
        for line in &layout.lines {
            let _ = line.line.paint(
                layout.bounds.origin + line.origin,
                layout.line_height,
                gpui::TextAlign::Left,
                Some(layout.bounds),
                window,
                cx,
            );
        }
    }
}

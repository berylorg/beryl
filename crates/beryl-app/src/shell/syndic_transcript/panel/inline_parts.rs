use super::*;
use std::{cell::Cell, rc::Rc};
use super::narrative_appearance::{NarrativeAppearance, NarrativeBlockAppearance, NarrativeBlockStyle};
use super::narrative_layout::{InlinePartsLayout, InlinePartsPreparation};

pub(super) struct InlinePartsGeometry {
    pub(super) record: ResidentPresentationRecord,
    pub(super) text: Vec<narrative::NarrativeGeometry>,
    pub(super) layout: Option<InlinePartsLayout>,
    pub(super) bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    focuses: Vec<(usize, FocusHandle)>,
    preparation: Option<InlinePartsPreparation>,
    block: NarrativeBlockAppearance,
    marker: super::narrative_appearance::NarrativeMarkerAppearance,
    refused: bool,
}

impl InlinePartsGeometry {
    pub(super) fn charge(&self) -> usize {
        self.layout.as_ref().map_or_else(|| self.preparation.as_ref().map_or(0, InlinePartsPreparation::charge), |layout| layout.charge)
    }
    pub(super) fn marker_bounds(&self, id: &ResidentPresentationRecordId) -> Option<gpui::Bounds<gpui::Pixels>> {
        let parts = self.record.authored_parts();
        let (_, bounds) = self.layout.as_ref()?.markers.iter().find(|(index, _)| &parts[*index].id == id)?;
        Some(gpui::Bounds::new(self.bounds.get()?.origin + bounds.origin + gpui::point(px(self.block.indent), px(0.0)), bounds.size))
    }
}

pub(super) fn measure(
    geometries: &mut Vec<InlinePartsGeometry>, host: &mut SyndicTranscriptHost,
    previous: &[InlinePartsGeometry],
    snapshot: &ResidentTranscriptSnapshot, record: &ResidentPresentationRecord,
    appearance: &NarrativeAppearance, width: gpui::Pixels, budget: usize,
    window: &mut Window, cx: &mut Context<SyndicTranscriptPanel>,
) -> (Option<f32>, usize) {
    if budget == 0 { return (None, 0); }
    let parts = record.authored_parts();
    let index = geometries.iter().position(|geometry| geometry.record == *record).unwrap_or_else(|| {
        let block = parts.iter().find_map(|part| {
            let ResidentPresentationRecordKind::NarrativeReady { prepared } = &part.kind else { return None };
            Some(match prepared.treatment() {
                syndic_storage::narrative::NarrativeTreatment::Heading(level) => NarrativeBlockStyle::Heading(level),
                syndic_storage::narrative::NarrativeTreatment::Quote => NarrativeBlockStyle::Quote,
                syndic_storage::narrative::NarrativeTreatment::List => NarrativeBlockStyle::List,
                _ => NarrativeBlockStyle::Paragraph,
            })
        }).unwrap_or(NarrativeBlockStyle::Paragraph);
        let focuses = parts.iter().enumerate().filter(|(_, part)| matches!(part.kind, ResidentPresentationRecordKind::ImageMarker(_)))
            .map(|(index, part)| {
                let old = previous.iter().find(|old| old.record == *record)
                    .and_then(|old| old.focuses.iter().find(|(old_index, _)| old.record.authored_parts()[*old_index] == *part))
                    .map(|(_, focus)| focus.clone());
                (index, old.unwrap_or_else(|| cx.focus_handle()))
            }).collect();
        geometries.push(InlinePartsGeometry { record: record.clone(), text: Vec::new(), layout: None, focuses,
            bounds: Rc::new(Cell::new(None)), preparation: None, block: appearance.block(block), marker: appearance.marker, refused: false });
        geometries.len() - 1
    });
    let geometry = &mut geometries[index];
    if let Some(layout) = &geometry.layout { return (Some(f32::from(layout.height) + 16.0), 1); }
    geometry.refused |= parts.iter().any(|part| matches!(part.kind, ResidentPresentationRecordKind::NarrativeUnavailable { .. }));
    if geometry.refused || parts.iter().any(|part| !matches!(part.kind,
        ResidentPresentationRecordKind::NarrativeReady { .. } | ResidentPresentationRecordKind::ImageMarker(_))) {
        return (Some(36.0), 1);
    }
    let allowance = host.inline_parts_layout_allowance(record);
    if !host.reserve_inline_parts_layout(record, allowance) || allowance < 4096 {
        geometry.refused = true;
        return (Some(36.0), 1);
    }
    let preparation = geometry.preparation.get_or_insert_with(|| InlinePartsPreparation::new(
        parts, snapshot.presentation_revision, (width - px(32.0 + geometry.block.indent)).max(px(1.0)),
        &geometry.block, allowance.saturating_sub(4096 + parts.len() * 1024),
    ));
    let mut remaining = budget;
    let result = preparation.advance(parts, appearance, &geometry.block, window, &mut remaining);
    match result {
        Ok(Some(mut layout)) => {
            layout.charge = layout.charge.saturating_add(4096 + parts.len() * 1024);
            if !host.reserve_inline_parts_layout(record, layout.charge) {
                geometry.refused = true;
                geometry.preparation = None;
                return (Some(36.0), budget - remaining);
            }
            geometry.text = layout.text.iter().filter_map(|(index, layout)| {
                let part = &parts[*index];
                let ResidentPresentationRecordKind::NarrativeReady { prepared } = &part.kind else { return None };
                let super::super::ResidentRecordSource::Syndic(source) = &part.provenance.source else { return None };
                Some(narrative::NarrativeGeometry::inline_part(super::super::NarrativeOwner {
                    view_id: source.view_id.clone(), record_id: part.id.clone(), activation_revision: snapshot.activation_revision,
                    provenance: part.provenance.clone(), prepared: prepared.clone(),
                }, geometry.block.clone(), layout.clone()))
            }).collect();
            let height = f32::from(layout.height) + 16.0;
            geometry.layout = Some(layout);
            geometry.preparation = None;
            (Some(height), budget - remaining)
        }
        Ok(None) => {
            if host.reserve_inline_parts_layout(record, preparation.charge().saturating_add(4096 + parts.len() * 1024)) {
                (None, budget - remaining)
            } else {
                geometry.refused = true;
                geometry.preparation = None;
                host.reserve_inline_parts_layout(record, 0);
                (Some(36.0), budget - remaining)
            }
        }
        Err(_) => {
            geometry.refused = true;
            geometry.preparation = None;
            host.reserve_inline_parts_layout(record, 0);
            (Some(36.0), budget - remaining)
        }
    }
}

pub(super) fn render(geometry: &InlinePartsGeometry, ranges: &[(ResidentPresentationRecordId, Range<usize>)], cx: &mut Context<SyndicTranscriptPanel>) -> AnyElement {
    let Some(layout) = &geometry.layout else {
        return div().child(if geometry.refused { "Inline content unavailable" } else { "Inline content preparing" }).into_any_element();
    };
    let bounds = geometry.bounds.clone();
    let mut element = div().relative().w_full().h(layout.height).child(canvas(
        move |area, _, _| bounds.set(Some(area)), |_, _, _, _| {},
    ).size_full().absolute());
    if let Some(marker) = geometry.text.first().and_then(|text| text.owner.prepared.list_marker()) {
        element = element.child(div().absolute().left_0().top_0().w(px(geometry.block.indent))
            .text_size(px(geometry.block.text.size)).text_color(geometry.block.text.color)
            .line_height(px(geometry.block.text.size * 20.0 / 14.0)).whitespace_nowrap()
            .child(SharedString::from(marker.to_owned())));
    }
    for text in &geometry.text {
        element = element.child(div().absolute().top_0().left_0().right_0().child(narrative::render(text,
            ranges.iter().find(|(id, _)| id == &text.owner.record_id).map(|(_, range)| range.clone()))));
    }
    for (index, bounds) in &layout.markers {
        let part = &geometry.record.authored_parts()[*index];
        let ResidentPresentationRecordKind::ImageMarker(marker) = part.kind else { continue };
        let id = part.id.clone();
        let Some((_, focus)) = geometry.focuses.iter().find(|(part, _)| part == index) else { continue };
        let focus = focus.clone();
        let pointer_focus = focus.clone();
        let click_focus = focus.clone();
        let click_id = id.clone();
        let key_id = id.clone();
        let key_focus = focus.clone();
        let menu_focus = focus.clone();
        let selected = ranges.iter().any(|(id, range)| id == &part.id && range == &(0..1));
        let colors = geometry.marker;
        element = element.child(div().id(SharedString::from(format!("transcript-inline-marker:{}", marker.marker.marker_id())))
            .debug_selector(|| format!("transcript-image-marker:{}", marker.marker.marker_id()))
            .absolute().left(bounds.origin.x + px(geometry.block.indent)).top(bounds.origin.y).w(bounds.size.width).h(bounds.size.height)
            .px(px(5.0)).py(px(1.0)).border_1().border_color(colors.foreground).rounded(px(5.0))
            .bg(if selected { colors.selected_background } else { colors.background }).text_color(if selected {colors.selected_foreground} else {colors.foreground}).text_size(px(12.0))
            .font_weight(gpui::FontWeight::SEMIBOLD).line_height(px(16.0)).whitespace_nowrap().opacity(0.8)
            .track_focus(&focus)
            .focusable()
            .hover(|style| if selected { style } else { style.bg(colors.hover_background).text_color(colors.hover_foreground) })
            .focus(|style| style.shadow(vec![gpui::BoxShadow {
                color: colors.focus.into(), offset: gpui::point(px(0.0), px(0.0)),
                blur_radius: px(0.0), spread_radius: px(2.0),
            }]))
            .on_mouse_down(MouseButton::Left, cx.listener(move |_, _, window, cx| {
                window.focus(&pointer_focus); cx.stop_propagation();
            }))
            .on_click(cx.listener(move |panel, _, window, cx| {
                panel.request_image_marker_inspection(&click_id, click_focus.clone(), window, cx);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(move |panel, event: &gpui::KeyDownEvent, window, cx| {
                if key_focus.is_focused(window) && matches!(event.keystroke.key.as_str(), "enter" | "space" | " ") {
                    panel.request_image_marker_inspection(&key_id, key_focus.clone(), window, cx);
                    cx.stop_propagation();
                } else if key_focus.is_focused(window) && event.keystroke.key == "f10" && event.keystroke.modifiers.shift {
                    if let Some(bounds) = panel.image_marker_bounds(&key_id, window) {
                        panel.open_record_menu(&key_id, bounds.center(), window, cx);
                    }
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |panel, event: &gpui::MouseDownEvent, window, cx| {
                window.focus(&menu_focus);
                panel.open_record_menu(&id, event.position, window, cx); cx.stop_propagation();
            }))
            .child(format!("Image {}", marker.marker.label())));
    }
    element.into_any_element()
}

impl SyndicTranscriptPanel {
    #[cfg(test)]
    pub(crate) fn image_marker_inspection_diagnostic_for_test(&self, id: &ResidentPresentationRecordId, window: &Window) -> String {
        format!("marker={id:?}, bounds={:?}, serial={}, pending={:?}, fallback={}, focused={}", self.image_marker_bounds(id, window), self.image_inspection_serial, self.image_inspection_pending, self.image_marker_focus_fallback.is_some(), self.image_marker_focus_handle(id).is_some_and(|focus| focus.is_focused(window)))
    }

    pub(crate) fn set_image_marker_focus_fallback(&mut self, focus: FocusHandle) {
        self.image_marker_focus_fallback = Some(focus);
    }

    pub(crate) fn image_marker_return_focus(&self, window: &Window) -> Option<FocusHandle> {
        let fallback = self.image_marker_focus_fallback.clone()?;
        if self.character_geometry_current(window) && matches!(self.host.shared_snapshot().state,
            super::super::ResidentTranscriptSnapshotState::ProviderBacked { .. }
                | super::super::ResidentTranscriptSnapshotState::Incomplete { .. }) {
            Some(self.focus_handle.clone())
        } else {
            Some(fallback)
        }
    }

    pub(in super) fn image_marker_focus_handle(&self, id: &ResidentPresentationRecordId) -> Option<FocusHandle> {
        self.inline_parts_geometry.iter().find_map(|group| group.focuses.iter()
            .find(|(index, _)| group.record.authored_parts()[*index].id == *id)
            .map(|(_, focus)| focus.clone()))
    }

    fn request_image_marker_inspection(&mut self, id: &ResidentPresentationRecordId, focus: FocusHandle, window: &mut Window, cx: &mut Context<Self>) {
        if self.image_inspection_pending.is_some() || self.image_marker_focus_fallback.is_none() { return; }
        let Some(bounds) = self.image_marker_bounds(id, window) else { return };
        let Some(snapshot) = self.published_snapshot.as_ref() else { return };
        let Some(frame) = self.last_frame_window.as_ref() else { return };
        let Ok(command) = resident_context_menu_command_for_realized_record_id(snapshot, frame, id) else { return };
        let Some(part) = snapshot.records.iter().find_map(|record| record.authored_part(id)) else { return };
        let Ok(record) = super::super::context_menu::resident_context_menu_record(part, command.record) else { return };
        let Some(request) = self.image_inspection_serial.checked_add(1) else { return };
        let event = ImageMarkerInspectionRequested { request, target: ResidentTranscriptContextMenuTarget::new(snapshot.presentation_revision, record),
            activation: snapshot.activation_revision, presentation: snapshot.presentation_revision, bounds, focus };
        self.image_inspection_serial = request;
        self.image_inspection_pending = Some(request);
        cx.emit(event);
        cx.notify();
    }

    pub(crate) fn image_marker_inspection_current(&self, event: &ImageMarkerInspectionRequested, window: &Window) -> bool {
        let snapshot = self.host.shared_snapshot();
        self.image_inspection_pending == Some(event.request)
            && snapshot.activation_revision == event.activation
            && snapshot.presentation_revision == event.presentation
            && event.target.presentation_revision == event.presentation
            && matches!(event.target.record.content_kind, super::super::ResidentContextMenuContentKind::ImageMarker(_))
            && self.image_marker_bounds(&event.target.record.record_id, window) == Some(event.bounds)
            && self.last_frame_window.as_ref().is_some_and(|frame| resident_context_menu_frame_loss(&snapshot, frame, &event.target).is_none())
    }

    pub(crate) fn finish_image_marker_inspection(&mut self, request: u64) {
        if self.image_inspection_pending == Some(request) { self.image_inspection_pending = None; }
    }

    pub(crate) fn image_marker_bounds(&self, id: &ResidentPresentationRecordId, window: &Window) -> Option<gpui::Bounds<gpui::Pixels>> {
        if !self.character_geometry_current(window) { return None; }
        self.inline_parts_geometry.iter().find(|geometry| geometry.record.authored_part(id).is_some()
            && self.host.inline_parts_current(&geometry.record))?.marker_bounds(id)
    }
}

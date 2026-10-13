use super::*;
use crate::widgets::anchored_context_menu::{
    self as widget, AnchoredContextMenu, MenuCollection, MenuColors, MenuEvent, MenuIntent,
    MenuRow, MenuRowKind,
};
use gpui::{Bounds, size};

#[derive(Default)]
pub(in super::super) struct QuoteState {
    serial: u64,
    pub(in super::super) capture: Option<QuoteCapture>,
    popup: Option<AnchoredContextMenu>,
    anchor: Option<Bounds<Pixels>>,
}

pub(in super::super) struct QuoteCapture {
    selection: NarrativeSelection,
    bindings: Vec<(ResidentPresentationRecordId, usize, usize, usize)>,
    ranges: Vec<(ResidentPresentationRecordId, Range<usize>)>,
    cancellation: beryl_home_store::CommandCancellation,
}

impl Drop for QuoteCapture {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

impl SyndicTranscriptPanel {
    #[cfg(test)]
    pub(crate) fn quote_menu_open_for_test(&self) -> bool {
        self.quote_state.popup.is_some()
    }
    #[cfg(test)]
    pub(crate) fn quote_pointer_facts_for_test(
        &self,
        pointer: Point<Pixels>,
        window: &Window,
    ) -> (bool, Option<usize>, Vec<Range<usize>>, bool, u64) {
        let hit = self.narrative_hit(pointer);
        let ranges = self
            .narrative_selection
            .as_ref()
            .and_then(|selection| self.narrative_ranges(selection))
            .unwrap_or_default()
            .into_iter()
            .filter(|(id, _)| hit.as_ref().is_some_and(|hit| &hit.record == id))
            .map(|(_, range)| range)
            .collect();
        let eligible = self.quote_capture().is_some_and(|capture| {
            self.host
                .mapped_quote_markdown(
                    capture.selection.activation,
                    capture.selection.revision,
                    &capture.ranges,
                )
                .is_ok()
        });
        (
            self.character_geometry_current(window),
            hit.map(|hit| hit.display),
            ranges,
            eligible,
            self.quote_state.serial,
        )
    }
    fn quote_capture(&self) -> Option<QuoteCapture> {
        let selection = self.narrative_selection.clone()?;
        let display_ranges = self.narrative_ranges(&selection)?;
        let mut metadata = std::mem::size_of::<QuoteCapture>();
        let mut ranges = Vec::new();
        let mut bindings = Vec::new();
        for (id, display) in display_ranges {
            metadata = metadata
                .checked_add(id.0.len().checked_mul(2)?)?
                .checked_add(std::mem::size_of::<(
                    ResidentPresentationRecordId,
                    usize,
                    usize,
                    usize,
                )>())?
                .checked_add(std::mem::size_of::<(
                    ResidentPresentationRecordId,
                    Range<usize>,
                )>())?;
            if metadata > 65_536 {
                return None;
            }
            let geometry = self.mapped_geometry(&id)?;
            let local = geometry.source_range(display)?;
            bindings.push((
                id.clone(),
                geometry.layout() as *const _ as usize,
                geometry.source().as_ptr() as usize,
                geometry.source().len(),
            ));
            ranges.push((id, local));
        }
        Some(QuoteCapture {
            selection,
            bindings,
            ranges,
            cancellation: beryl_home_store::CommandCancellation::default(),
        })
    }

    pub(crate) fn quote_request_current(
        &self,
        request: u64,
        activation: u64,
        presentation: u64,
        window: &Window,
    ) -> bool {
        let Some(capture) = &self.quote_state.capture else {
            return false;
        };
        let current = self.host.shared_snapshot();
        request == self.quote_state.serial
            && capture.selection.activation == activation
            && capture.selection.revision == presentation
            && current.activation_revision == activation
            && current.presentation_revision == presentation
            && self.narrative_selection.as_ref() == Some(&capture.selection)
            && self.character_geometry_current(window)
            && capture.bindings.iter().all(|(id, layout, source, len)| {
                self.mapped_geometry(id).is_some_and(|geometry| {
                    geometry.layout() as *const _ as usize == *layout
                        && geometry.source().as_ptr() as usize == *source
                        && geometry.source().len() == *len
                })
            })
    }

    pub(in super::super) fn open_quote_menu(
        &mut self,
        pointer: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_quote_menu(window);
        if pointer.is_none() && !self.focus_handle.is_focused(window) {
            return;
        }
        if !self.character_geometry_current(window) {
            return;
        }
        if let Some(pointer) = pointer {
            let Some(hit) = self.narrative_hit(pointer) else {
                return;
            };
            let Some(selection) = &self.narrative_selection else {
                return;
            };
            if !self.narrative_ranges(selection).is_some_and(|ranges| {
                ranges
                    .iter()
                    .any(|(id, range)| id == &hit.record && range.contains(&hit.display))
            }) {
                return;
            }
        }
        let Some(capture) = self.quote_capture() else {
            return;
        };
        if let Err(error) = self.host.mapped_quote_markdown(
            capture.selection.activation,
            capture.selection.revision,
            &capture.ranges,
        ) {
            if matches!(
                error,
                super::super::ResidentSelectionUnavailable::CopyCapacity
                    | super::super::ResidentSelectionUnavailable::CopyAllocation
            ) {
                self.keyboard_copy.feedback = "Quote selection is too large. Select less text.";
                cx.notify();
            }
            return;
        }
        let Some(serial) = self.quote_state.serial.checked_add(1) else {
            return;
        };
        let Some(bounds) = self.narrative_viewport_bounds else {
            return;
        };
        let anchor = pointer.or_else(|| {
            let head = &capture.selection.head;
            let row = self
                .last_frame_window
                .as_ref()?
                .records
                .iter()
                .find(|row| row.record_id == head.record)?;
            let geometry = self.mapped_geometry(&head.record)?;
            let position = geometry.layout().position(head.display)?;
            let origin = match &geometry {
                MappedGeometry::Prepared(value) => {
                    let snapshot = self.published_snapshot.as_ref()?;
                    let record = snapshot.records.get(row.index)?;
                    bounds.origin
                        + point(
                            px(16. + value.block.indent),
                            px(row.top_px
                                + super::super::record_vertical_padding(snapshot, record).0),
                        )
                }
                MappedGeometry::Literal(value) => value.bounds.get()?.origin,
            };
            Some(origin + position)
        });
        let Some(anchor) = anchor else {
            return;
        };
        self.quote_state.serial = serial;
        self.quote_state.capture = Some(capture);
        self.quote_state.anchor = Some(Bounds::new(anchor, size(px(1.), px(1.))));
        let mut popup = AnchoredContextMenu::open(serial, window, cx);
        popup.reconcile(&QuoteRows);
        popup.realize(&[QuoteRows.row(0)], 0..1, window);
        self.quote_state.popup = Some(popup);
        cx.notify();
    }

    fn close_quote_menu(&mut self, window: &mut Window) {
        if let Some(mut popup) = self.quote_state.popup.take() {
            popup.dismiss(window, &self.focus_handle);
        }
        self.quote_state.anchor = None;
    }

    pub(crate) fn render_quote_menu(
        &mut self,
        colors: MenuColors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.quote_state.popup.is_none() {
            return None;
        }
        let current = self.quote_state.capture.as_ref().is_some_and(|capture| {
            self.quote_request_current(
                self.quote_state.serial,
                capture.selection.activation,
                capture.selection.revision,
                window,
            )
        });
        if !current {
            self.quote_state.capture = None;
            self.close_quote_menu(window);
            return None;
        }
        Some(widget::render_static(
            self.quote_state.popup.as_ref()?,
            self.quote_state.anchor?,
            vec![QuoteRows.row(0)],
            colors,
            "transcript-selection-menu",
            quote_input,
            window,
            cx,
        ))
    }
}

struct QuoteRows;
impl MenuCollection for QuoteRows {
    fn len(&self) -> usize {
        1
    }
    fn index_of(&self, id: &str) -> Option<usize> {
        (id == "transcript-quote").then_some(0)
    }
    fn row(&self, index: usize) -> MenuRow {
        assert_eq!(index, 0);
        MenuRow {
            index,
            id: Some(Arc::from("transcript-quote")),
            label: "Quote".into(),
            kind: MenuRowKind::Command,
            selected: false,
            enabled: true,
            disabled_reason: None,
            selector: "transcript-quote".into(),
        }
    }
}

fn quote_input(
    panel: &mut SyndicTranscriptPanel,
    event: MenuEvent,
    window: &mut Window,
    cx: &mut Context<SyndicTranscriptPanel>,
) {
    let Some(popup) = panel.quote_state.popup.as_mut() else {
        return;
    };
    for intent in popup.input(event, &QuoteRows) {
        match intent {
            MenuIntent::Dismiss => {
                panel.close_quote_menu(window);
                cx.notify();
            }
            MenuIntent::Focus(_) => cx.notify(),
            MenuIntent::Activate { .. } => {
                let Some(capture) = &panel.quote_state.capture else {
                    return;
                };
                if !panel.quote_request_current(
                    panel.quote_state.serial,
                    capture.selection.activation,
                    capture.selection.revision,
                    window,
                ) {
                    panel.quote_state.capture = None;
                    panel.close_quote_menu(window);
                    cx.notify();
                    return;
                }
                let Ok(markdown) = panel.host.mapped_quote_markdown(
                    capture.selection.activation,
                    capture.selection.revision,
                    &capture.ranges,
                ) else {
                    return;
                };
                let request = super::super::QuoteInsertionRequested {
                    markdown,
                    request: panel.quote_state.serial,
                    activation: capture.selection.activation,
                    presentation: capture.selection.revision,
                    cancellation: capture.cancellation.clone(),
                };
                panel.close_quote_menu(window);
                cx.emit(request);
                cx.notify();
            }
        }
    }
}

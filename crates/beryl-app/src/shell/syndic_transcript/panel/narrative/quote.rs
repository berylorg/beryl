#[cfg(test)]
mod testing;

use super::super::super::TranscriptNarrativeKind;
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
    turn: Option<TurnCapture>,
    popup: Option<AnchoredContextMenu>,
    anchor: Option<Bounds<Pixels>>,
}

struct TurnCapture {
    target: super::super::ResidentTranscriptContextMenuTarget,
    activation: u64,
    selection: Option<NarrativeSelection>,
    layout: usize,
    source: usize,
    source_len: usize,
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
    pub(in super::super) fn invalidate_transcript_menu(&mut self) {
        self.quote_state.capture = None;
        self.quote_state.turn = None;
        self.host.clear_resident_context_menu_target();
    }

    fn transcript_menu_rows(&self) -> QuoteRows {
        QuoteRows {
            quote: self.quote_state.capture.is_some(),
            edit: self.quote_state.turn.is_some(),
        }
    }

    fn turn_menu_current(&self, window: &Window) -> bool {
        let Some(capture) = &self.quote_state.turn else {
            return false;
        };
        let snapshot = self.host.shared_snapshot();
        self.character_geometry_current(window)
            && snapshot.activation_revision == capture.activation
            && self.narrative_selection == capture.selection
            && self.host.resident_context_menu_target().as_ref() == Some(&capture.target)
            && self.host.context_menu_narrative_kind(&capture.target)
                == Some(TranscriptNarrativeKind::UserInput)
            && self.last_frame_window.as_ref().is_some_and(|frame| {
                super::super::resident_context_menu_frame_loss(&snapshot, frame, &capture.target)
                    .is_none()
            })
            && self
                .mapped_geometry(&capture.target.record.record_id)
                .is_some_and(|geometry| {
                    geometry.layout() as *const _ as usize == capture.layout
                        && geometry.source().as_ptr() as usize == capture.source
                        && geometry.source().len() == capture.source_len
                })
    }

    fn transcript_menu_current(&self, window: &Window) -> bool {
        let quote_current = self.quote_state.capture.as_ref().is_none_or(|capture| {
            self.quote_request_current(
                self.quote_state.serial,
                capture.selection.activation,
                capture.selection.revision,
                window,
            )
        });
        let turn_current = self.quote_state.turn.is_none() || self.turn_menu_current(window);
        (self.quote_state.capture.is_some() || self.quote_state.turn.is_some())
            && quote_current
            && turn_current
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
        let record_target = self
            .host
            .resident_context_menu_target()
            .filter(|target| {
                self.character_geometry_current(window)
                    && self.last_frame_window.as_ref().is_some_and(|frame| {
                        super::super::resident_context_menu_frame_loss(
                            &self.host.shared_snapshot(),
                            frame,
                            target,
                        )
                        .is_none()
                    })
            })
            .map(|target| NarrativeEndpoint {
                record: target.record.record_id,
                display: 0,
            });
        self.close_quote_menu(window);
        self.invalidate_transcript_menu();
        if pointer.is_none() && !self.focus_handle.is_focused(window) {
            return;
        }
        if !self.character_geometry_current(window) {
            return;
        }
        let endpoint = if let Some(pointer) = pointer {
            self.narrative_hit(pointer)
        } else {
            self.narrative_selection
                .as_ref()
                .map(|selection| selection.head.clone())
                .or(record_target)
        };
        let Some(endpoint) = endpoint else {
            return;
        };
        let selected = pointer.is_none()
            || self.narrative_selection.as_ref().is_some_and(|selection| {
                self.narrative_ranges(selection).is_some_and(|ranges| {
                    ranges.iter().any(|(id, range)| {
                        id == &endpoint.record && range.contains(&endpoint.display)
                    })
                })
            });
        let capture = selected
            .then(|| self.quote_capture())
            .flatten()
            .filter(|capture| {
                match self.host.mapped_quote_markdown(
                    capture.selection.activation,
                    capture.selection.revision,
                    &capture.ranges,
                ) {
                    Ok(markdown) => !markdown.is_empty(),
                    Err(error) => {
                        if matches!(
                            error,
                            super::super::ResidentSelectionUnavailable::CopyCapacity
                                | super::super::ResidentSelectionUnavailable::CopyAllocation
                        ) {
                            self.keyboard_copy.feedback =
                                "Quote selection is too large. Select less text.";
                            cx.notify();
                        }
                        false
                    }
                }
            });
        let turn = match self.apply_realized_context_menu_target_for_record_id(&endpoint.record) {
            super::super::ResidentContextMenuOutcome::Targeted(target)
                if self.host.context_menu_narrative_kind(&target)
                    == Some(TranscriptNarrativeKind::UserInput) =>
            {
                self.mapped_geometry(&endpoint.record)
                    .map(|geometry| TurnCapture {
                        target,
                        activation: self.host.shared_snapshot().activation_revision,
                        selection: self.narrative_selection.clone(),
                        layout: geometry.layout() as *const _ as usize,
                        source: geometry.source().as_ptr() as usize,
                        source_len: geometry.source().len(),
                    })
            }
            _ => None,
        };
        if turn.is_none() {
            self.host.clear_resident_context_menu_target();
        }
        if capture.is_none() && turn.is_none() {
            return;
        }
        let Some(serial) = self.quote_state.serial.checked_add(1) else {
            return;
        };
        let Some(bounds) = self.narrative_viewport_bounds else {
            return;
        };
        let anchor = pointer.or_else(|| {
            let head = &endpoint;
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
        self.quote_state.capture = capture;
        self.quote_state.turn = turn;
        self.quote_state.anchor = Some(Bounds::new(anchor, size(px(1.), px(1.))));
        let mut popup = AnchoredContextMenu::open(serial, window, cx);
        let rows = self.transcript_menu_rows();
        popup.reconcile(&rows);
        popup.realize(
            &(0..rows.len())
                .map(|index| rows.row(index))
                .collect::<Vec<_>>(),
            0..rows.len(),
            window,
        );
        self.quote_state.popup = Some(popup);
        cx.notify();
    }

    pub(in super::super) fn close_quote_menu(&mut self, window: &mut Window) {
        if let Some(mut popup) = self.quote_state.popup.take() {
            popup.dismiss(window, &self.focus_handle);
        }
        self.quote_state.anchor = None;
        self.quote_state.turn = None;
        self.host.clear_resident_context_menu_target();
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
        let current = self.transcript_menu_current(window);
        if !current {
            self.quote_state.capture = None;
            self.close_quote_menu(window);
            return None;
        }
        let rows = self.transcript_menu_rows();
        Some(widget::render_static(
            self.quote_state.popup.as_ref()?,
            self.quote_state.anchor?,
            (0..rows.len()).map(|index| rows.row(index)).collect(),
            colors,
            "transcript-selection-menu",
            quote_input,
            window,
            cx,
        ))
    }
}

struct QuoteRows {
    quote: bool,
    edit: bool,
}
impl MenuCollection for QuoteRows {
    fn len(&self) -> usize {
        usize::from(self.quote) + usize::from(self.edit)
    }
    fn index_of(&self, id: &str) -> Option<usize> {
        if self.quote && id == "transcript-quote" {
            Some(0)
        } else if self.edit && id == "transcript-edit-message" {
            Some(usize::from(self.quote))
        } else {
            None
        }
    }
    fn row(&self, index: usize) -> MenuRow {
        assert!(index < self.len() && self.len() <= 2);
        let quote = self.quote && index == 0;
        let id = if quote {
            "transcript-quote"
        } else {
            "transcript-edit-message"
        };
        MenuRow {
            index,
            id: Some(Arc::from(id)),
            label: if quote { "Quote" } else { "Edit message" }.into(),
            kind: MenuRowKind::Command,
            selected: false,
            enabled: quote,
            disabled_reason: (!quote)
                .then(|| "Historical message editing is not available yet.".into()),
            selector: id.into(),
        }
    }
}

fn quote_input(
    panel: &mut SyndicTranscriptPanel,
    event: MenuEvent,
    window: &mut Window,
    cx: &mut Context<SyndicTranscriptPanel>,
) {
    if !panel.transcript_menu_current(window) {
        panel.invalidate_transcript_menu();
        panel.close_quote_menu(window);
        cx.notify();
        return;
    }
    let rows = panel.transcript_menu_rows();
    let Some(popup) = panel.quote_state.popup.as_mut() else {
        return;
    };
    for intent in popup.input(event, &rows) {
        match intent {
            MenuIntent::Dismiss => {
                panel.close_quote_menu(window);
                cx.notify();
            }
            MenuIntent::Focus(_) => cx.notify(),
            MenuIntent::Activate { id, .. } => {
                if id.as_ref() != "transcript-quote" {
                    return;
                }
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

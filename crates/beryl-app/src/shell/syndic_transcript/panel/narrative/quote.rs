mod code;
mod discussion;
mod image_commands;

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
    image_media: std::sync::Weak<crate::image_processing::ImageMediaWindow>,
    image_commands: Option<image_commands::DirectImageCommands>,
    image_command_feedback: Option<image_commands::DirectImageFeedback>,
    serial: u64,
    pub(in super::super) capture: Option<QuoteCapture>,
    turn: Option<TurnCapture>,
    popup: Option<AnchoredContextMenu>,
    anchor: Option<Bounds<Pixels>>,
    marker_origin: Option<(
        super::super::ResidentTranscriptContextMenuTarget,
        gpui::FocusHandle,
    )>,
    edit_disabled_reason: Option<&'static str>,
    branch_label: Option<&'static str>,
    branch_disabled_reason: Option<&'static str>,
    branch_retry: bool,
    branch_source: Option<crate::syndic_transcript::CapturedDiscussionSource>,
    branch_qualified: Option<Result<(), &'static str>>,
    branch_pending: bool,
    code_capture: Option<code::CodeDiscussionCapture>,
    preserved_code_selection: Option<code::CodeDiscussionCapture>,
    preserved_branch_selection: Option<(
        NarrativeSelection,
        crate::syndic_transcript::CapturedDiscussionSource,
        super::super::MeasurementEnvironment,
    )>,
    restore_branch_selection: bool,
    #[cfg(test)]
    last_open_diagnostic: Option<String>,
}

struct TurnCapture {
    target: super::super::ResidentTranscriptContextMenuTarget,
    activation: u64,
    selection: Option<NarrativeSelection>,
    anchor: Option<crate::syndic_transcript::RealizedFrameAnchor>,
    narrative: Option<(usize, usize, usize)>,
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
    pub(crate) fn set_historical_edit_disabled_reason(&mut self, reason: Option<&'static str>) {
        self.quote_state.edit_disabled_reason = reason;
    }

    pub(in super::super) fn invalidate_transcript_menu(&mut self) {
        self.quote_state.image_commands = None;
        self.quote_state.image_command_feedback = None;
        self.quote_state.branch_qualified = None;
        self.quote_state.capture = None;
        self.quote_state.code_capture = None;
        self.quote_state.turn = None;
        self.host.clear_resident_context_menu_target();
    }

    fn transcript_menu_rows(&self) -> QuoteRows {
        let branch_source = self.discussion_menu_source();
        let branch_current =
            branch_source.is_some() && branch_source == self.quote_state.branch_source;
        let edit_reason = if self
            .quote_state
            .turn
            .as_ref()
            .is_some_and(|capture| capture.target.record.historical_target().is_none())
        {
            Some("This message has no exact current editing source.")
        } else {
            self.quote_state.edit_disabled_reason
        };
        QuoteRows {
            image_copy_reason: self.quote_state.image_commands.as_ref().map_or(
                Some("Checking the exact original image."),
                |commands| {
                    commands
                        .original
                        .reason(crate::image_processing::OriginalImageRequest::Copy)
                },
            ),
            image_save_reason: self.quote_state.image_commands.as_ref().map_or(
                Some("Checking the exact original image."),
                |commands| {
                    commands
                        .original
                        .reason(crate::image_processing::OriginalImageRequest::Save)
                },
            ),
            quote: self.quote_state.capture.is_some() || self.quote_state.code_capture.is_some(),
            branch: branch_source.is_some(),
            branch_label: if branch_current {
                self.quote_state
                    .branch_label
                    .unwrap_or("Discuss in new branch")
            } else {
                "Discuss in new branch"
            },
            branch_reason: if branch_current && self.quote_state.branch_label.is_some() {
                self.quote_state.branch_disabled_reason
            } else {
                match &self.quote_state.branch_qualified {
                    Some(Ok(())) => None,
                    Some(Err(reason)) => Some(*reason),
                    None => Some("Checking the exact finalized assistant source."),
                }
            },
            edit: self.quote_state.turn.as_ref().is_some_and(|capture| {
                !matches!(
                    capture.target.record.content_kind,
                    crate::syndic_transcript::ResidentContextMenuContentKind::GeneratedImage(_)
                )
            }),
            edit_reason,
            image: self
                .quote_state
                .turn
                .as_ref()
                .is_some_and(|capture| capture.target.record.content_kind.is_image()),
        }
    }

    fn turn_menu_current(&self, window: &Window) -> bool {
        let Some(capture) = &self.quote_state.turn else {
            return false;
        };
        if matches!(
            capture.target.record.content_kind,
            crate::syndic_transcript::ResidentContextMenuContentKind::GeneratedImage(_)
        ) {
            return self
                .refreshed_generated_image_target(&capture.target, window)
                .is_some();
        }
        let snapshot = self.host.shared_snapshot();
        self.character_geometry_current(window)
            && snapshot.activation_revision == capture.activation
            && self.narrative_selection == capture.selection
            && self.host.resident_context_menu_target().as_ref() == Some(&capture.target)
            && self.host.context_menu_narrative_kind(&capture.target)
                == Some(TranscriptNarrativeKind::UserInput)
            && (!matches!(
                capture.target.record.content_kind,
                crate::syndic_transcript::ResidentContextMenuContentKind::ImageMarker(_)
            ) || self.image_marker_bounds(&capture.target.record.record_id, window)
                == self.quote_state.anchor)
            && self.last_frame_window.as_ref().is_some_and(|frame| {
                frame.anchor == capture.anchor
                    && super::super::resident_context_menu_frame_loss(
                        &snapshot,
                        frame,
                        &capture.target,
                    )
                    .is_none()
            })
            && capture
                .narrative
                .is_none_or(|(layout, source, source_len)| {
                    self.mapped_geometry(&capture.target.record.record_id)
                        .is_some_and(|geometry| {
                            geometry.layout() as *const _ as usize == layout
                                && geometry.source().as_ptr() as usize == source
                                && geometry.source().len() == source_len
                        })
                })
    }

    pub(crate) fn historical_replacement_request_current(
        &self,
        request: &super::super::HistoricalReplacementRequested,
        window: &Window,
    ) -> bool {
        let snapshot = self.host.shared_snapshot();
        self.quote_state.serial == request.request
            && !request.cancellation.is_cancelled()
            && self.character_geometry_current(window)
            && snapshot.activation_revision == request.activation
            && snapshot.presentation_revision == request.presentation
            && request.target.presentation_revision == request.presentation
            && request.target.record.historical_target().is_some()
            && self.host.context_menu_narrative_kind(&request.target)
                == Some(TranscriptNarrativeKind::UserInput)
            && self.last_frame_window.as_ref().is_some_and(|frame| {
                frame.anchor == request.anchor
                    && super::super::resident_context_menu_frame_loss(
                        &snapshot,
                        frame,
                        &request.target,
                    )
                    .is_none()
            })
    }

    fn transcript_menu_current(&self, window: &Window) -> bool {
        if let Some(capture) = &self.quote_state.code_capture {
            return self.code_discussion_request_current(
                self.quote_state.serial,
                capture.activation,
                capture.presentation,
                window,
            );
        }
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
        if self.quote_state.code_capture.is_some() {
            return self.code_discussion_request_current(request, activation, presentation, window);
        }
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
        self.open_transcript_menu(None, pointer, window, cx);
    }

    pub(in super::super) fn open_record_menu(
        &mut self,
        record: &ResidentPresentationRecordId,
        pointer: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.image_marker_bounds(record, window).is_some()
            && self.image_marker_focus_fallback.is_none()
        {
            return;
        }
        if self.quote_state.popup.is_some()
            && self.transcript_menu_current(window)
            && self.quote_state.turn.as_ref().is_some_and(|capture| {
                capture.target.record.record_id == *record
                    && capture.target.record.content_kind.is_image()
            })
        {
            return;
        }
        self.open_transcript_menu(Some(record), Some(pointer), window, cx);
    }

    fn open_transcript_menu(
        &mut self,
        record: Option<&ResidentPresentationRecordId>,
        pointer: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.initial_history_loading {
            return;
        }
        #[cfg(test)]
        {
            self.quote_state.last_open_diagnostic = Some(format!(
                "record={record:?}, pointer={pointer:?}, geometry_current={}",
                self.character_geometry_current(window)
            ));
        }
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
        let endpoint = if let Some(record) = record {
            self.narrative_hit(pointer.unwrap())
                .filter(|hit| {
                    &hit.record == record
                        || self.published_snapshot.as_ref().is_some_and(|snapshot| {
                            snapshot.records.iter().any(|row| {
                                &row.id == record && row.authored_part(&hit.record).is_some()
                            })
                        })
                })
                .or_else(|| {
                    Some(NarrativeEndpoint {
                        record: record.clone(),
                        display: 0,
                    })
                })
        } else if let Some(pointer) = pointer {
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
        let target_outcome =
            self.apply_realized_context_menu_target_for_record_id(&endpoint.record);
        #[cfg(test)]
        {
            self.quote_state
                .last_open_diagnostic
                .as_mut()
                .unwrap()
                .push_str(&format!(
                    ", endpoint={:?}, admission={target_outcome:?}",
                    endpoint.record
                ));
        }
        let turn = match target_outcome {
            super::super::ResidentContextMenuOutcome::Targeted(target)
                if self.host.context_menu_narrative_kind(&target)
                    == Some(TranscriptNarrativeKind::UserInput)
                    || matches!(
                        target.record.content_kind,
                        crate::syndic_transcript::ResidentContextMenuContentKind::GeneratedImage(_)
                    ) =>
            {
                Some(TurnCapture {
                    target,
                    activation: self.host.shared_snapshot().activation_revision,
                    selection: self.narrative_selection.clone(),
                    anchor: self
                        .last_frame_window
                        .as_ref()
                        .and_then(|frame| frame.anchor.clone()),
                    narrative: self.mapped_geometry(&endpoint.record).map(|geometry| {
                        (
                            geometry.layout() as *const _ as usize,
                            geometry.source().as_ptr() as usize,
                            geometry.source().len(),
                        )
                    }),
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
                .find(|row| {
                    self.published_snapshot
                        .as_ref()
                        .and_then(|snapshot| snapshot.records.get(row.index))
                        .is_some_and(|record| record.authored_part(&head.record).is_some())
                })?;
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
        let marker_bounds = turn.as_ref().and_then(|capture| {
            capture
                .target
                .record
                .content_kind
                .is_image()
                .then(|| self.image_marker_bounds(&capture.target.record.record_id, window))
                .flatten()
        });
        self.quote_state.marker_origin = turn
            .as_ref()
            .filter(|_| marker_bounds.is_some())
            .and_then(|capture| {
                self.image_marker_focus_handle(&capture.target.record.record_id)
                    .map(|focus| (capture.target.clone(), focus))
            });
        self.quote_state.serial = serial;
        self.quote_state.capture = capture;
        self.quote_state.turn = turn;
        self.quote_state.anchor =
            marker_bounds.or_else(|| Some(Bounds::new(anchor, size(px(1.), px(1.)))));
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
        self.open_image_original_commands(window, cx);
        if let Some(capture) = &self.quote_state.capture {
            if let Some(captured) = self.host.mapped_discussion_source(
                capture.selection.activation,
                capture.selection.revision,
                &capture.ranges,
            ) {
                cx.emit(super::super::DiscussionSourceQualificationRequested(
                    super::super::DiscussionCreationRequested {
                        captured,
                        request: serial,
                        activation: capture.selection.activation,
                        presentation: capture.selection.revision,
                    },
                ));
            }
        }
        cx.notify();
    }

    pub(in super::super) fn close_quote_menu(&mut self, window: &mut Window) {
        if self
            .quote_state
            .image_commands
            .as_ref()
            .is_some_and(|commands| !commands.effect_started)
        {
            self.quote_state.image_commands = None;
        }
        let marker = self.quote_state.marker_origin.take();
        let code_focus = self.quote_state.code_capture.as_ref().and_then(|capture| {
            self.code_panels
                .iter()
                .find(|code| code.id == capture.id)
                .map(|code| code.focus.clone())
        });
        let fallback = code_focus
            .or_else(|| {
                marker
                    .as_ref()
                    .and_then(|_| self.image_marker_return_focus(window))
            })
            .unwrap_or_else(|| self.focus_handle.clone());
        if let Some(mut popup) = self.quote_state.popup.take() {
            popup.dismiss(window, &fallback);
            if let Some((target, focus)) = marker {
                let eligible = self
                    .image_marker_bounds(&target.record.record_id, window)
                    .is_some()
                    && self.last_frame_window.as_ref().is_some_and(|frame| {
                        super::super::resident_context_menu_frame_loss(
                            &self.host.shared_snapshot(),
                            frame,
                            &target,
                        )
                        .is_none()
                    });
                if !eligible && focus.is_focused(window) {
                    window.focus(&fallback);
                }
            }
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
        self.refresh_generated_image_commands(window);
        self.poll_image_original_commands(window, cx);
        if self.initial_history_loading || self.quote_state.popup.is_none() {
            return self.image_original_feedback(window);
        }
        let current = self.transcript_menu_current(window);
        if !current {
            self.quote_state.capture = None;
            self.close_quote_menu(window);
            return None;
        }
        let rows = self.transcript_menu_rows();
        self.quote_state.popup.as_mut()?.reconcile(&rows);
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
    image_copy_reason: Option<&'static str>,
    image_save_reason: Option<&'static str>,
    branch: bool,
    branch_label: &'static str,
    branch_reason: Option<&'static str>,
    quote: bool,
    edit: bool,
    image: bool,
    edit_reason: Option<&'static str>,
}
impl MenuCollection for QuoteRows {
    fn len(&self) -> usize {
        usize::from(self.quote)
            + usize::from(self.branch)
            + usize::from(self.edit)
            + 2 * usize::from(self.image)
    }
    fn index_of(&self, id: &str) -> Option<usize> {
        if self.quote && id == "transcript-quote" {
            Some(0)
        } else if self.branch && id == "transcript-discuss-branch" {
            Some(usize::from(self.quote))
        } else if self.edit && id == "transcript-edit-message" {
            Some(usize::from(self.quote) + usize::from(self.branch) + 2 * usize::from(self.image))
        } else if self.image && id == "transcript-image-copy" {
            Some(usize::from(self.quote) + usize::from(self.branch))
        } else if self.image && id == "transcript-image-save" {
            Some(usize::from(self.quote) + usize::from(self.branch) + 1)
        } else {
            None
        }
    }
    fn row(&self, index: usize) -> MenuRow {
        assert!(index < self.len() && self.len() <= 5);
        let quote = self.quote && index == 0;
        let media_index = index.saturating_sub(usize::from(self.quote) + usize::from(self.branch));
        let (id, label, reason) = if quote {
            ("transcript-quote", "Quote", None)
        } else if self.branch && index == usize::from(self.quote) {
            (
                "transcript-discuss-branch",
                self.branch_label,
                self.branch_reason,
            )
        } else if self.image && media_index < 2 {
            (
                if media_index == 0 {
                    "transcript-image-copy"
                } else {
                    "transcript-image-save"
                },
                if media_index == 0 { "Copy" } else { "Save…" },
                if media_index == 0 {
                    self.image_copy_reason
                } else {
                    self.image_save_reason
                },
            )
        } else {
            ("transcript-edit-message", "Edit message", self.edit_reason)
        };
        MenuRow {
            index,
            id: Some(Arc::from(id)),
            label: label.into(),
            kind: MenuRowKind::Command,
            selected: false,
            enabled: quote
                || ((id == "transcript-edit-message"
                    || id == "transcript-discuss-branch"
                    || id == "transcript-image-copy"
                    || id == "transcript-image-save")
                    && reason.is_none()),
            disabled_reason: reason.map(Into::into),
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
    panel.refresh_generated_image_commands(window);
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
                if id.as_ref() == "transcript-image-copy" || id.as_ref() == "transcript-image-save"
                {
                    panel.activate_image_original_command(
                        id.as_ref() == "transcript-image-save",
                        window,
                        cx,
                    );
                    return;
                }
                if id.as_ref() == "transcript-discuss-branch" {
                    if panel.quote_state.code_capture.is_some() {
                        panel.activate_code_discussion(window, cx);
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
                        return;
                    }
                    let Some(captured) = panel.host.mapped_discussion_source(
                        capture.selection.activation,
                        capture.selection.revision,
                        &capture.ranges,
                    ) else {
                        return;
                    };
                    let Some(environment) = panel
                        .geometry
                        .as_ref()
                        .map(|geometry| geometry.environment.clone())
                    else {
                        return;
                    };
                    panel.quote_state.preserved_branch_selection =
                        Some((capture.selection.clone(), captured.clone(), environment));
                    panel.quote_state.preserved_code_selection = None;
                    panel.quote_state.restore_branch_selection = false;
                    if panel.quote_state.branch_retry
                        && panel.quote_state.branch_source.as_ref() == Some(&captured)
                    {
                        cx.emit(super::super::DiscussionActivationRetryRequested(
                            super::super::DiscussionCreationRequested {
                                captured,
                                request: panel.quote_state.serial,
                                activation: capture.selection.activation,
                                presentation: capture.selection.revision,
                            },
                        ));
                        return;
                    }
                    cx.emit(super::super::DiscussionCreationRequested {
                        captured,
                        request: panel.quote_state.serial,
                        activation: capture.selection.activation,
                        presentation: capture.selection.revision,
                    });
                    return;
                }
                if id.as_ref() == "transcript-edit-message" {
                    let Some(capture) = &panel.quote_state.turn else {
                        return;
                    };
                    let request = super::super::HistoricalReplacementRequested {
                        target: capture.target.clone(),
                        request: panel.quote_state.serial,
                        activation: capture.activation,
                        presentation: capture.target.presentation_revision,
                        anchor: capture.anchor.clone(),
                        cancellation: beryl_home_store::CommandCancellation::new(),
                    };
                    if !panel.historical_replacement_request_current(&request, window) {
                        return;
                    }
                    panel.close_quote_menu(window);
                    cx.emit(request);
                    cx.notify();
                    return;
                }
                if id.as_ref() != "transcript-quote" {
                    return;
                }
                if panel.quote_state.code_capture.is_some() {
                    panel.quote_code_discussion(window, cx);
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

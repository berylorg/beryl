use std::ops::Range;

use gpui::{
    Action, AnyElement, App, Context, FocusHandle, Focusable, MouseButton, Render, SharedString,
    Window, div, prelude::*, px,
};

use super::super::OpenTranscriptContextMenu;
use super::{
    DemandFact, DemandFactKind, LocalPresentationReason, ManualTranscriptScrollCommand,
    PreparedTranscriptActivation, RealizedFrameRecord, RealizedFrameRequest, RealizedFrameWindow,
    ResidentContextMenuCommandTarget, ResidentContextMenuOutcome, ResidentContextMenuUnavailable,
    ResidentFallbackTarget, ResidentMediaActionOutcome, ResidentMediaActionUnavailable,
    ResidentMediaCopyCommandTarget, ResidentMediaPreviewCommandTarget,
    ResidentMediaSaveCommandTarget, ResidentPresentationRecord, ResidentPresentationRecordId,
    ResidentPresentationRecordKind, ResidentQuoteOutcome, ResidentResourceSlice,
    ResidentSelectionOutcome, ResidentSelectionUnavailable, ResidentTranscriptContextMenuTarget,
    ResidentTranscriptCopyPayload, ResidentTranscriptMediaActionTarget,
    ResidentTranscriptQuotePayload, ResidentTranscriptQuoteTarget, ResidentTranscriptSelection,
    ResidentTranscriptSnapshot, ResidentTranscriptStatusFacts, ResourceId, ResourceKind,
    ResourceMetadata, SyndicTranscriptDiagnosticSnapshot, SyndicTranscriptHost,
    TranscriptActivationOutcome, TranscriptActivationSeed, TranscriptActivationSource,
    TranscriptCommandResult, realized_resident_selectable_record_ids,
    resident_context_menu_command_for_realized_record_id, resident_context_menu_frame_loss,
    resident_media_action_command_for_realized_record_id, resident_media_action_frame_loss,
    resident_quote_command_for_realized_record_ids, resident_quote_frame_loss,
    resident_selection_command_for_realized_record_ids, resident_selection_frame_loss,
};
use crate::diagnostic_dynamic_tools::TranscriptFrameMetricsSnapshot;

pub(crate) const SYNDIC_TRANSCRIPT_KEY_CONTEXT: &str = "SyndicTranscriptPanel";
const SYNDIC_TRANSCRIPT_OVERSCAN_RATIO: f32 = 0.5;
const SYNDIC_TRANSCRIPT_DEFAULT_RECORD_HEIGHT_PX: f32 = 72.0;
const SYNDIC_TRANSCRIPT_RESOURCE_PREVIEW_BYTES: u64 = 4096;
const SYNDIC_TRANSCRIPT_RESOURCE_PREVIEW_TEXT_BYTES: usize = 2048;

pub(crate) struct SyndicTranscriptPanel {
    focus_handle: FocusHandle,
    host: SyndicTranscriptHost,
    last_frame_window: Option<RealizedFrameWindow>,
}

impl SyndicTranscriptPanel {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host: SyndicTranscriptHost::empty(),
            last_frame_window: None,
        }
    }

    pub(crate) fn snapshot(&self) -> ResidentTranscriptSnapshot {
        self.host.snapshot()
    }

    pub(crate) fn status_facts(&self) -> ResidentTranscriptStatusFacts {
        self.host.status_facts()
    }

    pub(crate) fn diagnostic_snapshot(&self) -> SyndicTranscriptDiagnosticSnapshot {
        self.host.diagnostic_snapshot()
    }

    pub(crate) fn frame_metrics_snapshot(&self) -> TranscriptFrameMetricsSnapshot {
        self.host.frame_metrics_snapshot()
    }

    pub(crate) fn begin_activation(
        &mut self,
        seed: TranscriptActivationSeed,
    ) -> TranscriptActivationOutcome {
        self.host.begin_activation(seed)
    }

    pub(crate) fn apply_prepared_activation(
        &mut self,
        prepared: PreparedTranscriptActivation,
        source: TranscriptActivationSource,
    ) -> TranscriptActivationOutcome {
        self.host.apply_prepared_activation(prepared, source)
    }

    pub(crate) fn manual_scroll(
        &mut self,
        command: ManualTranscriptScrollCommand,
    ) -> RealizedFrameWindow {
        let window = self.host.manual_scroll(command);
        self.remember_realized_frame(window)
    }

    pub(crate) fn manual_scroll_delta(
        &mut self,
        viewport_height_px: f32,
        delta_px: f32,
        cx: &mut Context<Self>,
    ) -> RealizedFrameWindow {
        let snapshot = self.host.snapshot();
        let window = self.manual_scroll(ManualTranscriptScrollCommand::new(
            viewport_height_px,
            viewport_height_px * SYNDIC_TRANSCRIPT_OVERSCAN_RATIO,
            SYNDIC_TRANSCRIPT_DEFAULT_RECORD_HEIGHT_PX,
            delta_px,
            Some(snapshot.presentation_revision),
        ));
        cx.notify();
        window
    }

    pub(crate) fn close_transient_popups_for_dynamic_tool(
        &mut self,
        _: &mut Context<Self>,
    ) -> bool {
        false
    }

    pub(crate) fn unavailable_command(&self, command: &'static str) -> TranscriptCommandResult {
        self.host.unavailable_command(command)
    }

    pub(crate) fn resident_copy_payload(
        &self,
    ) -> Result<ResidentTranscriptCopyPayload, ResidentSelectionUnavailable> {
        self.host.resident_copy_payload()
    }

    pub(crate) fn resident_quote_payload(
        &self,
    ) -> Result<ResidentTranscriptQuotePayload, ResidentSelectionUnavailable> {
        self.host.resident_quote_payload()
    }

    pub(crate) fn resident_quote_target(&self) -> Option<ResidentTranscriptQuoteTarget> {
        self.host.resident_quote_target()
    }

    pub(crate) fn resident_context_menu_target(
        &self,
    ) -> Option<ResidentTranscriptContextMenuTarget> {
        self.host.resident_context_menu_target()
    }

    pub(crate) fn resident_context_menu_command_target(&self) -> ResidentContextMenuCommandTarget {
        self.host.resident_context_menu_command_target()
    }

    pub(crate) fn resident_media_action_target(
        &self,
    ) -> Option<ResidentTranscriptMediaActionTarget> {
        self.host.resident_media_action_target()
    }

    pub(crate) fn resident_media_preview_command_target(
        &self,
    ) -> ResidentMediaPreviewCommandTarget {
        self.host.resident_media_preview_command_target()
    }

    pub(crate) fn resident_media_copy_command_target(&self) -> ResidentMediaCopyCommandTarget {
        self.host.resident_media_copy_command_target()
    }

    pub(crate) fn resident_media_save_command_target(&self) -> ResidentMediaSaveCommandTarget {
        self.host.resident_media_save_command_target()
    }

    pub(crate) fn apply_realized_selection(
        &mut self,
        record_ids: &[ResidentPresentationRecordId],
        cx: &mut Context<Self>,
    ) -> ResidentSelectionOutcome {
        let outcome = self.apply_realized_selection_for_record_ids(record_ids);
        cx.notify();
        outcome
    }

    pub(crate) fn apply_realized_selection_for_record_ids(
        &mut self,
        record_ids: &[ResidentPresentationRecordId],
    ) -> ResidentSelectionOutcome {
        let Some(frame_window) = self.last_frame_window.as_ref() else {
            let _ = self.host.clear_resident_selection();
            return ResidentSelectionOutcome::Unavailable(
                ResidentSelectionUnavailable::NoRealizedFrame,
            );
        };
        let snapshot = self.host.snapshot();
        match resident_selection_command_for_realized_record_ids(
            &snapshot,
            frame_window,
            record_ids,
        ) {
            Ok(command) => self.host.apply_resident_selection(command),
            Err(error) => {
                let _ = self.host.clear_resident_selection();
                ResidentSelectionOutcome::Unavailable(error)
            }
        }
    }

    pub(crate) fn apply_realized_quote_target(
        &mut self,
        record_ids: &[ResidentPresentationRecordId],
        cx: &mut Context<Self>,
    ) -> ResidentQuoteOutcome {
        let outcome = self.apply_realized_quote_target_for_record_ids(record_ids);
        cx.notify();
        outcome
    }

    pub(crate) fn apply_realized_quote_target_for_record_ids(
        &mut self,
        record_ids: &[ResidentPresentationRecordId],
    ) -> ResidentQuoteOutcome {
        let Some(frame_window) = self.last_frame_window.as_ref() else {
            let _ = self.host.clear_resident_quote_target();
            return ResidentQuoteOutcome::Unavailable(
                ResidentSelectionUnavailable::NoRealizedFrame,
            );
        };
        let snapshot = self.host.snapshot();
        match resident_quote_command_for_realized_record_ids(&snapshot, frame_window, record_ids) {
            Ok(command) => self.host.apply_resident_quote_target(command),
            Err(error) => {
                let _ = self.host.clear_resident_quote_target();
                ResidentQuoteOutcome::Unavailable(error)
            }
        }
    }

    pub(crate) fn apply_realized_context_menu_target(
        &mut self,
        record_id: &ResidentPresentationRecordId,
        cx: &mut Context<Self>,
    ) -> ResidentContextMenuOutcome {
        let outcome = self.apply_realized_context_menu_target_for_record_id(record_id);
        cx.notify();
        outcome
    }

    pub(crate) fn apply_realized_context_menu_target_for_record_id(
        &mut self,
        record_id: &ResidentPresentationRecordId,
    ) -> ResidentContextMenuOutcome {
        let Some(frame_window) = self.last_frame_window.as_ref() else {
            let _ = self.host.clear_resident_context_menu_target();
            return ResidentContextMenuOutcome::Unavailable(
                ResidentContextMenuUnavailable::NoRealizedFrame,
            );
        };
        let snapshot = self.host.snapshot();
        match resident_context_menu_command_for_realized_record_id(
            &snapshot,
            frame_window,
            record_id,
        ) {
            Ok(command) => self.host.apply_resident_context_menu_target(command),
            Err(error) => {
                let _ = self.host.clear_resident_context_menu_target();
                ResidentContextMenuOutcome::Unavailable(error)
            }
        }
    }

    pub(crate) fn clear_resident_context_menu_target(
        &mut self,
        cx: &mut Context<Self>,
    ) -> ResidentContextMenuOutcome {
        let outcome = self.host.clear_resident_context_menu_target();
        cx.notify();
        outcome
    }

    pub(crate) fn apply_realized_media_action_target(
        &mut self,
        record_id: &ResidentPresentationRecordId,
        cx: &mut Context<Self>,
    ) -> ResidentMediaActionOutcome {
        let outcome = self.apply_realized_media_action_target_for_record_id(record_id);
        cx.notify();
        outcome
    }

    pub(crate) fn apply_realized_media_action_target_for_record_id(
        &mut self,
        record_id: &ResidentPresentationRecordId,
    ) -> ResidentMediaActionOutcome {
        let Some(frame_window) = self.last_frame_window.as_ref() else {
            let _ = self.host.clear_resident_media_action_target();
            return ResidentMediaActionOutcome::Unavailable(
                ResidentMediaActionUnavailable::NoRealizedFrame,
            );
        };
        let snapshot = self.host.snapshot();
        match resident_media_action_command_for_realized_record_id(
            &snapshot,
            frame_window,
            record_id,
        ) {
            Ok(command) => self.host.apply_resident_media_action_target(command),
            Err(error) => {
                let _ = self.host.clear_resident_media_action_target();
                ResidentMediaActionOutcome::Unavailable(error)
            }
        }
    }

    pub(crate) fn clear_resident_media_action_target(
        &mut self,
        cx: &mut Context<Self>,
    ) -> ResidentMediaActionOutcome {
        let outcome = self.host.clear_resident_media_action_target();
        cx.notify();
        outcome
    }

    fn remember_realized_frame(&mut self, window: RealizedFrameWindow) -> RealizedFrameWindow {
        let snapshot = self.host.snapshot();
        self.reconcile_resident_selection_with_frame(&snapshot, &window);
        self.reconcile_resident_quote_target_with_frame(&snapshot, &window);
        self.reconcile_resident_context_menu_target_with_frame(&snapshot, &window);
        self.reconcile_resident_media_action_target_with_frame(&snapshot, &window);
        self.last_frame_window = Some(window.clone());
        window
    }

    fn reconcile_resident_selection_with_frame(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        frame_window: &RealizedFrameWindow,
    ) -> Option<ResidentSelectionUnavailable> {
        let selection = self.host.resident_selection()?;
        let frame_loss = resident_selection_frame_loss(snapshot, frame_window, &selection)?;
        let _ = self.host.clear_resident_selection();
        Some(frame_loss)
    }

    fn reconcile_resident_quote_target_with_frame(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        frame_window: &RealizedFrameWindow,
    ) -> Option<ResidentSelectionUnavailable> {
        let target = self.host.resident_quote_target()?;
        let frame_loss = resident_quote_frame_loss(snapshot, frame_window, &target)?;
        let _ = self.host.clear_resident_quote_target();
        Some(frame_loss)
    }

    fn reconcile_resident_context_menu_target_with_frame(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        frame_window: &RealizedFrameWindow,
    ) -> Option<ResidentContextMenuUnavailable> {
        let target = self.host.resident_context_menu_target()?;
        let frame_loss = resident_context_menu_frame_loss(snapshot, frame_window, &target)?;
        let _ = self.host.clear_resident_context_menu_target();
        Some(frame_loss)
    }

    fn reconcile_resident_media_action_target_with_frame(
        &mut self,
        snapshot: &ResidentTranscriptSnapshot,
        frame_window: &RealizedFrameWindow,
    ) -> Option<ResidentMediaActionUnavailable> {
        let target = self.host.resident_media_action_target()?;
        let frame_loss = resident_media_action_frame_loss(snapshot, frame_window, &target)?;
        let _ = self.host.clear_resident_media_action_target();
        Some(frame_loss)
    }
}

impl Focusable for SyndicTranscriptPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SyndicTranscriptPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport_size = window.viewport_size();
        let viewport_width_px = f32::from(viewport_size.width);
        let viewport_height_px = f32::from(viewport_size.height);
        let snapshot = self.host.snapshot();

        self.host.push_demand_fact(DemandFact::new(
            snapshot.presentation_revision,
            DemandFactKind::Viewport {
                width_px: viewport_width_px,
                height_px: viewport_height_px,
            },
        ));

        let frame_window = self.host.realize_frame(RealizedFrameRequest {
            viewport_height_px,
            overscan_height_px: viewport_height_px * SYNDIC_TRANSCRIPT_OVERSCAN_RATIO,
            default_record_height_px: SYNDIC_TRANSCRIPT_DEFAULT_RECORD_HEIGHT_PX,
            manual_delta_px: 0.0,
            observed_presentation_revision: Some(snapshot.presentation_revision),
        });
        report_nested_resource_demands(&snapshot, &frame_window, &mut self.host);
        self.reconcile_resident_selection_with_frame(&snapshot, &frame_window);
        self.reconcile_resident_quote_target_with_frame(&snapshot, &frame_window);
        self.reconcile_resident_context_menu_target_with_frame(&snapshot, &frame_window);
        self.reconcile_resident_media_action_target_with_frame(&snapshot, &frame_window);
        let selected_record_ids =
            render_selected_record_ids(&snapshot, &frame_window, self.host.resident_selection());
        self.last_frame_window = Some(frame_window.clone());

        div()
            .relative()
            .size_full()
            .min_h(px(0.0))
            .overflow_hidden()
            .key_context(SYNDIC_TRANSCRIPT_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .child(render_frame_window(
                &snapshot,
                &frame_window,
                &selected_record_ids,
                cx,
            ))
    }
}

fn render_frame_window(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    selected_record_ids: &[ResidentPresentationRecordId],
    cx: &mut Context<SyndicTranscriptPanel>,
) -> gpui::Div {
    let mut frame = div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .min_h(px(0.0))
        .min_w(px(0.0));

    for frame_record in &frame_window.records {
        let Some(record) = snapshot.records.get(frame_record.index) else {
            continue;
        };
        if record.id != frame_record.record_id {
            continue;
        }

        frame = frame.child(render_realized_record(
            snapshot,
            record,
            frame_record,
            record_is_selected(&record.id, selected_record_ids),
            cx,
        ));
    }

    frame
}

fn render_realized_record(
    snapshot: &ResidentTranscriptSnapshot,
    record: &ResidentPresentationRecord,
    frame_record: &RealizedFrameRecord,
    is_selected: bool,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> AnyElement {
    let context_record_id = record.id.clone();
    div()
        .id(SharedString::from(format!(
            "syndic-transcript-record:{}",
            record.id.0
        )))
        .absolute()
        .left_0()
        .right_0()
        .top(px(frame_record.top_px))
        .h(px(frame_record.height_px))
        .min_h(px(frame_record.height_px))
        .overflow_hidden()
        .px_4()
        .py_2()
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |panel, _, window, cx| {
                let _ = panel.apply_realized_context_menu_target(&context_record_id, cx);
                window.dispatch_action(OpenTranscriptContextMenu.boxed_clone(), cx);
                cx.stop_propagation();
            }),
        )
        .child(render_record_content(snapshot, record))
        .when(is_selected, |row| {
            row.child(render_selection_affordance(&record.id))
        })
        .into_any_element()
}

fn render_selected_record_ids(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    selection: Option<ResidentTranscriptSelection>,
) -> Vec<ResidentPresentationRecordId> {
    let Some(selection) = selection else {
        return Vec::new();
    };
    let selectable_record_ids = realized_resident_selectable_record_ids(snapshot, frame_window);

    selection
        .record_ids()
        .into_iter()
        .filter(|record_id| record_is_selected(record_id, &selectable_record_ids))
        .collect()
}

fn record_is_selected(
    record_id: &ResidentPresentationRecordId,
    selected_record_ids: &[ResidentPresentationRecordId],
) -> bool {
    selected_record_ids
        .iter()
        .any(|selected_record_id| selected_record_id == record_id)
}

fn render_selection_affordance(record_id: &ResidentPresentationRecordId) -> AnyElement {
    div()
        .id(SharedString::from(format!(
            "syndic-transcript-selection-affordance:{}",
            record_id.0
        )))
        .absolute()
        .left_0()
        .top_0()
        .bottom_0()
        .w(px(2.0))
        .into_any_element()
}

fn render_record_content(
    snapshot: &ResidentTranscriptSnapshot,
    record: &ResidentPresentationRecord,
) -> AnyElement {
    match &record.kind {
        ResidentPresentationRecordKind::TextChunk { text, .. } => div()
            .w_full()
            .min_w(px(0.0))
            .text_sm()
            .line_height(px(20.0))
            .whitespace_normal()
            .child(text.clone())
            .into_any_element(),
        ResidentPresentationRecordKind::ResourceReference {
            resource_id,
            resource_kind,
            label,
        } => render_resource_reference(snapshot, resource_id, resource_kind, label),
        ResidentPresentationRecordKind::LocalUiFallback { reason, target } => div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap_1()
            .opacity(0.78)
            .child(
                div()
                    .text_sm()
                    .line_height(px(20.0))
                    .whitespace_normal()
                    .child(fallback_reason_label(reason).to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .line_height(px(16.0))
                    .whitespace_nowrap()
                    .truncate()
                    .child(fallback_target_label(target)),
            )
            .into_any_element(),
        ResidentPresentationRecordKind::LocalAffordance => {
            div().w_full().h(px(1.0)).min_h(px(1.0)).into_any_element()
        }
    }
}

fn report_nested_resource_demands(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    host: &mut SyndicTranscriptHost,
) {
    let mut reported_ranges: Vec<(ResourceId, Range<u64>)> = Vec::new();
    let mut reported_media_pins: Vec<ResourceId> = Vec::new();

    for frame_record in &frame_window.records {
        let Some(record) = snapshot.records.get(frame_record.index) else {
            continue;
        };
        if record.id != frame_record.record_id {
            continue;
        }

        let ResidentPresentationRecordKind::ResourceReference {
            resource_id,
            resource_kind,
            ..
        } = &record.kind
        else {
            continue;
        };

        if resource_kind_is_media(resource_kind)
            && !reported_media_pins
                .iter()
                .any(|reported_id| reported_id == resource_id)
        {
            host.push_demand_fact(DemandFact::new(
                snapshot.presentation_revision,
                DemandFactKind::MediaPreviewPin {
                    resource_id: resource_id.clone(),
                },
            ));
            reported_media_pins.push(resource_id.clone());
        }

        let Some(range) = resource_demand_range(snapshot, resource_id) else {
            continue;
        };
        if reported_ranges.iter().any(|(reported_id, reported_range)| {
            reported_id == resource_id && reported_range == &range
        }) {
            continue;
        }

        host.push_demand_fact(DemandFact::new(
            snapshot.presentation_revision,
            DemandFactKind::ResourceRange {
                resource_id: resource_id.clone(),
                range: range.clone(),
            },
        ));
        reported_ranges.push((resource_id.clone(), range));
    }
}

fn resource_demand_range(
    snapshot: &ResidentTranscriptSnapshot,
    resource_id: &ResourceId,
) -> Option<Range<u64>> {
    let range = snapshot
        .resources
        .metadata_for(resource_id)
        .and_then(metadata_preview_range)
        .unwrap_or(0..SYNDIC_TRANSCRIPT_RESOURCE_PREVIEW_BYTES);

    if range.start >= range.end || resource_slices_cover_range(snapshot, resource_id, &range) {
        return None;
    }

    Some(range)
}

fn metadata_preview_range(metadata: &ResourceMetadata) -> Option<Range<u64>> {
    if let Some(range) = metadata.preview_range.clone() {
        return bounded_resource_preview_range(range);
    }

    if metadata.byte_len == 0 {
        return None;
    }

    bounded_resource_preview_range(0..metadata.byte_len)
}

fn bounded_resource_preview_range(range: Range<u64>) -> Option<Range<u64>> {
    let end = range.end.min(
        range
            .start
            .saturating_add(SYNDIC_TRANSCRIPT_RESOURCE_PREVIEW_BYTES),
    );
    (range.start < end).then_some(range.start..end)
}

fn resource_slices_cover_range(
    snapshot: &ResidentTranscriptSnapshot,
    resource_id: &ResourceId,
    range: &Range<u64>,
) -> bool {
    snapshot
        .resources
        .slices_for(resource_id)
        .any(|slice| slice.range.start <= range.start && slice.range.end >= range.end)
}

fn render_resource_reference(
    snapshot: &ResidentTranscriptSnapshot,
    resource_id: &ResourceId,
    resource_kind: &ResourceKind,
    label: &Option<String>,
) -> AnyElement {
    let metadata = snapshot.resources.metadata_for(resource_id);
    let title = label.clone().unwrap_or_else(|| resource_id.0.clone());
    let mut shell = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .opacity(0.72)
                .whitespace_nowrap()
                .truncate()
                .child(resource_kind_label(resource_kind).to_string()),
        )
        .child(
            div()
                .text_sm()
                .line_height(px(20.0))
                .whitespace_nowrap()
                .truncate()
                .child(title),
        );

    shell = if let Some(metadata) = metadata {
        shell.child(
            div()
                .text_xs()
                .opacity(0.72)
                .whitespace_nowrap()
                .truncate()
                .child(resource_metadata_label(metadata)),
        )
    } else {
        shell.child(
            div()
                .text_xs()
                .opacity(0.72)
                .whitespace_nowrap()
                .truncate()
                .child("Resource metadata pending"),
        )
    };

    shell
        .child(render_resource_shell(snapshot, resource_id, resource_kind))
        .into_any_element()
}

fn render_resource_shell(
    snapshot: &ResidentTranscriptSnapshot,
    resource_id: &ResourceId,
    resource_kind: &ResourceKind,
) -> AnyElement {
    match resource_kind {
        ResourceKind::Code => render_text_resource_shell(snapshot, resource_id, "Code preview"),
        ResourceKind::Table => render_text_resource_shell(snapshot, resource_id, "Table preview"),
        ResourceKind::Image | ResourceKind::GeneratedImage => {
            render_binary_resource_shell(snapshot, resource_id, "Media preview")
        }
        ResourceKind::Attachment => {
            render_binary_resource_shell(snapshot, resource_id, "Attachment preview")
        }
        ResourceKind::Other(_) => {
            render_binary_resource_shell(snapshot, resource_id, "Unsupported resource")
        }
    }
}

fn render_text_resource_shell(
    snapshot: &ResidentTranscriptSnapshot,
    resource_id: &ResourceId,
    title: &'static str,
) -> AnyElement {
    let Some(slice) = first_resident_resource_slice(snapshot, resource_id) else {
        return render_resource_pending_line(title);
    };

    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .opacity(0.72)
                .whitespace_nowrap()
                .truncate()
                .child(resource_slice_label(slice)),
        )
        .child(
            div()
                .text_xs()
                .line_height(px(16.0))
                .font_family("monospace".to_string())
                .whitespace_normal()
                .child(resource_preview_text(slice)),
        )
        .into_any_element()
}

fn render_binary_resource_shell(
    snapshot: &ResidentTranscriptSnapshot,
    resource_id: &ResourceId,
    title: &'static str,
) -> AnyElement {
    let label = first_resident_resource_slice(snapshot, resource_id)
        .map(resource_slice_label)
        .unwrap_or_else(|| format!("{title} pending"));

    div()
        .w_full()
        .min_w(px(0.0))
        .text_xs()
        .line_height(px(16.0))
        .opacity(0.72)
        .whitespace_nowrap()
        .truncate()
        .child(label)
        .into_any_element()
}

fn render_resource_pending_line(title: &'static str) -> AnyElement {
    div()
        .w_full()
        .min_w(px(0.0))
        .text_xs()
        .line_height(px(16.0))
        .opacity(0.72)
        .whitespace_nowrap()
        .truncate()
        .child(format!("{title} pending"))
        .into_any_element()
}

fn first_resident_resource_slice<'a>(
    snapshot: &'a ResidentTranscriptSnapshot,
    resource_id: &'a ResourceId,
) -> Option<&'a ResidentResourceSlice> {
    snapshot.resources.slices_for(resource_id).next()
}

fn resource_metadata_label(metadata: &ResourceMetadata) -> String {
    let mut parts = Vec::new();
    parts.push(format!("{} bytes", metadata.byte_len));
    if let Some(media_type) = &metadata.media_type {
        parts.push(media_type.clone());
    }
    if let Some(line_count) = metadata.line_count {
        parts.push(format!("{line_count} lines"));
    }
    if let Some(row_count) = metadata.row_count {
        parts.push(format!("{row_count} rows"));
    }
    if let Some(column_count) = metadata.column_count {
        parts.push(format!("{column_count} columns"));
    }
    parts.join(" | ")
}

fn resource_slice_label(slice: &ResidentResourceSlice) -> String {
    let complete_label = if slice.complete {
        "complete"
    } else {
        "partial"
    };
    format!(
        "Resident range {}..{} ({} bytes, {complete_label})",
        slice.range.start,
        slice.range.end,
        slice.bytes.len()
    )
}

fn resource_preview_text(slice: &ResidentResourceSlice) -> String {
    let preview_len = slice
        .bytes
        .len()
        .min(SYNDIC_TRANSCRIPT_RESOURCE_PREVIEW_TEXT_BYTES);
    let mut text = String::from_utf8_lossy(&slice.bytes[..preview_len]).into_owned();
    if slice.bytes.len() > preview_len || !slice.complete {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("...");
    }
    if text.is_empty() {
        "Resident range has no preview bytes".to_string()
    } else {
        text
    }
}

fn resource_kind_is_media(kind: &ResourceKind) -> bool {
    matches!(kind, ResourceKind::Image | ResourceKind::GeneratedImage)
}

fn resource_kind_label(kind: &ResourceKind) -> &str {
    match kind {
        ResourceKind::Code => "Code",
        ResourceKind::Table => "Table",
        ResourceKind::Image => "Image",
        ResourceKind::Attachment => "Attachment",
        ResourceKind::GeneratedImage => "Generated image",
        ResourceKind::Other(label) => label.as_str(),
    }
}

fn fallback_reason_label(reason: &LocalPresentationReason) -> &'static str {
    match reason {
        LocalPresentationReason::BudgetRejected => "Content unavailable within current budget",
        LocalPresentationReason::PolicyDenied => "Content unavailable",
        LocalPresentationReason::ResourceUnavailable => "Resource unavailable",
        LocalPresentationReason::PendingCoherentData => "Content pending",
        LocalPresentationReason::Unsupported => "Unsupported content",
    }
}

fn fallback_target_label(target: &ResidentFallbackTarget) -> String {
    match target {
        ResidentFallbackTarget::ProjectionRecord(projection_id) => projection_id.0.clone(),
        ResidentFallbackTarget::Resource(resource_id) => resource_id.0.clone(),
        ResidentFallbackTarget::ResourceRange { resource_id, range } => {
            format!("{}:{}..{}", resource_id.0, range.start, range.end)
        }
    }
}

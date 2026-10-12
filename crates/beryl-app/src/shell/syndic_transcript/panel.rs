use std::ops::Range;

#[path = "panel/code_appearance.rs"]
mod code_appearance;
#[path = "panel/code_layout.rs"]
mod code_layout;
#[path = "panel/code_layout_mount.rs"]
mod code_layout_mount;
#[path = "panel/code_panel.rs"]
mod code_panel;
#[path = "panel/code_viewport.rs"]
mod code_viewport;
#[path = "panel/context_appearance.rs"]
mod context_appearance;
#[path = "panel/copy_keyboard.rs"]
mod copy_keyboard;
mod inline_table_actions;
#[path = "panel/table_appearance.rs"]
mod table_appearance;
#[path = "panel/table_export.rs"]
mod table_export;
mod table_owner;
#[path = "panel/table_panel.rs"]
mod table_panel;
#[path = "panel/table_save.rs"]
mod table_save;
#[path = "panel/table_source_actions.rs"]
mod table_source_actions;
#[path = "panel/table_viewport.rs"]
mod table_viewport;

use gpui::{
    Action, AnyElement, App, Context, FocusHandle, Focusable, MouseButton, Render, SharedString,
    Window, canvas, div, prelude::*, px,
};

gpui::actions!(syndic_transcript, [OpenTranscriptContextMenu]);
pub(crate) struct ResourceViewportDemandChanged;
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
    TranscriptActivationOutcome, TranscriptActivationPlacement, TranscriptActivationSeed,
    TranscriptActivationSource, TranscriptCommandResult, realized_resident_selectable_record_ids,
    resident_context_menu_command_for_realized_record_id, resident_context_menu_frame_loss,
    resident_media_action_command_for_realized_record_id, resident_media_action_frame_loss,
    resident_quote_command_for_realized_record_ids, resident_quote_frame_loss,
    resident_selection_command_for_realized_record_ids, resident_selection_frame_loss,
};

pub(crate) const SYNDIC_TRANSCRIPT_KEY_CONTEXT: &str = "SyndicTranscriptPanel";
const SYNDIC_TRANSCRIPT_OVERSCAN_RATIO: f32 = 0.5;
const SYNDIC_TRANSCRIPT_DEFAULT_RECORD_HEIGHT_PX: f32 = 72.0;
const SYNDIC_TRANSCRIPT_RESOURCE_PREVIEW_BYTES: u64 = 4096;
const SYNDIC_TRANSCRIPT_RESOURCE_PREVIEW_TEXT_BYTES: usize = 2048;

struct StagedTranscript {
    activation: Option<PreparedTranscriptActivation>,
    host: SyndicTranscriptHost,
    outcome: TranscriptActivationOutcome,
    allocation: Option<gpui::Size<gpui::Pixels>>,
    unplaced_tail: bool,
    adjacent: Option<super::AdjacentTranscriptDemand>,
    resource: Option<super::ResourceTranscriptDemand>,
}

#[derive(Clone, PartialEq)]
struct MeasurementEnvironment {
    viewport: gpui::Size<gpui::Pixels>,
    window_viewport: gpui::Size<gpui::Pixels>,
    scale: f32,
    rem: gpui::Pixels,
    text: gpui::TextStyle,
}

struct PendingGeometry {
    revision: u64,
    environment: MeasurementEnvironment,
    delta: f32,
    preparation: super::frame::MeasuredFramePreparation,
    ready: Option<RealizedFrameWindow>,
    published: bool,
}

struct ResourceQueuePosition {
    activation_revision: u64,
    view_id: super::TranscriptViewId,
    record_id: ResidentPresentationRecordId,
}

pub(crate) struct SyndicTranscriptPanel {
    keyboard_copy: copy_keyboard::KeyboardCopy,
    focus_handle: FocusHandle,
    host: SyndicTranscriptHost,
    last_frame_window: Option<RealizedFrameWindow>,
    published_snapshot: Option<std::sync::Arc<ResidentTranscriptSnapshot>>,
    staged: Option<StagedTranscript>,
    geometry: Option<PendingGeometry>,
    pending_delta: f32,
    rejected_adjacent: Option<super::AdjacentTranscriptDemand>,
    rejected_resource: Vec<super::ResourceTranscriptDemand>,
    resource_queue_position: Option<ResourceQueuePosition>,
    appearance: Option<std::sync::Arc<crate::theme_runtime::AppearanceGeneration>>,
    code_appearance: code_appearance::CodeAppearance,
    context_appearance: context_appearance::ContextAppearance,
    code_panels: Vec<code_panel::MountedCodePanel>,
    table_panels: Vec<table_panel::MountedTablePanel>,
    retired_table_panels: Vec<table_panel::MountedTablePanel>,
    table_wheel_owner: std::rc::Rc<std::cell::Cell<Option<u64>>>,
    table_appearance: table_appearance::TableAppearance,
    save_admission: crate::main_window::WorkerAdmission,
    retired_code_panels: Vec<code_panel::MountedCodePanel>,
    code_release_registered: bool,
    code_wheel_owner: std::rc::Rc<std::cell::Cell<Option<u64>>>,
    #[cfg(test)]
    viewport_bounds: Option<gpui::Bounds<gpui::Pixels>>,
    #[cfg(test)]
    painted_glyphs: usize,
    #[cfg(test)]
    code_reads_paused_through_revision: Option<super::ProviderRevision>,
}

impl gpui::EventEmitter<ResourceViewportDemandChanged> for SyndicTranscriptPanel {}

impl SyndicTranscriptPanel {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            keyboard_copy: Default::default(),
            focus_handle: cx.focus_handle(),
            host: SyndicTranscriptHost::empty(),
            last_frame_window: None,
            published_snapshot: None,
            staged: None,
            geometry: None,
            pending_delta: 0.0,
            rejected_adjacent: None,
            rejected_resource: Vec::new(),
            resource_queue_position: None,
            appearance: None,
            code_appearance: code_appearance::CodeAppearance::default(),
            context_appearance: context_appearance::ContextAppearance::default(),
            code_panels: Vec::new(),
            table_panels: Vec::new(),
            retired_table_panels: Vec::new(),
            table_wheel_owner: std::rc::Rc::default(),
            table_appearance: Default::default(),
            save_admission: Default::default(),
            retired_code_panels: Vec::new(),
            code_release_registered: false,
            code_wheel_owner: std::rc::Rc::default(),
            #[cfg(test)]
            viewport_bounds: None,
            #[cfg(test)]
            painted_glyphs: 0,
            #[cfg(test)]
            code_reads_paused_through_revision: None,
        }
    }

    pub(crate) fn snapshot(&self) -> std::sync::Arc<ResidentTranscriptSnapshot> {
        self.host.shared_snapshot()
    }

    pub(crate) fn lifetime(&self) -> std::sync::Weak<()> {
        self.host.lifetime()
    }

    pub(crate) fn refresh_placement(&self) -> TranscriptActivationPlacement {
        self.host.refresh_placement()
    }

    pub(crate) fn publish_coherent_activation(
        &mut self,
        prepared: PreparedTranscriptActivation,
    ) -> TranscriptActivationOutcome {
        assert!(
            self.staged
                .as_ref()
                .is_some_and(|staged| staged.activation.as_ref() == Some(&prepared))
                && self.measurement_ready(),
            "attachment publication requires its measured generation"
        );
        self.commit_measured_activation()
    }

    pub(crate) fn stage_activation(
        &mut self,
        prepared: &PreparedTranscriptActivation,
        allocation: Option<gpui::Size<gpui::Pixels>>,
    ) {
        if self
            .staged
            .as_ref()
            .is_some_and(|staged| staged.activation.as_ref() == Some(prepared))
        {
            let staged = self.staged.as_mut().unwrap();
            if staged.allocation != allocation {
                staged.allocation = allocation;
                self.geometry = None;
            }
            return;
        }
        if !self.host.is_same_view(prepared) {
            self.pending_delta = 0.0;
        }
        let (host, outcome) = self.host.prepare_coherent_activation(prepared.clone());
        let unplaced_tail = host.scroll_state().anchor.is_none()
            && prepared.placement == TranscriptActivationPlacement::Tail;
        self.staged = Some(StagedTranscript {
            activation: Some(prepared.clone()),
            host,
            outcome,
            allocation,
            unplaced_tail,
            adjacent: None,
            resource: None,
        });
        self.geometry = None;
        self.close_invalid_geometry();
    }

    pub(crate) fn adjacent_demand(&self) -> Option<super::AdjacentTranscriptDemand> {
        if self.pending_delta != 0.0 {
            return None;
        }
        self.host
            .adjacent_demand()
            .filter(|demand| self.rejected_adjacent.as_ref() != Some(demand))
    }

    pub(crate) fn reject_adjacent_demand(&mut self, demand: super::AdjacentTranscriptDemand) {
        self.rejected_adjacent = Some(demand);
    }

    pub(crate) fn adjacent_candidate_current(
        &self,
        demand: &super::AdjacentTranscriptDemand,
    ) -> bool {
        self.adjacent_demand().as_ref() == Some(demand)
            && self
                .staged
                .as_ref()
                .is_some_and(|staged| staged.adjacent.as_ref() == Some(demand))
    }

    pub(crate) fn stage_adjacent_extension(
        &mut self,
        demand: &super::AdjacentTranscriptDemand,
        prepared: &PreparedTranscriptActivation,
    ) -> bool {
        if self.adjacent_demand().as_ref() != Some(demand) || self.staged.is_some() {
            return false;
        }
        let Some(host) = self.host.prepare_adjacent_extension(demand, prepared) else {
            return false;
        };
        let snapshot = host.shared_snapshot();
        let outcome = TranscriptActivationOutcome {
            activation_revision: snapshot.activation_revision,
            presentation_revision: snapshot.presentation_revision,
            state: snapshot.state.clone(),
            retained_previous_snapshot: true,
            provider_request: None,
        };
        self.staged = Some(StagedTranscript {
            activation: Some(prepared.clone()),
            host,
            outcome,
            allocation: None,
            unplaced_tail: false,
            adjacent: Some(demand.clone()),
            resource: None,
        });
        self.geometry = None;
        true
    }

    pub(crate) fn resource_demand(&self) -> Option<super::ResourceTranscriptDemand> {
        if self.pending_delta != 0.0 {
            return None;
        }
        let mut demands = self.host.resource_demands().into_iter().filter(|demand| {
            #[cfg(test)]
            if demand.kind == ResourceKind::Code
                && demand.indexed.is_some()
                && self
                    .code_reads_paused_through_revision
                    .is_some_and(|revision| demand.revision <= revision)
            {
                return false;
            }
            !self.rejected_resource.iter().any(|rejected| {
                rejected.activation_revision == demand.activation_revision
                    && rejected.view_id == demand.view_id
                    && rejected.revision == demand.revision
                    && rejected.record_id == demand.record_id
                    && rejected.provenance == demand.provenance
                    && rejected.metadata == demand.metadata
                    && rejected.range == demand.range
                    && rejected.indexed == demand.indexed
            })
        });
        let first = demands.next()?;
        let Some(position) = self.resource_queue_position.as_ref().filter(|position| {
            position.activation_revision == first.activation_revision
                && position.view_id == first.view_id
        }) else {
            return Some(first);
        };
        std::iter::once(first).chain(demands).min_by(|left, right| {
            let key = |demand: &super::ResourceTranscriptDemand| {
                demand.record_id.0 <= position.record_id.0
            };
            key(left)
                .cmp(&key(right))
                .then_with(|| left.record_id.0.cmp(&right.record_id.0))
        })
    }

    pub(crate) fn reject_resource_demand(&mut self, demand: super::ResourceTranscriptDemand) {
        self.reject_code_source_action(&demand);
        self.reject_table_source_action(&demand);
        if demand.kind == ResourceKind::Table
            && let Some(table) = self.table_panels.iter_mut().find(|table| {
                table
                    .viewport
                    .owner
                    .indexed()
                    .is_some_and(|owner| owner.resource_id == demand.resource_id)
            })
        {
            table.viewport.feedback = "Table range unavailable";
        }
        if self.resource_demand().as_ref() == Some(&demand) {
            self.advance_resource_queue(&demand);
        }
        self.prune_rejected_resources();
        self.rejected_resource.retain(|rejected| {
            rejected.activation_revision == demand.activation_revision
                && rejected.view_id == demand.view_id
                && rejected.record_id != demand.record_id
        });
        if self.rejected_resource.len() < 1024 {
            self.rejected_resource.push(demand);
        }
    }

    fn prune_rejected_resources(&mut self) {
        let current = self.host.resource_demands();
        self.rejected_resource.retain(|rejected| {
            current.iter().any(|demand| {
                rejected.activation_revision == demand.activation_revision
                    && rejected.view_id == demand.view_id
                    && rejected.revision == demand.revision
                    && rejected.record_id == demand.record_id
                    && rejected.provenance == demand.provenance
                    && rejected.metadata == demand.metadata
                    && rejected.range == demand.range
                    && rejected.indexed == demand.indexed
            })
        });
    }

    pub(crate) fn resource_candidate_current(
        &self,
        demand: &super::ResourceTranscriptDemand,
    ) -> bool {
        self.resource_demand().as_ref() == Some(demand)
            && self
                .staged
                .as_ref()
                .is_some_and(|staged| staged.resource.as_ref() == Some(demand))
    }

    pub(crate) fn stage_resource_response(
        &mut self,
        demand: &super::ResourceTranscriptDemand,
        response: super::TranscriptProviderResponseKind,
        allocation: Option<gpui::Size<gpui::Pixels>>,
    ) -> bool {
        self.stage_resource_delivery(
            demand,
            super::provider::ResourceDeliveryResponse::Raw(response),
            allocation,
        )
    }

    pub(crate) fn refresh_pending_indexed_resource(
        &mut self,
        demand: &super::ResourceTranscriptDemand,
    ) -> bool {
        if self.staged.is_some() {
            return false;
        }
        self.host.refresh_pending_indexed_resource(demand)
    }

    pub(crate) fn set_indexed_resource_demand(
        &mut self,
        demand: &super::ResourceTranscriptDemand,
        inner: Option<super::provider::IndexedResourceDemand>,
    ) -> bool {
        if self.staged.as_ref().is_some_and(|staged| {
            staged
                .resource
                .as_ref()
                .is_some_and(|current| current.resource_id == demand.resource_id)
        }) {
            self.discard_staged_activation();
        }
        for panel in &mut self.code_panels {
            if panel.viewport.owner.resource_id == demand.resource_id {
                code_panel::release_layout(&mut self.host, &panel.viewport.owner);
                if let Some(staged) = &mut self.staged {
                    code_panel::release_layout(&mut staged.host, &panel.viewport.owner);
                }
                panel.layout.reset();
                panel
                    .actions
                    .cancel_save("Save unavailable: source owner replaced");
                panel.overridden = true;
            }
        }
        self.host.set_indexed_resource_demand(demand, inner)
    }

    pub(crate) fn indexed_resource_response(
        &self,
        resource: &super::ResourceId,
    ) -> Option<std::sync::Arc<super::provider::IndexedResourceResponse>> {
        self.host.indexed_resource_response(resource)
    }

    pub(crate) fn stage_resource_delivery(
        &mut self,
        demand: &super::ResourceTranscriptDemand,
        response: super::provider::ResourceDeliveryResponse,
        allocation: Option<gpui::Size<gpui::Pixels>>,
    ) -> bool {
        if self.resource_demand().as_ref() != Some(demand) || self.staged.is_some() {
            return false;
        }
        let Some(host) = self.host.prepare_resource_delivery(demand, response) else {
            return false;
        };
        let snapshot = host.shared_snapshot();
        let outcome = TranscriptActivationOutcome {
            activation_revision: snapshot.activation_revision,
            presentation_revision: snapshot.presentation_revision,
            state: snapshot.state.clone(),
            retained_previous_snapshot: true,
            provider_request: None,
        };
        self.staged = Some(StagedTranscript {
            activation: None,
            host,
            outcome,
            allocation,
            unplaced_tail: false,
            adjacent: None,
            resource: Some(demand.clone()),
        });
        self.geometry = None;
        true
    }

    pub(crate) fn update_resource_allocation(
        &mut self,
        demand: &super::ResourceTranscriptDemand,
        allocation: gpui::Size<gpui::Pixels>,
    ) -> bool {
        if !self.resource_candidate_current(demand) {
            return false;
        }
        let staged = self.staged.as_mut().unwrap();
        if staged.allocation != Some(allocation) {
            staged.allocation = Some(allocation);
            self.geometry = None;
        }
        true
    }

    pub(crate) fn publish_resource_candidate(&mut self, demand: &super::ResourceTranscriptDemand) {
        assert!(
            self.resource_candidate_current(demand) && self.measurement_ready(),
            "resource publication requires its current measured generation"
        );
        self.commit_measured_activation();
        self.advance_resource_queue(demand);
    }

    fn advance_resource_queue(&mut self, demand: &super::ResourceTranscriptDemand) {
        self.resource_queue_position = Some(ResourceQueuePosition {
            activation_revision: demand.activation_revision,
            view_id: demand.view_id.clone(),
            record_id: demand.record_id.clone(),
        });
    }

    pub(crate) fn measurement_ready(&self) -> bool {
        self.staged.is_some()
            && self
                .geometry
                .as_ref()
                .is_some_and(|geometry| geometry.ready.is_some())
    }

    pub(crate) fn measurement_current(&self, window: &Window) -> bool {
        self.measurement_ready()
            && self.geometry.as_ref().is_some_and(|geometry| {
                geometry.environment.window_viewport == window.viewport_size()
                    && geometry.environment.scale == window.scale_factor()
                    && geometry.environment.rem == window.rem_size()
                    && geometry.environment.text == window.text_style()
            })
    }

    pub(crate) fn set_appearance(
        &mut self,
        appearance: std::sync::Arc<crate::theme_runtime::AppearanceGeneration>,
    ) {
        if self
            .appearance
            .as_ref()
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, &appearance))
        {
            return;
        }
        self.code_appearance =
            code_appearance::CodeAppearance::prepare(appearance.prepared().appearance());
        self.context_appearance =
            context_appearance::ContextAppearance::prepare(appearance.prepared().appearance());
        self.table_appearance =
            table_appearance::TableAppearance::prepare(appearance.prepared().appearance());
        self.appearance = Some(appearance);
        self.geometry = None;
        self.close_invalid_geometry();
    }

    pub(crate) fn measurement_diagnostics(&self) -> (bool, usize, usize) {
        self.geometry
            .as_ref()
            .map(|geometry| {
                (
                    !geometry.published,
                    geometry.preparation.retained_measurement_count()
                        + geometry
                            .ready
                            .as_ref()
                            .map(|frame| frame.records.len())
                            .unwrap_or(0),
                    geometry.preparation.work_last_step(),
                )
            })
            .unwrap_or((false, 0, 0))
    }

    #[cfg(test)]
    pub(crate) fn measured_frame_for_test(&self) -> Option<RealizedFrameWindow> {
        self.last_frame_window.clone()
    }

    #[cfg(test)]
    pub(crate) fn viewport_bounds_for_test(&self) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.viewport_bounds
    }

    #[cfg(test)]
    pub(crate) fn painted_glyphs_for_test(&self) -> usize {
        self.painted_glyphs
    }

    pub(crate) fn discard_staged_activation(&mut self) {
        if self.staged.as_ref().is_some_and(|staged| {
            staged
                .activation
                .as_ref()
                .is_some_and(|activation| !self.host.is_same_view(activation))
        }) {
            self.pending_delta = 0.0;
        }
        self.staged = None;
        self.geometry = None;
    }

    fn commit_measured_activation(&mut self) -> TranscriptActivationOutcome {
        let staged = self.staged.take().expect("measured attachment is staged");
        self.host = staged.host;
        self.publish_ready_geometry();
        staged.outcome
    }

    fn close_invalid_geometry(&mut self) {
        self.keyboard_copy.gesture.invalidate_records();
        self.keyboard_copy.feedback = "";
        self.host.clear_resident_selection();
        self.host.clear_resident_quote_target();
        self.host.clear_resident_context_menu_target();
        self.host.clear_resident_media_action_target();
    }

    fn queue_manual_delta(&mut self, delta: f32, cx: &mut Context<Self>) {
        if !delta.is_finite() || delta == 0.0 {
            return;
        }
        if self
            .staged
            .as_ref()
            .is_some_and(|staged| staged.adjacent.is_some() || staged.resource.is_some())
        {
            self.discard_staged_activation();
        }
        self.host.detach_tail();
        if let Some(staged) = &mut self.staged {
            staged.host.detach_tail();
        }
        self.pending_delta += delta;
        self.close_invalid_geometry();
        cx.notify();
    }

    fn publish_ready_geometry(&mut self) {
        let geometry = self.geometry.as_mut().expect("measured geometry exists");
        let frame = geometry.ready.take().expect("measured geometry is ready");
        self.pending_delta -= geometry.delta;
        geometry.delta = 0.0;
        geometry.published = true;
        self.host.accept_measured_frame(frame.clone());
        self.prune_rejected_resources();
        self.published_snapshot = Some(self.host.shared_snapshot());
        self.last_frame_window = Some(frame);
    }

    pub(crate) fn retire(&mut self) {
        self.reset_keyboard_copy();
        self.keyboard_copy = Default::default();
        for code in &mut self.code_panels {
            code.actions.cancel_copy("");
            code.actions.cancel_save("");
        }
        for table in &mut self.table_panels {
            table.actions.cancel("");
        }
        self.host.retire();
        self.last_frame_window = None;
        self.published_snapshot = None;
        self.staged = None;
        self.geometry = None;
        self.pending_delta = 0.0;
        self.rejected_adjacent = None;
        self.rejected_resource.clear();
        self.resource_queue_position = None;
        self.retired_code_panels.append(&mut self.code_panels);
        self.retired_table_panels.append(&mut self.table_panels);
        self.table_wheel_owner.set(None);
        self.code_wheel_owner.set(None);
    }

    pub(crate) fn status_facts(&self) -> ResidentTranscriptStatusFacts {
        self.host.status_facts()
    }

    pub(crate) fn diagnostic_snapshot(&self) -> SyndicTranscriptDiagnosticSnapshot {
        let mut snapshot = self.host.diagnostic_snapshot();
        let (pending, retained, work) = self.measurement_diagnostics();
        snapshot.frame.measurement_preparation_pending = pending;
        snapshot.frame.retained_preparation_measurements = retained;
        snapshot.frame.layout_observations_last_frame = work;
        snapshot.code_panels = self.code_diagnostics();
        snapshot.table_panels =
            self.table_panels
                .iter()
                .map(|table| {
                    let scroll = table.viewport.scroll.borrow();
                    let (rows, columns) = table.viewport.realized_ranges();
                    let first_row = (scroll.vertical / scroll.row_height).floor() as u64;
                    let first_column = (scroll.horizontal / scroll.column_width).floor() as u64;
                    super::diagnostics::TablePanelDiagnostics {
                        id: table.id,
                        revision: table.viewport.revision.get(),
                        rows: scroll.rows,
                        columns: scroll.columns,
                        visible_rows: first_row
                            ..(first_row
                                + (f32::from(scroll.bounds.size.height) / scroll.row_height).ceil()
                                    as u64)
                                .min(scroll.rows),
                        realized_rows: rows.start..rows.end,
                        visible_columns: first_column
                            ..(first_column
                                + (f32::from(scroll.bounds.size.width) / scroll.column_width).ceil()
                                    as u64)
                                .min(scroll.columns),
                        realized_columns: columns.start..columns.end,
                        row_overscan: table_viewport::ROW_OVERSCAN,
                        column_overscan: table_viewport::COLUMN_OVERSCAN,
                        cells: table.viewport.paint_cells().len(),
                        fallback_rows: table.viewport.fallback_rows().len(),
                        pending: self.host.resource_demands().iter().any(|owner| {
                            table.viewport.owner.indexed().is_some_and(|table_owner| {
                                owner.resource_id == table_owner.resource_id
                            })
                        }),
                        offsets: (scroll.horizontal, scroll.vertical),
                        selected_for_wheel: self.table_wheel_owner.get() == Some(table.id),
                        focused: table
                            .viewport
                            .focus
                            .map(|cell| (cell.ordinal(), cell.column)),
                        selection: table.viewport.selection.as_ref().map(|selection| {
                            (
                                (selection.anchor.ordinal(), selection.anchor.column),
                                (selection.head.ordinal(), selection.head.column),
                            )
                        }),
                        vertical_scrollbar: scroll.extent(gpui_scrollbar::Axis::Vertical) > 0.0,
                        horizontal_scrollbar: scroll.extent(gpui_scrollbar::Axis::Horizontal) > 0.0,
                    }
                })
                .collect();
        snapshot
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
        cx: &mut Context<Self>,
    ) {
        self.queue_manual_delta(command.frame_request().manual_delta_px, cx);
    }

    pub(crate) fn manual_scroll_delta(
        &mut self,
        viewport_height_px: f32,
        delta_px: f32,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.host.shared_snapshot();
        self.manual_scroll(
            ManualTranscriptScrollCommand::new(
                viewport_height_px,
                viewport_height_px * SYNDIC_TRANSCRIPT_OVERSCAN_RATIO,
                SYNDIC_TRANSCRIPT_DEFAULT_RECORD_HEIGHT_PX,
                delta_px,
                Some(snapshot.presentation_revision),
            ),
            cx,
        );
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
        self.reset_keyboard_copy();
        if self.staged.is_some()
            || self
                .geometry
                .as_ref()
                .is_none_or(|geometry| !geometry.published)
        {
            return ResidentSelectionOutcome::Unavailable(
                ResidentSelectionUnavailable::NoRealizedFrame,
            );
        }
        let Some(frame_window) = self.last_frame_window.as_ref() else {
            let _ = self.host.clear_resident_selection();
            return ResidentSelectionOutcome::Unavailable(
                ResidentSelectionUnavailable::NoRealizedFrame,
            );
        };
        let snapshot = self.host.shared_snapshot();
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
        if self.staged.is_some()
            || self
                .geometry
                .as_ref()
                .is_none_or(|geometry| !geometry.published)
        {
            return ResidentQuoteOutcome::Unavailable(
                ResidentSelectionUnavailable::NoRealizedFrame,
            );
        }
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
        let snapshot = self.host.shared_snapshot();
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
        if self.staged.is_some()
            || self
                .geometry
                .as_ref()
                .is_none_or(|geometry| !geometry.published)
        {
            return ResidentContextMenuOutcome::Unavailable(
                ResidentContextMenuUnavailable::NoRealizedFrame,
            );
        }
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
        let snapshot = self.host.shared_snapshot();
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
        if self.staged.is_some()
            || self
                .geometry
                .as_ref()
                .is_none_or(|geometry| !geometry.published)
        {
            return ResidentMediaActionOutcome::Unavailable(
                ResidentMediaActionUnavailable::NoRealizedFrame,
            );
        }
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
        let snapshot = self.host.shared_snapshot();
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
        let snapshot = self.host.shared_snapshot();
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = cx.entity();
        #[cfg(test)]
        let paint_panel = panel.clone();
        canvas(
            move |bounds, window, cx| {
                if bounds.size.width <= px(0.0) || bounds.size.height <= px(0.0) {
                    return None;
                }
                let mut content = panel.update(cx, |panel, cx| {
                    #[cfg(test)]
                    {
                        panel.viewport_bounds = Some(bounds);
                    }
                    panel
                        .render_measured(bounds.size, window, cx)
                        .into_any_element()
                });
                content.layout_as_root(bounds.size.map(gpui::AvailableSpace::Definite), window, cx);
                content.prepaint_at(bounds.origin, window, cx);
                Some(content)
            },
            move |_, content, window, cx| {
                if let Some(mut content) = content {
                    #[cfg(test)]
                    let before = {
                        let paint = window.paint_snapshot_for_test();
                        paint.glyphs.len() + paint.color_glyphs.len()
                    };
                    content.paint(window, cx);
                    #[cfg(test)]
                    {
                        let paint = window.paint_snapshot_for_test();
                        let count = paint.glyphs.len() + paint.color_glyphs.len() - before;
                        paint_panel.update(cx, |panel, _| panel.painted_glyphs = count);
                    }
                }
            },
        )
        .size_full()
    }
}

impl SyndicTranscriptPanel {
    fn render_measured(
        &mut self,
        viewport_size: gpui::Size<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.observe_copy_focus(window, cx);
        if !self.code_release_registered {
            cx.on_release_in(window, |panel, window, app| {
                for code in panel
                    .code_panels
                    .drain(..)
                    .chain(panel.retired_code_panels.drain(..))
                {
                    code.retire(window, app);
                }
                for mut table in panel
                    .table_panels
                    .drain(..)
                    .chain(panel.retired_table_panels.drain(..))
                {
                    table.retire(window, app);
                }
            })
            .detach();
            self.code_release_registered = true;
        }
        for code in self.retired_code_panels.drain(..) {
            code.retire(window, cx);
        }
        let viewport_width_px = f32::from(viewport_size.width);
        let measured_size = self
            .staged
            .as_ref()
            .and_then(|staged| staged.allocation)
            .unwrap_or(viewport_size);
        let viewport_height_px = f32::from(measured_size.height);
        let target = self
            .staged
            .as_ref()
            .map(|staged| &staged.host)
            .unwrap_or(&self.host);
        let snapshot = target.shared_snapshot();
        let environment = MeasurementEnvironment {
            viewport: measured_size,
            window_viewport: window.viewport_size(),
            scale: window.scale_factor(),
            rem: window.rem_size(),
            text: window.text_style(),
        };
        let changed = self.geometry.as_ref().is_none_or(|geometry| {
            geometry.revision != snapshot.presentation_revision
                || geometry.environment != environment
        });
        let settled = self
            .geometry
            .as_ref()
            .is_some_and(|geometry| geometry.published)
            && self.last_frame_window.is_some()
            && !changed;
        if changed || (settled && self.pending_delta != 0.0) {
            let mut state = target.scroll_state();
            if self
                .staged
                .as_ref()
                .is_some_and(|staged| staged.unplaced_tail)
            {
                state.scroll_mode = super::RealizedFrameScrollMode::LiveTailFollowing;
            }
            self.close_invalid_geometry();
            self.geometry = Some(PendingGeometry {
                revision: snapshot.presentation_revision,
                environment,
                delta: self.pending_delta,
                preparation: super::frame::MeasuredFramePreparation::new(
                    snapshot.clone(),
                    state,
                    RealizedFrameRequest {
                        viewport_height_px,
                        overscan_height_px: viewport_height_px * SYNDIC_TRANSCRIPT_OVERSCAN_RATIO,
                        default_record_height_px: SYNDIC_TRANSCRIPT_DEFAULT_RECORD_HEIGHT_PX,
                        manual_delta_px: self.pending_delta,
                        observed_presentation_revision: Some(snapshot.presentation_revision),
                    },
                ),
                ready: None,
                published: false,
            });
        }
        let geometry = self.geometry.as_mut().unwrap();
        if geometry.ready.is_none() && !geometry.published {
            let table_row_height = self.table_appearance.row_height(cx).unwrap_or(28.0);
            geometry.ready = geometry.preparation.advance(32, |record| {
                let mut element = div()
                    .w(viewport_size.width)
                    .px_4()
                    .py(px(
                        if matches!(
                            record.provenance.source,
                            super::ResidentRecordSource::LocalContext { .. }
                        ) {
                            0.0
                        } else {
                            8.0
                        },
                    ))
                    .child(render_record_content(
                        &snapshot,
                        record,
                        self.code_appearance.header_height(),
                        &self.context_appearance,
                        &self.table_appearance,
                        table_row_height,
                    ))
                    .into_any_element();
                f32::from(
                    element
                        .layout_as_root(
                            gpui::size(
                                gpui::AvailableSpace::Definite(viewport_size.width),
                                gpui::AvailableSpace::MinContent,
                            ),
                            window,
                            cx,
                        )
                        .height,
                )
            });
        }
        if geometry.ready.is_some() && self.staged.is_none() {
            self.publish_ready_geometry();
        }
        if self.staged.is_some()
            || self.pending_delta != 0.0
            || self.last_frame_window.is_none()
            || self
                .geometry
                .as_ref()
                .is_some_and(|geometry| !geometry.published)
        {
            window.request_animation_frame();
        }
        let snapshot = self
            .published_snapshot
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(ResidentTranscriptSnapshot::empty()));
        let Some(frame_window) = self.last_frame_window.clone() else {
            window.request_animation_frame();
            return div().size_full();
        };

        self.host.push_demand_fact(DemandFact::new(
            snapshot.presentation_revision,
            DemandFactKind::Viewport {
                width_px: viewport_width_px,
                height_px: viewport_height_px,
            },
        ));

        report_nested_resource_demands(&snapshot, &frame_window, &mut self.host);
        self.reconcile_code_panels(&snapshot, &frame_window, window, cx);
        self.reconcile_table_panels(&snapshot, &frame_window, window, cx);
        self.reconcile_resident_selection_with_frame(&snapshot, &frame_window);
        self.reconcile_resident_quote_target_with_frame(&snapshot, &frame_window);
        self.reconcile_resident_context_menu_target_with_frame(&snapshot, &frame_window);
        self.reconcile_resident_media_action_target_with_frame(&snapshot, &frame_window);
        let selected_record_ids =
            render_selected_record_ids(&snapshot, &frame_window, self.host.resident_selection());
        let source_message = if snapshot.records.is_empty() {
            match &snapshot.state {
                super::ResidentTranscriptSnapshotState::Unavailable { reason } => {
                    Some(reason.clone())
                }
                super::ResidentTranscriptSnapshotState::Incomplete { detail, .. } => Some(
                    detail
                        .clone()
                        .unwrap_or_else(|| "Earlier messages are unavailable.".to_owned()),
                ),
                _ => None,
            }
        } else {
            None
        };
        self.last_frame_window = Some(frame_window.clone());

        div()
            .relative()
            .w(viewport_size.width)
            .h(viewport_size.height)
            .min_h(px(0.0))
            .overflow_hidden()
            .key_context(SYNDIC_TRANSCRIPT_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|panel, _, window, cx| {
                    panel.code_wheel_owner.set(None);
                    panel.table_wheel_owner.set(None);
                    window.focus(&panel.focus_handle);
                    cx.notify();
                }),
            )
            .on_scroll_wheel(
                cx.listener(|panel, event: &gpui::ScrollWheelEvent, window, cx| {
                    let delta = event.delta.pixel_delta(window.line_height());
                    panel.queue_manual_delta(-f32::from(delta.y), cx);
                    cx.stop_propagation();
                }),
            )
            .on_key_down(
                cx.listener(|panel, event: &gpui::KeyDownEvent, window, cx| {
                    if copy_keyboard::is_copy(event) {
                        panel.copy_records_key(event, window, cx);
                        cx.stop_propagation();
                        return;
                    }
                    let height = panel
                        .geometry
                        .as_ref()
                        .map(|geometry| f32::from(geometry.environment.viewport.height))
                        .unwrap_or(0.0);
                    let delta = match event.keystroke.key.as_str() {
                        "up" => -40.0,
                        "down" => 40.0,
                        "pageup" => -height,
                        "pagedown" => height,
                        _ => return,
                    };
                    panel.queue_manual_delta(delta, cx);
                    cx.stop_propagation();
                }),
            )
            .children(
                source_message
                    .map(|message| div().px_4().py_2().text_sm().opacity(0.72).child(message)),
            )
            .child(render_frame_window(
                &snapshot,
                &frame_window,
                &selected_record_ids,
                &self.code_panels,
                &self.table_panels,
                &self.host,
                &self.code_wheel_owner,
                &self.code_appearance,
                &self.context_appearance,
                &self.table_appearance,
                self.staged.is_some(),
                cx,
            ))
            .children(
                self.keyboard_copy
                    .feedback
                    .starts_with("Copy unavailable")
                    .then(|| {
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .px_4()
                            .py_2()
                            .text_sm()
                            .child(self.keyboard_copy.feedback)
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/syndic_transcript_panel.rs"
    ));
}

fn render_frame_window(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    selected_record_ids: &[ResidentPresentationRecordId],
    code_panels: &[code_panel::MountedCodePanel],
    table_panels: &[table_panel::MountedTablePanel],
    host: &SyndicTranscriptHost,
    wheel_owner: &std::rc::Rc<std::cell::Cell<Option<u64>>>,
    code_appearance: &code_appearance::CodeAppearance,
    context_appearance: &context_appearance::ContextAppearance,
    table_appearance: &table_appearance::TableAppearance,
    commands_pending: bool,
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
            code_panels,
            table_panels,
            host,
            wheel_owner,
            code_appearance,
            context_appearance,
            table_appearance,
            commands_pending,
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
    code_panels: &[code_panel::MountedCodePanel],
    table_panels: &[table_panel::MountedTablePanel],
    host: &SyndicTranscriptHost,
    wheel_owner: &std::rc::Rc<std::cell::Cell<Option<u64>>>,
    code_appearance: &code_appearance::CodeAppearance,
    context_appearance: &context_appearance::ContextAppearance,
    table_appearance: &table_appearance::TableAppearance,
    commands_pending: bool,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> AnyElement {
    let context_record_id = record.id.clone();
    div()
        .debug_selector(|| format!("syndic-transcript-record:{}", record.id.0))
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
        .py(px(
            if matches!(
                record.provenance.source,
                super::ResidentRecordSource::LocalContext { .. }
            ) {
                0.0
            } else {
                8.0
            },
        ))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |panel, _, window, cx| {
                let _ = panel.apply_realized_context_menu_target(&context_record_id, cx);
                window.dispatch_action(OpenTranscriptContextMenu.boxed_clone(), cx);
                cx.stop_propagation();
            }),
        )
        .child(
            if let Some(code) = code_panels
                .iter()
                .find(|code| code.viewport.owner.record_id == record.id)
            {
                code_panel::render(
                    code,
                    host,
                    wheel_owner,
                    code_appearance,
                    commands_pending,
                    cx,
                )
            } else if let Some(table) = table_panels
                .iter()
                .find(|table| table.viewport.owner.record_id() == &record.id)
            {
                table_panel::render(
                    table,
                    table_appearance,
                    code_appearance,
                    commands_pending,
                    cx,
                )
            } else {
                render_record_content(
                    snapshot,
                    record,
                    code_appearance.header_height(),
                    context_appearance,
                    table_appearance,
                    table_appearance.row_height(cx).unwrap_or(28.0),
                )
            },
        )
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
    code_header_height: f32,
    appearance: &context_appearance::ContextAppearance,
    table_appearance: &table_appearance::TableAppearance,
    table_row_height: f32,
) -> AnyElement {
    if let super::ResidentRecordSource::LocalContext { group, chunk_index } =
        &record.provenance.source
    {
        let mut callout = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0 * appearance.scale()))
            .px(px(14.0 * appearance.scale()))
            .border_l_1()
            .border_r_1()
            .border_color(appearance.border)
            .bg(appearance.background)
            .text_color(appearance.foreground);
        if *chunk_index == 0 {
            callout = callout
                .pt(px(12.0 * appearance.scale()))
                .border_t_1()
                .rounded_t(px(8.0 * appearance.scale()))
                .child(
                    div()
                        .flex()
                        .gap(px(0.8 * appearance.heading.size / 10.0))
                        .text_size(px(appearance.heading.size))
                        .font(appearance.heading.font.clone())
                        .text_color(appearance.heading.color)
                        .children(
                            "DISCUSSING"
                                .chars()
                                .map(|glyph| div().child(glyph.to_string())),
                        ),
                );
        } else {
            callout = callout.pt(px(8.0 * appearance.scale()));
        }
        let last = *chunk_index + 1 == group.chunk_count;
        if last {
            callout = callout
                .pb(px(12.0 * appearance.scale()))
                .border_b_1()
                .rounded_b(px(8.0 * appearance.scale()));
        }
        if matches!(
            record.kind,
            ResidentPresentationRecordKind::ContextUnavailable
        ) {
            callout = callout.opacity(0.62);
        }
        callout = callout.child(match &record.kind {
            ResidentPresentationRecordKind::TextChunk { text, .. } => div()
                .text_size(px(appearance.body.size))
                .font(appearance.body.font.clone())
                .text_color(appearance.body.color)
                .line_height(px(20.0 * appearance.scale()))
                .whitespace_normal()
                .child(SharedString::from(text.clone())),
            _ => div()
                .text_size(px(appearance.body.size))
                .font(appearance.body.font.clone())
                .text_color(appearance.body.color)
                .child("Discussion context unavailable"),
        });
        if last && group.descriptor.is_some() {
            callout = callout.child(
                div()
                    .text_size(px(appearance.provenance.size))
                    .font(appearance.provenance.font.clone())
                    .text_color(appearance.provenance.color)
                    .whitespace_nowrap()
                    .truncate()
                    .child("Selected assistant passage · Parent conversation"),
            );
        }
        return callout.into_any_element();
    }
    match &record.kind {
        ResidentPresentationRecordKind::InlineTable { .. } => {
            table_panel::shell(table_appearance, table_row_height)
                .child("Table loading")
                .into_any_element()
        }
        ResidentPresentationRecordKind::InlineTableUnavailable { .. } => {
            table_panel::shell(table_appearance, table_row_height)
                .child("Table structure unavailable")
                .into_any_element()
        }
        ResidentPresentationRecordKind::TextChunk { text, .. } => div()
            .w_full()
            .min_w(px(0.0))
            .text_sm()
            .line_height(px(20.0))
            .whitespace_normal()
            .child(SharedString::from(text.clone()))
            .into_any_element(),
        ResidentPresentationRecordKind::ResourceReference {
            resource_id,
            resource_kind,
            label,
        } => {
            if *resource_kind == ResourceKind::Code {
                code_viewport::shell(code_header_height)
                    .child("Code loading")
                    .into_any_element()
            } else if *resource_kind == ResourceKind::Table {
                table_panel::shell(table_appearance, table_row_height)
                    .child("Table loading")
                    .into_any_element()
            } else {
                render_resource_reference(
                    snapshot,
                    resource_id,
                    resource_kind,
                    label,
                    code_header_height,
                )
            }
        }
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
        ResidentPresentationRecordKind::ContextUnavailable => div()
            .child("Discussion context unavailable")
            .into_any_element(),
    }
}

fn report_nested_resource_demands(
    snapshot: &ResidentTranscriptSnapshot,
    frame_window: &RealizedFrameWindow,
    host: &mut SyndicTranscriptHost,
) {
    let mut reported_ranges: Vec<(ResourceId, Range<u64>)> = Vec::new();

    for frame_record in &frame_window.records {
        let Some(record) = snapshot.records.get(frame_record.index) else {
            continue;
        };
        if record.id != frame_record.record_id {
            continue;
        }

        let ResidentPresentationRecordKind::ResourceReference { resource_id, .. } = &record.kind
        else {
            continue;
        };

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
    let metadata = snapshot.resources.metadata_for(resource_id)?;
    let range = metadata_preview_range(metadata)?;

    if range.start >= range.end || resource_slices_cover_range(snapshot, resource_id, &range) {
        return None;
    }

    Some(range)
}

fn metadata_preview_range(metadata: &ResourceMetadata) -> Option<Range<u64>> {
    if let Some(range) = metadata.preview_range.clone() {
        if range.end > metadata.byte_len {
            return None;
        }
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
    snapshot.resources.slices_for(resource_id).any(|slice| {
        snapshot
            .resources
            .metadata_for(resource_id)
            .is_some_and(|metadata| metadata.revision == slice.revision)
            && slice.range.start <= range.start
            && slice.range.end >= range.end
    })
}

fn render_resource_reference(
    snapshot: &ResidentTranscriptSnapshot,
    resource_id: &ResourceId,
    resource_kind: &ResourceKind,
    label: &Option<std::sync::Arc<str>>,
    code_header_height: f32,
) -> AnyElement {
    let metadata = snapshot.resources.metadata_for(resource_id);
    let title = label
        .clone()
        .unwrap_or_else(|| resource_id.0.clone().into());
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
                .child(gpui::SharedString::from(title)),
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
        .child(render_resource_shell(
            snapshot,
            resource_id,
            resource_kind,
            code_header_height,
        ))
        .into_any_element()
}

fn render_resource_shell(
    snapshot: &ResidentTranscriptSnapshot,
    resource_id: &ResourceId,
    resource_kind: &ResourceKind,
    code_header_height: f32,
) -> AnyElement {
    match resource_kind {
        ResourceKind::Code => code_viewport::shell(code_header_height)
            .child("Code loading")
            .into_any_element(),
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
                .font_family("Consolas")
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

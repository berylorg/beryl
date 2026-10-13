use std::ops::Range;

use super::{
    DemandFact, DemandFactSinkSnapshot, DisabledTranscriptCommand, ManualTranscriptScrollCommand,
    PreparedTranscriptActivation, RealizedFrameAnchor, RealizedFrameClamp, RealizedFrameRequest,
    RealizedFrameScrollController, RealizedFrameScrollStateSnapshot, RealizedFrameWindow,
    ResidentContextMenuCommand, ResidentContextMenuCommandTarget, ResidentContextMenuOutcome,
    ResidentMediaActionCommand, ResidentMediaActionOutcome, ResidentMediaActionUnavailable,
    ResidentMediaCopyCommandTarget, ResidentMediaPreviewCommandTarget,
    ResidentMediaSaveCommandTarget, ResidentProviderResponseEffect, ResidentQuoteCommand,
    ResidentQuoteOutcome, ResidentSelectionCommand, ResidentSelectionOutcome,
    ResidentSelectionUnavailable, ResidentTranscriptContextMenuTarget,
    ResidentTranscriptCopyPayload, ResidentTranscriptCore, ResidentTranscriptMediaActionTarget,
    ResidentTranscriptMediaPayload, ResidentTranscriptQuotePayload, ResidentTranscriptQuoteTarget,
    ResidentTranscriptSelection, ResidentTranscriptSnapshot, ResidentTranscriptStatusFacts,
    SyndicTranscriptDiagnosticSnapshot, TranscriptActivationOutcome, TranscriptActivationPlacement,
    TranscriptActivationSeed, TranscriptActivationSource, TranscriptCommandResult,
    TranscriptProviderResponse,
};

#[derive(Debug)]
pub(crate) struct SyndicTranscriptHost {
    lifetime: std::sync::Arc<()>,
    core: ResidentTranscriptCore,
    scroll_controller: RealizedFrameScrollController,
    last_frame_window: Option<RealizedFrameWindow>,
    measured_presentation_revision: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AdjacentTranscriptDemand {
    pub(crate) activation_revision: u64,
    pub(crate) presentation_revision: u64,
    pub(crate) view_id: super::TranscriptViewId,
    pub(crate) revision: super::ProviderRevision,
    pub(crate) range: Range<u64>,
    pub(crate) direction: super::TranscriptPageDirection,
    pub(crate) retained_records: Vec<super::ResidentPresentationRecordId>,
}

impl Default for SyndicTranscriptHost {
    fn default() -> Self {
        Self::empty()
    }
}

impl SyndicTranscriptHost {
    #[cfg(test)]
    pub(crate) fn provider_revision_for_test(&self) -> Option<super::ProviderRevision> {
        self.core.core_snapshot().resident.provider_revision
    }

    #[cfg(test)]
    pub(crate) fn narrative_kind_for_test(
        &self,
        id: &super::ResidentPresentationRecordId,
    ) -> Option<super::TranscriptNarrativeKind> {
        let snapshot = self.core.core_snapshot();
        let projection = snapshot
            .presentation
            .records
            .iter()
            .find_map(|record| record.authored_part(id))?
            .provenance
            .projection_id
            .as_ref()?;
        snapshot
            .resident
            .view_records
            .iter()
            .find(|record| &record.projection_id == projection)
            .map(|record| record.narrative_kind.clone())
    }
    pub(crate) fn narrative_demand(&self) -> Option<super::NarrativeTranscriptDemand> {
        self.core.narrative_demand()
    }

    pub(crate) fn narrative_demand_current(
        &self,
        demand: &super::NarrativeTranscriptDemand,
    ) -> bool {
        self.core.narrative_demand_current(demand)
    }

    pub(crate) fn reserve_narrative_preparation(
        &mut self,
        demand: &super::NarrativeTranscriptDemand,
    ) -> Option<usize> {
        self.core.reserve_narrative_preparation(demand)
    }

    pub(crate) fn finish_narrative_preparation(
        &mut self,
        demand: &super::NarrativeTranscriptDemand,
    ) {
        self.core.finish_narrative_preparation(demand);
    }

    pub(crate) fn cancel_narrative_preparations(&mut self) {
        self.core.cancel_narrative_preparations();
    }

    pub(crate) fn prepare_narrative_delivery(
        &self,
        demand: &super::NarrativeTranscriptDemand,
        response: super::NarrativePreparationResponse,
    ) -> Option<Self> {
        Some(Self {
            lifetime: self.lifetime.clone(),
            core: self.core.prepare_narrative_delivery(demand, response)?,
            scroll_controller: self.scroll_controller.clone(),
            last_frame_window: self.last_frame_window.clone(),
            measured_presentation_revision: self.measured_presentation_revision,
        })
    }

    pub(crate) fn narrative_current(&self, owner: &super::NarrativeOwner) -> bool {
        self.core.narrative_current(owner)
    }

    pub(crate) fn inline_parts_current(&self, record: &super::ResidentPresentationRecord) -> bool {
        self.core.inline_parts_current(record)
    }

    pub(crate) fn inline_parts_layout_allowance(
        &self,
        record: &super::ResidentPresentationRecord,
    ) -> usize {
        self.core.inline_parts_layout_allowance(record)
    }

    pub(crate) fn reserve_inline_parts_layout(
        &mut self,
        record: &super::ResidentPresentationRecord,
        bytes: usize,
    ) -> bool {
        self.core.reserve_inline_parts_layout(record, bytes)
    }

    pub(crate) fn narrative_layout_allowance(&self, owner: &super::NarrativeOwner) -> usize {
        self.core.narrative_layout_allowance(owner)
    }

    pub(crate) fn literal_narrative_current(&self, owner: &super::LiteralNarrativeOwner) -> bool {
        self.core.literal_narrative_current(owner)
    }

    pub(crate) fn literal_narrative_layout_allowance(
        &self,
        owner: &super::LiteralNarrativeOwner,
    ) -> usize {
        self.core.literal_narrative_layout_allowance(owner)
    }

    pub(crate) fn reserve_literal_narrative_layout(
        &mut self,
        owner: &super::LiteralNarrativeOwner,
        bytes: usize,
    ) -> bool {
        self.core.reserve_literal_narrative_layout(owner, bytes)
    }

    pub(crate) fn release_previous_narrative_layout(&mut self) {
        self.core.release_previous_narrative_layout();
    }

    pub(crate) fn hold_previous_narrative_layout(&mut self, bytes: usize) {
        self.core.hold_previous_narrative_layout(bytes);
    }

    pub(crate) fn reserve_narrative_layout(
        &mut self,
        owner: &super::NarrativeOwner,
        bytes: usize,
    ) -> bool {
        self.core.reserve_narrative_layout(owner, bytes)
    }
    pub(crate) fn empty() -> Self {
        Self {
            lifetime: std::sync::Arc::new(()),
            core: ResidentTranscriptCore::empty(),
            scroll_controller: RealizedFrameScrollController::new(),
            last_frame_window: None,
            measured_presentation_revision: None,
        }
    }

    pub(crate) fn snapshot(&self) -> ResidentTranscriptSnapshot {
        self.core.presentation_snapshot()
    }

    pub(crate) fn shared_snapshot(&self) -> std::sync::Arc<ResidentTranscriptSnapshot> {
        self.core.shared_presentation_snapshot()
    }

    pub(crate) fn lifetime(&self) -> std::sync::Weak<()> {
        std::sync::Arc::downgrade(&self.lifetime)
    }

    pub(crate) fn adjacent_demand(&self) -> Option<AdjacentTranscriptDemand> {
        let frame = self.last_frame_window.as_ref()?;
        let direction = frame
            .demand_facts
            .iter()
            .rev()
            .find_map(|fact| match fact.kind {
                super::DemandFactKind::AdjacentRange { direction, .. } => Some(direction),
                _ => None,
            })?;
        let state = self.core.core_snapshot();
        let resident = &state.resident;
        let first = resident.view_records.first()?.position.0;
        let end = resident.view_records.last()?.position.0.checked_add(1)?;
        let limit = state.policy.view_page_limit.min(64) as u64;
        let range = match direction {
            super::TranscriptPageDirection::Backward if !resident.at_start && first > 1 => {
                first.saturating_sub(limit).max(1)..first
            }
            super::TranscriptPageDirection::Forward if !resident.at_end => {
                end..end.checked_add(limit)?
            }
            _ => return None,
        };
        Some(AdjacentTranscriptDemand {
            activation_revision: state.presentation.activation_revision,
            presentation_revision: state.presentation.presentation_revision,
            view_id: resident.view_id.clone()?,
            revision: resident.provider_revision?,
            range,
            direction,
            retained_records: self.core.protected_presentation_record_ids(),
        })
    }

    pub(crate) fn resource_demand(&self) -> Option<super::ResourceTranscriptDemand> {
        self.last_frame_window.as_ref()?;
        self.core.resource_demand()
    }

    pub(crate) fn resource_demands(&self) -> Vec<super::ResourceTranscriptDemand> {
        if self.last_frame_window.is_none() {
            return Vec::new();
        }
        self.core.resource_demands()
    }

    pub(crate) fn code_panel_demands(&self) -> Vec<super::ResourceTranscriptDemand> {
        if self.last_frame_window.is_none() {
            return Vec::new();
        }
        self.core.code_panel_demands()
    }

    pub(crate) fn table_panel_demands(&self) -> Vec<super::ResourceTranscriptDemand> {
        if self.last_frame_window.is_none() {
            return Vec::new();
        }
        self.core.table_panel_demands()
    }

    pub(crate) fn reserve_indexed_layout(
        &mut self,
        demand: &super::ResourceTranscriptDemand,
        bytes: usize,
    ) -> bool {
        self.core.reserve_indexed_layout(demand, bytes)
    }

    pub(crate) fn inline_table_current(&self, owner: &super::core::InlineTableOwner) -> bool {
        self.core.inline_table_current(owner)
    }

    pub(crate) fn reserve_inline_table_layout(
        &mut self,
        owner: &super::core::InlineTableOwner,
        bytes: usize,
    ) -> bool {
        self.core.reserve_inline_table_layout(owner, bytes)
    }

    pub(crate) fn can_reserve_indexed_layout(
        &self,
        demand: &super::ResourceTranscriptDemand,
        bytes: usize,
    ) -> bool {
        self.core.can_reserve_indexed_layout(demand, bytes)
    }

    pub(crate) fn indexed_table_window(
        &self,
        resource: &super::ResourceId,
    ) -> Vec<std::sync::Arc<super::provider::IndexedResourceResponse>> {
        self.core.indexed_table_window(resource)
    }

    #[cfg(test)]
    pub(crate) fn indexed_table_state_for_test(&self, resource: &super::ResourceId) -> String {
        self.core.indexed_table_state_for_test(resource)
    }

    pub(crate) fn acknowledge_table_window(
        &mut self,
        owner: &super::ResourceTranscriptDemand,
        pages: &[std::sync::Arc<super::provider::IndexedResourceResponse>],
    ) -> bool {
        if self.measured_presentation_revision != Some(self.shared_snapshot().presentation_revision)
        {
            return false;
        }
        self.core.acknowledge_table_window(owner, pages)
    }

    pub(crate) fn prepare_resource_response(
        &self,
        demand: &super::ResourceTranscriptDemand,
        response: super::TranscriptProviderResponseKind,
    ) -> Option<Self> {
        if !self.resource_demands().contains(demand) {
            return None;
        }
        let mut core = self.core.prepare_resource_response(demand, response)?;
        core.carry_narrative_layout_from(&self.core);
        Some(Self {
            lifetime: self.lifetime.clone(),
            core,
            scroll_controller: self.scroll_controller.clone(),
            last_frame_window: self.last_frame_window.clone(),
            measured_presentation_revision: self.measured_presentation_revision,
        })
    }

    pub(crate) fn refresh_pending_indexed_resource(
        &mut self,
        demand: &super::ResourceTranscriptDemand,
    ) -> bool {
        self.core.refresh_pending_indexed_resource(demand)
    }

    pub(crate) fn set_indexed_resource_demand(
        &mut self,
        demand: &super::ResourceTranscriptDemand,
        inner: Option<super::provider::IndexedResourceDemand>,
    ) -> bool {
        self.core.set_indexed_resource_demand(demand, inner)
    }

    pub(crate) fn can_reserve_code_layout(
        &self,
        demand: &super::ResourceTranscriptDemand,
        bytes: usize,
    ) -> bool {
        self.core.can_reserve_code_layout(demand, bytes)
    }

    pub(crate) fn reserve_code_layout(
        &mut self,
        demand: &super::ResourceTranscriptDemand,
        bytes: usize,
    ) -> bool {
        self.core.reserve_code_layout(demand, bytes)
    }

    pub(crate) fn indexed_resource_response(
        &self,
        resource: &super::ResourceId,
    ) -> Option<std::sync::Arc<super::provider::IndexedResourceResponse>> {
        self.core.indexed_resource_response(resource)
    }

    pub(crate) fn indexed_resource_owner(
        &self,
        resource: &super::ResourceId,
    ) -> Option<super::ResourceTranscriptDemand> {
        self.core.indexed_resource_owner(resource)
    }

    pub(crate) fn indexed_resource_pages(
        &self,
        resource: &super::ResourceId,
    ) -> Vec<std::sync::Arc<super::provider::IndexedResourceResponse>> {
        self.core.indexed_resource_pages(resource)
    }

    pub(crate) fn prepare_resource_delivery(
        &self,
        demand: &super::ResourceTranscriptDemand,
        response: super::provider::ResourceDeliveryResponse,
    ) -> Option<Self> {
        match response {
            super::provider::ResourceDeliveryResponse::Raw(response) => {
                self.prepare_resource_response(demand, response)
            }
            super::provider::ResourceDeliveryResponse::Indexed(response) => {
                if !self.resource_demands().contains(demand) {
                    return None;
                }
                let mut core = self
                    .core
                    .prepare_indexed_resource_response(demand, response)?;
                core.carry_narrative_layout_from(&self.core);
                Some(Self {
                    lifetime: self.lifetime.clone(),
                    core,
                    scroll_controller: self.scroll_controller.clone(),
                    last_frame_window: self.last_frame_window.clone(),
                    measured_presentation_revision: self.measured_presentation_revision,
                })
            }
        }
    }

    pub(crate) fn prepare_adjacent_extension(
        &self,
        demand: &AdjacentTranscriptDemand,
        prepared: &PreparedTranscriptActivation,
    ) -> Option<Self> {
        if self.adjacent_demand().as_ref() != Some(demand) {
            return None;
        }
        let mut core = self.core.prepare_adjacent_extension(prepared)?;
        core.carry_narrative_layout_from(&self.core);
        Some(Self {
            lifetime: self.lifetime.clone(),
            core,
            scroll_controller: self.scroll_controller.clone(),
            last_frame_window: None,
            measured_presentation_revision: None,
        })
    }

    pub(crate) fn publish_coherent_activation(
        &mut self,
        prepared: PreparedTranscriptActivation,
    ) -> TranscriptActivationOutcome {
        let (candidate, outcome) = self.prepare_coherent_activation(prepared);
        *self = candidate;
        outcome
    }

    pub(crate) fn prepare_coherent_activation(
        &self,
        prepared: PreparedTranscriptActivation,
    ) -> (Self, TranscriptActivationOutcome) {
        let mut candidate = Self::empty();
        candidate.core = self.core.empty_successor();
        candidate.lifetime = std::sync::Arc::clone(&self.lifetime);
        let same_view = prepared.view_id.as_ref() == self.core.selected_view_id();
        let mut outcome = if same_view {
            candidate.scroll_controller = self.scroll_controller.clone();
            candidate.last_frame_window = self.last_frame_window.clone();
            candidate
                .core
                .apply_prepared_activation(prepared, TranscriptActivationSource::RunningThread)
        } else {
            candidate.apply_prepared_activation(prepared, TranscriptActivationSource::RunningThread)
        };
        if same_view {
            candidate.core.inherit_narratives(&self.core);
            let snapshot = candidate.core.presentation_snapshot();
            outcome.presentation_revision = snapshot.presentation_revision;
            outcome.state = snapshot.state;
        }
        (candidate, outcome)
    }

    pub(crate) fn scroll_state(&self) -> RealizedFrameScrollStateSnapshot {
        self.scroll_controller.state_snapshot()
    }

    pub(crate) fn is_same_view(&self, prepared: &PreparedTranscriptActivation) -> bool {
        prepared.view_id.as_ref() == self.core.selected_view_id()
    }

    pub(crate) fn selected_view_id(&self) -> Option<&super::TranscriptViewId> {
        self.core.selected_view_id()
    }

    pub(crate) fn detach_tail(&mut self) {
        self.scroll_controller.detach_live_tail_following();
    }

    pub(crate) fn accept_measured_frame(&mut self, mut window: RealizedFrameWindow) {
        let snapshot = self.shared_snapshot();
        self.measured_presentation_revision = (window.presentation_revision
            == snapshot.presentation_revision)
            .then_some(window.presentation_revision);
        if let Some(previous) = &self.last_frame_window {
            let before = previous.overscan_range.start
                ..previous.overscan_range.end.min(window.overscan_range.start);
            let after = previous.overscan_range.start.max(window.overscan_range.end)
                ..previous.overscan_range.end;
            for range in [before, after] {
                if !range.is_empty() {
                    window.demand_facts.push(DemandFact::new(
                        snapshot.presentation_revision,
                        super::DemandFactKind::ObsoleteRange { range },
                    ));
                }
            }
        }
        let mode = self.scroll_controller.state_snapshot().scroll_mode;
        self.scroll_controller
            .accept_measured_frame(&snapshot, &window, mode);
        for fact in &window.demand_facts {
            self.core.push_demand_fact(fact.clone());
        }
        self.core.reconcile_resource_demand();
        self.last_frame_window = Some(window);
    }

    pub(crate) fn refresh_placement(&self) -> TranscriptActivationPlacement {
        let state = self.scroll_controller.state_snapshot();
        if state.scroll_mode == super::frame::RealizedFrameScrollMode::LiveTailFollowing {
            TranscriptActivationPlacement::Tail
        } else {
            state
                .anchor
                .and_then(|anchor| anchor.position)
                .map(TranscriptActivationPlacement::Position)
                .unwrap_or(TranscriptActivationPlacement::Start)
        }
    }

    pub(crate) fn retire(&mut self) {
        *self = Self::empty();
    }

    pub(crate) fn status_facts(&self) -> ResidentTranscriptStatusFacts {
        ResidentTranscriptStatusFacts::from_core_snapshot(
            &self.core.core_snapshot(),
            self.scroll_controller.state_snapshot(),
        )
    }

    pub(crate) fn demand_fact_snapshot(&self) -> DemandFactSinkSnapshot {
        self.core.demand_fact_snapshot()
    }

    pub(crate) fn push_demand_fact(&mut self, fact: DemandFact) {
        self.core.push_demand_fact(fact);
    }

    pub(crate) fn begin_activation(
        &mut self,
        seed: TranscriptActivationSeed,
    ) -> TranscriptActivationOutcome {
        match seed.placement {
            TranscriptActivationPlacement::Tail => {
                self.scroll_controller.begin_live_tail_following()
            }
            TranscriptActivationPlacement::Start | TranscriptActivationPlacement::Position(_) => {
                self.scroll_controller.detach_live_tail_following();
            }
        }
        self.core.begin_activation(seed)
    }

    pub(crate) fn apply_prepared_activation(
        &mut self,
        prepared: PreparedTranscriptActivation,
        source: TranscriptActivationSource,
    ) -> TranscriptActivationOutcome {
        match prepared.placement {
            TranscriptActivationPlacement::Tail => {
                self.scroll_controller.begin_live_tail_following()
            }
            TranscriptActivationPlacement::Start | TranscriptActivationPlacement::Position(_) => {
                self.scroll_controller.detach_live_tail_following();
            }
        }
        self.core.apply_prepared_activation(prepared, source)
    }

    pub(crate) fn handle_provider_response(
        &mut self,
        response: TranscriptProviderResponse,
    ) -> ResidentProviderResponseEffect {
        self.core.handle_provider_response(response)
    }

    pub(crate) fn drain_demand_facts(&mut self) -> Vec<DemandFact> {
        self.core.drain_demand_facts()
    }

    pub(crate) fn realize_frame(&mut self, request: RealizedFrameRequest) -> RealizedFrameWindow {
        let snapshot = self.core.shared_presentation_snapshot();
        let window = self.scroll_controller.realize(&snapshot, request);
        for fact in &window.demand_facts {
            self.core.push_demand_fact(fact.clone());
        }
        self.last_frame_window = Some(window.clone());
        window
    }

    pub(crate) fn manual_scroll(
        &mut self,
        command: ManualTranscriptScrollCommand,
    ) -> RealizedFrameWindow {
        let window = self.realize_frame(command.frame_request());
        window
    }

    pub(crate) fn apply_resident_selection(
        &mut self,
        command: ResidentSelectionCommand,
    ) -> ResidentSelectionOutcome {
        self.core.apply_resident_selection(command)
    }

    pub(crate) fn clear_resident_selection(&mut self) -> ResidentSelectionOutcome {
        self.core.clear_resident_selection()
    }

    pub(crate) fn resident_copy_payload(
        &self,
    ) -> Result<ResidentTranscriptCopyPayload, ResidentSelectionUnavailable> {
        self.core.resident_copy_payload()
    }

    pub(crate) fn resident_selection(&self) -> Option<ResidentTranscriptSelection> {
        self.core.resident_selection()
    }

    pub(crate) fn apply_resident_quote_target(
        &mut self,
        command: ResidentQuoteCommand,
    ) -> ResidentQuoteOutcome {
        self.core.apply_resident_quote_target(command)
    }

    pub(crate) fn clear_resident_quote_target(&mut self) -> ResidentQuoteOutcome {
        self.core.clear_resident_quote_target()
    }

    pub(crate) fn resource_discussion_source(
        &self,
        owner: &super::ResourceTranscriptDemand,
        activation: u64,
        presentation: u64,
        range: std::ops::Range<u64>,
        text: String,
    ) -> Option<super::CapturedDiscussionSource> {
        self.core
            .resource_discussion_source(owner, activation, presentation, range, text)
    }

    pub(crate) fn mapped_discussion_source(
        &self,
        activation: u64,
        revision: u64,
        ranges: &[(super::ResidentPresentationRecordId, std::ops::Range<usize>)],
    ) -> Option<super::core::CapturedDiscussionSource> {
        self.core
            .mapped_discussion_source(activation, revision, ranges)
    }

    pub(crate) fn mapped_quote_markdown(
        &self,
        activation: u64,
        revision: u64,
        ranges: &[(super::ResidentPresentationRecordId, std::ops::Range<usize>)],
    ) -> Result<String, ResidentSelectionUnavailable> {
        self.core
            .mapped_quote_markdown(activation, revision, ranges)
    }

    pub(crate) fn resident_quote_payload(
        &self,
    ) -> Result<ResidentTranscriptQuotePayload, ResidentSelectionUnavailable> {
        self.core.resident_quote_payload()
    }

    pub(crate) fn resident_quote_target(&self) -> Option<ResidentTranscriptQuoteTarget> {
        self.core.resident_quote_target()
    }

    pub(crate) fn apply_resident_context_menu_target(
        &mut self,
        command: ResidentContextMenuCommand,
    ) -> ResidentContextMenuOutcome {
        self.core.apply_resident_context_menu_target(command)
    }

    pub(crate) fn clear_resident_context_menu_target(&mut self) -> ResidentContextMenuOutcome {
        self.core.clear_resident_context_menu_target()
    }

    pub(crate) fn resident_context_menu_target(
        &self,
    ) -> Option<ResidentTranscriptContextMenuTarget> {
        self.core.resident_context_menu_target()
    }

    pub(crate) fn context_menu_narrative_kind(
        &self,
        target: &ResidentTranscriptContextMenuTarget,
    ) -> Option<super::TranscriptNarrativeKind> {
        let snapshot = self.core.core_snapshot();
        if target.presentation_revision != snapshot.presentation.presentation_revision {
            return None;
        }
        let record = snapshot
            .presentation
            .records
            .iter()
            .find_map(|record| record.authored_part(&target.record.record_id))?;
        let current = super::context_menu::resident_context_menu_record(
            record,
            target.record.geometry.clone(),
        )
        .ok()?;
        if current != target.record {
            return None;
        }
        super::context_menu::resident_context_menu_narrative_kind(
            &current,
            &snapshot.resident.view_records,
        )
    }

    pub(crate) fn resident_context_menu_command_target(&self) -> ResidentContextMenuCommandTarget {
        ResidentContextMenuCommandTarget::from_active_target(
            self.core.resident_context_menu_target(),
        )
    }

    pub(crate) fn apply_resident_media_action_target(
        &mut self,
        command: ResidentMediaActionCommand,
    ) -> ResidentMediaActionOutcome {
        self.core.apply_resident_media_action_target(command)
    }

    pub(crate) fn clear_resident_media_action_target(&mut self) -> ResidentMediaActionOutcome {
        self.core.clear_resident_media_action_target()
    }

    pub(crate) fn resident_media_action_payload(
        &self,
    ) -> Result<ResidentTranscriptMediaPayload, ResidentMediaActionUnavailable> {
        self.core.resident_media_action_payload()
    }

    pub(crate) fn resident_media_action_target(
        &self,
    ) -> Option<ResidentTranscriptMediaActionTarget> {
        self.core.resident_media_action_target()
    }

    pub(crate) fn resident_media_preview_command_target(
        &self,
    ) -> ResidentMediaPreviewCommandTarget {
        ResidentMediaPreviewCommandTarget::from_resident_payload(
            self.core.resident_media_action_payload(),
        )
    }

    pub(crate) fn resident_media_copy_command_target(&self) -> ResidentMediaCopyCommandTarget {
        ResidentMediaCopyCommandTarget::from_resident_payload(
            self.core.resident_media_action_payload(),
        )
    }

    pub(crate) fn resident_media_save_command_target(&self) -> ResidentMediaSaveCommandTarget {
        ResidentMediaSaveCommandTarget::from_resident_payload(
            self.core.resident_media_action_payload(),
        )
    }

    pub(crate) fn diagnostic_snapshot(&self) -> SyndicTranscriptDiagnosticSnapshot {
        let core_snapshot = self.core.core_snapshot();
        let scroll_snapshot = self.scroll_controller.state_snapshot();
        let mut snapshot = SyndicTranscriptDiagnosticSnapshot::from_core_snapshot(&core_snapshot);
        snapshot.frame.scroll_mode = scroll_snapshot.scroll_mode.diagnostic_label();
        snapshot.frame.anchor_record = scroll_snapshot.anchor.map(|anchor| anchor.record_id.0);
        snapshot
    }

    pub(crate) fn unavailable_command(&self, command: &'static str) -> TranscriptCommandResult {
        TranscriptCommandResult::unavailable(command)
    }
}

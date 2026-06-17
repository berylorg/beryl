use crate::diagnostic_dynamic_tools::TranscriptFrameMetricsSnapshot;

use super::{
    DemandFact, DemandFactSinkSnapshot, ManualTranscriptScrollCommand, RealizedFrameRequest,
    RealizedFrameScrollController, RealizedFrameWindow, ResidentContextMenuCommand,
    ResidentContextMenuCommandTarget, ResidentContextMenuOutcome, ResidentMediaActionCommand,
    ResidentMediaActionOutcome, ResidentMediaActionUnavailable, ResidentMediaCopyCommandTarget,
    ResidentMediaPreviewCommandTarget, ResidentMediaSaveCommandTarget,
    ResidentProviderResponseEffect, ResidentQuoteCommand, ResidentQuoteOutcome,
    ResidentSelectionCommand, ResidentSelectionOutcome, ResidentSelectionUnavailable,
    ResidentTranscriptContextMenuTarget, ResidentTranscriptCopyPayload, ResidentTranscriptCore,
    ResidentTranscriptMediaActionTarget, ResidentTranscriptMediaPayload,
    ResidentTranscriptQuotePayload, ResidentTranscriptQuoteTarget, ResidentTranscriptSelection,
    ResidentTranscriptSnapshot, ResidentTranscriptStatusFacts, SyndicTranscriptDiagnosticSnapshot,
    TranscriptActivationOutcome, TranscriptActivationPlacement, TranscriptActivationSeed,
    TranscriptCommandResult, TranscriptProviderResponse,
};

#[derive(Clone, Debug)]
pub(crate) struct SyndicTranscriptHost {
    core: ResidentTranscriptCore,
    scroll_controller: RealizedFrameScrollController,
}

impl Default for SyndicTranscriptHost {
    fn default() -> Self {
        Self::empty()
    }
}

impl SyndicTranscriptHost {
    pub(crate) fn empty() -> Self {
        Self {
            core: ResidentTranscriptCore::empty(),
            scroll_controller: RealizedFrameScrollController::new(),
        }
    }

    pub(crate) fn snapshot(&self) -> ResidentTranscriptSnapshot {
        self.core.presentation_snapshot()
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
        let snapshot = self.core.presentation_snapshot();
        let window = self.scroll_controller.realize(&snapshot, request);
        for fact in &window.demand_facts {
            self.core.push_demand_fact(fact.clone());
        }
        window
    }

    pub(crate) fn manual_scroll(
        &mut self,
        command: ManualTranscriptScrollCommand,
    ) -> RealizedFrameWindow {
        self.realize_frame(command.frame_request())
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

    pub(crate) fn frame_metrics_snapshot(&self) -> TranscriptFrameMetricsSnapshot {
        TranscriptFrameMetricsSnapshot::default()
    }

    pub(crate) fn unavailable_command(&self, command: &'static str) -> TranscriptCommandResult {
        TranscriptCommandResult::unavailable(command)
    }
}

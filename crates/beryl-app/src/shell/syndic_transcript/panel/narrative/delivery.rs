use super::*;

impl SyndicTranscriptPanel {
    pub(crate) fn cancel_narrative_preparations(&mut self) {
        if self
            .staged
            .as_ref()
            .is_some_and(|staged| staged.narrative.is_some())
        {
            self.discard_staged_activation();
        }
        self.host.cancel_narrative_preparations();
    }

    pub(crate) fn narrative_demand(&self) -> Option<NarrativeTranscriptDemand> {
        if self.pending_delta != 0.0 {
            return None;
        }
        if self.staged.is_some() || self.last_frame_window.is_none() {
            return None;
        }
        self.host.narrative_demand()
    }

    pub(crate) fn reserve_narrative_preparation(
        &mut self,
        demand: &NarrativeTranscriptDemand,
    ) -> Option<usize> {
        self.host.reserve_narrative_preparation(demand)
    }

    pub(crate) fn narrative_demand_current(&self, demand: &NarrativeTranscriptDemand) -> bool {
        self.pending_delta == 0.0 && self.host.narrative_demand_current(demand)
    }

    pub(crate) fn finish_narrative_preparation(&mut self, demand: &NarrativeTranscriptDemand) {
        self.host.finish_narrative_preparation(demand);
    }

    pub(crate) fn stage_narrative_delivery(
        &mut self,
        demand: &NarrativeTranscriptDemand,
        response: NarrativePreparationResponse,
        allocation: Option<gpui::Size<Pixels>>,
    ) -> bool {
        if self.staged.is_some() {
            return false;
        }
        let Some(host) = self.host.prepare_narrative_delivery(demand, response) else {
            return false;
        };
        let snapshot = host.shared_snapshot();
        let outcome = super::super::TranscriptActivationOutcome {
            activation_revision: snapshot.activation_revision,
            presentation_revision: snapshot.presentation_revision,
            state: snapshot.state.clone(),
            retained_previous_snapshot: true,
            provider_request: None,
        };
        self.quote_state.capture = None;
        self.staged = Some(StagedTranscript {
            activation: None,
            host,
            outcome,
            allocation,
            unplaced_tail: false,
            adjacent: None,
            resource: None,
            narrative: Some(demand.clone()),
        });
        self.geometry = None;
        true
    }

    pub(crate) fn narrative_candidate_current(&self, demand: &NarrativeTranscriptDemand) -> bool {
        self.staged
            .as_ref()
            .is_some_and(|candidate| candidate.narrative.as_ref() == Some(demand))
            && self.narrative_demand_current(demand)
    }

    pub(crate) fn update_narrative_allocation(
        &mut self,
        demand: &NarrativeTranscriptDemand,
        allocation: gpui::Size<Pixels>,
    ) -> bool {
        if !self.narrative_candidate_current(demand) {
            return false;
        }
        let staged = self.staged.as_mut().unwrap();
        if staged.allocation != Some(allocation) {
            staged.allocation = Some(allocation);
            self.geometry = None;
        }
        true
    }

    pub(crate) fn publish_narrative_candidate(&mut self, demand: &NarrativeTranscriptDemand) {
        assert!(self.narrative_candidate_current(demand) && self.measurement_ready());
        self.commit_measured_activation();
        self.finish_narrative_preparation(demand);
    }

    pub(crate) fn discard_narrative_candidate(&mut self, demand: &NarrativeTranscriptDemand) {
        if self
            .staged
            .as_ref()
            .is_some_and(|candidate| candidate.narrative.as_ref() == Some(demand))
        {
            self.discard_staged_activation();
        }
        self.finish_narrative_preparation(demand);
    }
}

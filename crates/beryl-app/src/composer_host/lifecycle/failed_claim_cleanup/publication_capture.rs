use super::super::super::publication::{ComposerHostPublicationTicket, PublicationStage};
use super::*;

pub(super) struct RetiredClaimPublicationCapture {
    binding: ComposerHostBinding,
    ticket: ComposerHostPublicationTicket,
}

impl RetiredClaimPublicationCapture {
    pub(super) fn validate(
        &self,
        resident: &super::super::failed_resident::ComposerHostFailedResident,
    ) -> Result<(), String> {
        if resident.binding() != self.binding
            || resident.checkpoint() != self.binding.candidate()
            || self.ticket.candidate_generation() != self.binding.candidate().candidate_generation()
        {
            return Err("original predecessor publication capture changed".into());
        }
        Ok(())
    }
}

impl SyndicComposerHost {
    pub(crate) fn retire_failed_claim_cleanup_with_publication_capture(
        self: Box<Self>,
        store: &HomeStore,
        saved: Option<ComposerHostSelectionSave>,
        committed: bool,
        markers: &crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
        captured: ComposerHostBinding,
    ) -> Result<ComposerHostRetiredClaimPredecessor, (Box<Self>, ComposerHostBinding)> {
        let Some(proof) = self.authenticate_failed_claim_publication_capture(captured) else {
            return Err((self, captured));
        };
        match self.retire_failed_claim_cleanup(store, saved, committed, markers) {
            Ok(mut predecessor) => {
                predecessor.publication_capture = Some(proof);
                Ok(predecessor)
            }
            Err(host) => Err((host, captured)),
        }
    }

    fn authenticate_failed_claim_publication_capture(
        &self,
        captured: ComposerHostBinding,
    ) -> Option<RetiredClaimPublicationCapture> {
        let active = self.active.as_ref()?;
        if active.binding != captured || active.storage_candidate != captured.candidate() {
            return None;
        }
        let save = self.lifecycle.barrier.as_ref()?.publication.as_ref()?;
        let Some(ComposerHostPublicationLane::Publication(pending)) =
            self.publication.lane.as_deref()
        else {
            return None;
        };
        if pending.binding() != captured
            || pending.ticket != save.ticket
            || pending.ticket.candidate_generation() != captured.candidate().candidate_generation()
        {
            return None;
        }
        let prepared = match &pending.stage {
            PublicationStage::Ready(prepared)
            | PublicationStage::Reconciling { prepared, .. }
            | PublicationStage::Terminal {
                prepared: Some(prepared),
                ..
            } => prepared,
            _ => return None,
        };
        let retained = self.publication.retained.as_ref()?;
        let candidate_pair = syndic_storage::DraftRootHistoryPairV1::new(
            active.storage_candidate.root(),
            active.storage_candidate.history(),
        );
        if retained.binding != captured
            || prepared.syndic.request().candidate() != candidate_pair
            || prepared.syndic.request().session_id() != active.storage_candidate.session_id()
            || prepared.syndic.request().candidate_generation()
                != active.storage_candidate.candidate_generation()
            || prepared.syndic.request().selector() != active.durable_selector
            || retained.prepared.syndic.request() != prepared.syndic.request()
            || retained.prepared.syndic.canonical_request() != prepared.syndic.canonical_request()
            || retained.prepared.syndic.captured_frontier() != prepared.syndic.captured_frontier()
            || matches!(&pending.stage, PublicationStage::Reconciling { .. })
                && !matches!(
                    &retained.outcome,
                    RetainedComposerCommandOutcome::Indeterminate { .. }
                )
        {
            return None;
        }
        Some(RetiredClaimPublicationCapture {
            binding: captured,
            ticket: pending.ticket,
        })
    }
}

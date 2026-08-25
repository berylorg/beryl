use beryl_home_store::HomeStore;

use super::*;
use crate::composer_host::publication::PublicationStage;
use crate::composer_host::{
    ComposerHostPublicationReleaseCompletion, ComposerHostPublicationReleaseReason,
};

impl SyndicComposerHost {
    pub fn dispose_composer_service(
        &mut self,
        store: &HomeStore,
    ) -> Result<ComposerHostServiceDisposalCompletion, ComposerHostError> {
        self.lifecycle.service_disposed = true;
        self.lifecycle.clear_runtime();
        self.lifecycle.dirty_adoption_seen = false;
        self.lifecycle.last_publication_completion = None;
        self.pending.clear();
        self.pending_mutation = None;
        self.detached_mutations.clear();
        self.pending_history = None;
        self.detached_history.clear();
        self.last_mutation_identity = None;
        self.last_history_identity = None;
        self.last_history_outcome = None;
        self.last_request_id = 0;

        let action = match self.publication.lane.as_deref() {
            Some(ComposerHostPublicationLane::Publication(pending)) => match &pending.stage {
                PublicationStage::Sealing { .. }
                | PublicationStage::Sealed(_)
                | PublicationStage::Ready(_) => ServiceDisposalAction::Release(pending.ticket),
                PublicationStage::Releasing { .. } => {
                    ServiceDisposalAction::DriveRelease(pending.ticket)
                }
                PublicationStage::Reconciling { .. } => {
                    ServiceDisposalAction::ReconcilePublication(pending.ticket)
                }
                PublicationStage::Terminal { .. } => ServiceDisposalAction::Complete,
            },
            Some(ComposerHostPublicationLane::Disposal(pending))
                if pending.reconciliation.is_some() =>
            {
                ServiceDisposalAction::ReconcileDisposal(pending.ticket)
            }
            Some(ComposerHostPublicationLane::Disposal(_)) | None => {
                ServiceDisposalAction::Complete
            }
        };

        let pending = match action {
            ServiceDisposalAction::Release(ticket) => matches!(
                self.release_publication_lane(
                    store,
                    ticket,
                    ComposerHostPublicationReleaseReason::ServiceDisposed,
                )?,
                ComposerHostPublicationReleaseCompletion::Pending
            ),
            ServiceDisposalAction::DriveRelease(ticket) => matches!(
                self.drive_publication_lane(store, ticket)?,
                ComposerHostPublicationDrive::ReleasePending
            ),
            ServiceDisposalAction::ReconcilePublication(ticket) => matches!(
                self.reconcile_publication_lane(store, ticket)?,
                ComposerHostPublicationCompletion::ReconciliationPending
            ),
            ServiceDisposalAction::ReconcileDisposal(ticket) => matches!(
                self.reconcile_clean_disposal(store, ticket)?,
                ComposerHostDisposalCompletion::ReconciliationPending
            ),
            ServiceDisposalAction::Complete => false,
        };
        if pending {
            return Ok(ComposerHostServiceDisposalCompletion::Pending);
        }
        self.publication.lane = None;
        self.active = None;
        Ok(ComposerHostServiceDisposalCompletion::Disposed)
    }
}

enum ServiceDisposalAction {
    Release(ComposerHostPublicationTicket),
    DriveRelease(ComposerHostPublicationTicket),
    ReconcilePublication(ComposerHostPublicationTicket),
    ReconcileDisposal(ComposerHostDisposalTicket),
    Complete,
}

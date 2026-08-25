use beryl_home_store::HomeStore;

use super::*;
use crate::composer_host::publication::PublicationStage;
use crate::composer_host::{
    ComposerHostPublicationReleaseCompletion, ComposerHostPublicationReleaseReason,
};

pub(super) enum PublicationStep {
    Progress,
    Ready,
    Complete(ComposerHostPublicationCompletion),
}

impl SyndicComposerHost {
    pub(super) fn current_flush_callback(
        &self,
        store: &HomeStore,
        ticket: ComposerHostFlushTicket,
    ) -> bool {
        let Some(active) = self.active.as_ref() else {
            return false;
        };
        active.binding.home_id() == ticket.home_id
            && active.binding.home_generation() == ticket.home_generation
            && active.binding.host_generation() == ticket.host_generation
            && ComposerHostLifecycleCoordinator::callback_store_matches(active.binding, store)
    }

    pub(super) fn current_publication_callback(
        &self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> bool {
        let Some(ComposerHostPublicationLane::Publication(pending)) =
            self.publication.lane.as_deref()
        else {
            return false;
        };
        let Some(active) = self.active.as_ref() else {
            return false;
        };
        let binding = pending.binding();
        pending.ticket == ticket
            && binding.home_id() == active.binding.home_id()
            && binding.home_generation() == active.binding.home_generation()
            && binding.host_generation() == active.binding.host_generation()
            && ComposerHostLifecycleCoordinator::callback_store_matches(binding, store)
    }

    pub(super) fn advance_publication_step(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<PublicationStep, ComposerHostError> {
        let cancellation_can_win = match self.publication.lane.as_deref() {
            Some(ComposerHostPublicationLane::Publication(pending)) if pending.ticket == ticket => {
                pending.is_cancelled()
                    && !matches!(
                        &pending.stage,
                        PublicationStage::Reconciling { .. } | PublicationStage::Terminal { .. }
                    )
            }
            _ => false,
        };
        if cancellation_can_win {
            return Ok(
                match self.release_publication_lane(
                    store,
                    ticket,
                    ComposerHostPublicationReleaseReason::Cancelled,
                )? {
                    ComposerHostPublicationReleaseCompletion::Pending => PublicationStep::Progress,
                    ComposerHostPublicationReleaseCompletion::Released => {
                        PublicationStep::Complete(
                            ComposerHostPublicationCompletion::CancelledBeforeAdmission,
                        )
                    }
                },
            );
        }
        let action = match self.publication.lane.as_deref() {
            Some(ComposerHostPublicationLane::Publication(pending)) if pending.ticket == ticket => {
                match &pending.stage {
                    PublicationStage::Ready(_) => 1,
                    PublicationStage::Reconciling { .. } => 2,
                    PublicationStage::Terminal { reason, .. } => {
                        return Ok(PublicationStep::Complete(match reason {
                            ComposerHostPublicationUnavailable::DurableBaseConflict => {
                                ComposerHostPublicationCompletion::DurableBaseConflict
                            }
                            ComposerHostPublicationUnavailable::SessionDisposed => {
                                ComposerHostPublicationCompletion::SessionDisposed
                            }
                            ComposerHostPublicationUnavailable::IdentityCollision
                            | ComposerHostPublicationUnavailable::DisposalDirtyConflict => {
                                ComposerHostPublicationCompletion::OccupiedIdentityCollision
                            }
                            ComposerHostPublicationUnavailable::ReconciliationCollision => {
                                ComposerHostPublicationCompletion::ReconciliationCollision
                            }
                        }));
                    }
                    _ => 0,
                }
            }
            _ => return Err(ComposerHostError::StalePublicationGeneration),
        };
        match action {
            0 => Ok(match self.drive_publication_lane(store, ticket)? {
                ComposerHostPublicationDrive::Ready => PublicationStep::Ready,
                ComposerHostPublicationDrive::Progress
                | ComposerHostPublicationDrive::ReleasePending => PublicationStep::Progress,
                ComposerHostPublicationDrive::NotCommitted(_) => {
                    match self.release_publication_lane(
                        store,
                        ticket,
                        ComposerHostPublicationReleaseReason::Failed,
                    )? {
                        ComposerHostPublicationReleaseCompletion::Pending => {
                            self.mark_publication_failure(
                                ticket,
                                ComposerHostFlushFailure::NotCommitted,
                            );
                            PublicationStep::Progress
                        }
                        ComposerHostPublicationReleaseCompletion::Released => {
                            PublicationStep::Complete(
                                ComposerHostPublicationCompletion::NotCommitted,
                            )
                        }
                    }
                }
            }),
            1 => Ok(PublicationStep::Complete(
                self.execute_publication_lane(store, ticket)?,
            )),
            _ => Ok(PublicationStep::Complete(
                self.reconcile_publication_lane(store, ticket)?,
            )),
        }
    }

    fn mark_publication_failure(
        &mut self,
        ticket: ComposerHostPublicationTicket,
        failure: ComposerHostFlushFailure,
    ) {
        if let Some(save) = self.lifecycle.autosave.as_mut()
            && save.ticket == ticket
        {
            save.failure = Some(failure);
        }
        if let Some(save) = self
            .lifecycle
            .barrier
            .as_mut()
            .and_then(|barrier| barrier.publication.as_mut())
            && save.ticket == ticket
        {
            save.failure = Some(failure);
        }
    }
}

pub(super) fn publication_failure(
    unavailable: ComposerHostPublicationUnavailable,
) -> ComposerHostFlushFailure {
    match unavailable {
        ComposerHostPublicationUnavailable::DurableBaseConflict => {
            ComposerHostFlushFailure::DurableBaseConflict
        }
        ComposerHostPublicationUnavailable::SessionDisposed => {
            ComposerHostFlushFailure::SessionDisposed
        }
        ComposerHostPublicationUnavailable::IdentityCollision => {
            ComposerHostFlushFailure::IdentityCollision
        }
        ComposerHostPublicationUnavailable::ReconciliationCollision => {
            ComposerHostFlushFailure::ReconciliationCollision
        }
        ComposerHostPublicationUnavailable::DisposalDirtyConflict => {
            ComposerHostFlushFailure::DisposalDirtyConflict
        }
    }
}

pub(super) fn recoverable_error(error: &ComposerHostError) -> bool {
    matches!(error_failure(error), ComposerHostFlushFailure::Recoverable)
}

pub(super) fn stale_callback_error(error: &ComposerHostError) -> bool {
    matches!(
        error,
        ComposerHostError::ForeignHome { .. }
            | ComposerHostError::HomeGenerationChanged { .. }
            | ComposerHostError::OldBinding
            | ComposerHostError::StalePublicationGeneration
    )
}

pub(super) fn error_failure(error: &ComposerHostError) -> ComposerHostFlushFailure {
    match error {
        ComposerHostError::DurableSelectorChanged => ComposerHostFlushFailure::DurableBaseConflict,
        ComposerHostError::PublicationUnavailable => ComposerHostFlushFailure::SessionDisposed,
        ComposerHostError::CandidateBindingChanged => ComposerHostFlushFailure::GenerationLost,
        _ => ComposerHostFlushFailure::Recoverable,
    }
}

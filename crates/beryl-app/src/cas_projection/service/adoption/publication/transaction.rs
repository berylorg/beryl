use std::sync::Arc;

use super::super::super::super::{
    connection::{
        CandidateSetRecoveryPublicationFailure, ConnectionEpochIdentity,
        RecoveredConnectionPublicationReason,
    },
    persistent_failure::PersistentFailureOldServiceEpochRetirementReason,
    service_supervisor::RunningServiceSlot,
};
use super::super::super::ProjectionConnectionServiceCloseError;
use super::super::ConnectionAdoptionState;
use super::{
    RecoveredServicePublicationReason, RecoveryServicePublicationAttempt,
    ReplacementWorkerPublicationState,
};

impl RecoveryServicePublicationAttempt {
    pub(super) fn run(
        &mut self,
        slot: &Arc<RunningServiceSlot>,
    ) -> Result<(), RecoveredServicePublicationReason> {
        self.preflight()?;
        self.retire_adoption_fence()?;
        #[cfg(test)]
        if std::mem::take(&mut self.attempt.retain_late_after_publication_fence) {
            self.attempt
                .inventory
                .as_ref()
                .expect("the retirement witness still owns its old-cut inventory")
                .retain_late_adoption_authority_for_test();
        }
        self.retire_old_connection_epochs()?;
        self.retire_old_service_epoch()?;
        self.commit(slot)
    }

    pub(super) fn dispose(&mut self) -> Result<(), ProjectionConnectionServiceCloseError> {
        let epoch_failed = self.restore_prepared_epoch().is_err();
        self.attempt.make_inert();
        let result = self.attempt.dispose_inert();
        self.connection_owners.clear();
        self.retirement_witness.take();
        if epoch_failed {
            Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
        } else {
            result
        }
    }

    fn retire_adoption_fence(&mut self) -> Result<(), RecoveredServicePublicationReason> {
        let fence = self
            .attempt
            .fence
            .take()
            .ok_or(RecoveredServicePublicationReason::PublicationTopologyMismatch)?;
        let retirement = self
            .attempt
            .inventory
            .as_ref()
            .ok_or(RecoveredServicePublicationReason::PublicationTopologyMismatch)?
            .retire_pending_quarantine_adoption(fence);
        match retirement {
            Ok(witness) => {
                self.retirement_witness = Some(witness);
                Ok(())
            }
            Err(error) => {
                self.attempt.fence = Some(error.into_fence());
                Err(RecoveredServicePublicationReason::AdoptionPublisherWon)
            }
        }
    }

    fn retire_old_connection_epochs(&mut self) -> Result<(), RecoveredServicePublicationReason> {
        for state in &mut self.attempt.connection_states {
            let ConnectionAdoptionState::Adopted(attachment) = state else {
                return Err(RecoveredServicePublicationReason::PublicationTopologyMismatch);
            };
            attachment
                .retire_closed_old_epoch()
                .map_err(|_| RecoveredServicePublicationReason::OldConnectionEpochRetirement)?;
        }
        Ok(())
    }

    fn retire_old_service_epoch(&mut self) -> Result<(), RecoveredServicePublicationReason> {
        let inventory = self
            .attempt
            .inventory
            .take()
            .ok_or(RecoveredServicePublicationReason::PublicationTopologyMismatch)?;
        let witness = self
            .retirement_witness
            .take()
            .ok_or(RecoveredServicePublicationReason::PublicationTopologyMismatch)?;
        match inventory.retire_old_service_epoch(witness) {
            Ok(witness) => {
                self.retirement_witness = Some(witness);
                Ok(())
            }
            Err(error) => {
                let reason = old_service_retirement_reason(error.reason());
                let (inventory, witness) = error.into_parts();
                self.attempt.inventory = Some(inventory);
                self.retirement_witness = Some(witness);
                Err(reason)
            }
        }
    }

    fn commit(
        &mut self,
        slot: &Arc<RunningServiceSlot>,
    ) -> Result<(), RecoveredServicePublicationReason> {
        let count = self.connection_owners.len();
        let mut connections = Vec::new();
        let mut forwarding_barriers = Vec::new();
        let mut authority_barriers = Vec::new();
        if connections.try_reserve_exact(count).is_err()
            || forwarding_barriers.try_reserve_exact(count).is_err()
            || authority_barriers.try_reserve_exact(count).is_err()
        {
            return Err(RecoveredServicePublicationReason::PublicationCapacityUnavailable);
        }
        connections.extend(
            self.connection_owners
                .iter()
                .map(|owner| Arc::clone(owner.connection_for_recovery_publication())),
        );

        let service = self
            .attempt
            .adopted_service
            .take()
            .ok_or(RecoveredServicePublicationReason::RecoveredServiceMismatch)?;
        let state = self
            .attempt
            .adopted_beryl_state
            .take()
            .ok_or(RecoveredServicePublicationReason::RecoveredServiceMismatch)?;
        self.prepared_epoch = Some(RunningServiceSlot::prepare_epoch(service, state));
        let validation_service = Arc::clone(
            self.prepared_epoch
                .as_ref()
                .expect("the publication attempt retains its prepared service epoch")
                .service(),
        );
        let home = validation_service.retained_home_for_recovery();
        let startup = Arc::clone(
            self.attempt
                .adopted_startup_gate
                .as_ref()
                .ok_or(RecoveredServicePublicationReason::StartupFenceUnavailable)?,
        );
        let expected_epoch = ConnectionEpochIdentity::new(
            validation_service.home_id(),
            validation_service.home_generation(),
            validation_service.service_generation(),
        );

        let mut installed = None;
        let publication = 'publication: {
            for connection in &connections {
                match connection.lock_epoch_for_recovery_publication() {
                    Ok(barrier) => forwarding_barriers.push(barrier),
                    Err(_) => {
                        break 'publication Err(
                            RecoveredServicePublicationReason::ForwardingEpochUnavailable,
                        );
                    }
                }
            }
            let service_connections = match validation_service.connections.lock() {
                Ok(connections) => connections,
                Err(_) => {
                    break 'publication Err(
                        RecoveredServicePublicationReason::ServiceMembershipUnavailable,
                    );
                }
            };
            for owner in &self.connection_owners {
                match owner.lock_for_recovery_publication(self.attempt.cut) {
                    Ok(barrier) => authority_barriers.push(barrier),
                    Err(failure) => {
                        break 'publication Err(candidate_authority_reason(failure));
                    }
                }
            }

            if let Err(reason) = self.validate_locked_publication(
                &connections,
                &forwarding_barriers,
                &authority_barriers,
                service_connections.as_slice(),
                &validation_service,
                &home,
                &startup,
                expected_epoch,
                ReplacementWorkerPublicationState::RetiredUnarmed,
            ) {
                break 'publication Err(reason);
            }
            for state in &mut self.attempt.connection_states {
                let ConnectionAdoptionState::Adopted(attachment) = state else {
                    break 'publication Err(
                        RecoveredServicePublicationReason::PublicationTopologyMismatch,
                    );
                };
                if let Err(failure) = attachment.arm_replacement_workers(&startup) {
                    break 'publication Err(connection_publication_reason(failure));
                }
            }
            if let Err(reason) = self.validate_locked_publication(
                &connections,
                &forwarding_barriers,
                &authority_barriers,
                service_connections.as_slice(),
                &validation_service,
                &home,
                &startup,
                expected_epoch,
                ReplacementWorkerPublicationState::Armed,
            ) {
                break 'publication Err(reason);
            }
            let startup_guard = match startup.lock_for_publication() {
                Ok(startup_guard) => startup_guard,
                Err(()) => {
                    break 'publication Err(
                        RecoveredServicePublicationReason::StartupFenceUnavailable,
                    );
                }
            };
            let epoch = self
                .prepared_epoch
                .take()
                .expect("validated publication retains its prepared service epoch");
            installed = match slot.install_recovered(epoch, startup_guard) {
                Ok(installed) => Some(installed),
                Err((epoch, startup_guard)) => {
                    self.prepared_epoch = Some(epoch);
                    drop(startup_guard);
                    break 'publication Err(
                        RecoveredServicePublicationReason::ProcessServiceSlotUnavailable,
                    );
                }
            };
            self.attempt.publication_committed = true;
            Ok(())
        };

        drop(authority_barriers);
        drop(forwarding_barriers);
        drop(validation_service);
        drop(home);
        if let Err(reason) = publication {
            self.restore_prepared_epoch()?;
            return Err(reason);
        }
        self.connection_owners.clear();
        self.retirement_witness.take();
        self.finish_attempt_after_publication();
        installed
            .expect("successful publication retains its installed service epoch")
            .finish_after_unlock();
        Ok(())
    }

    fn restore_prepared_epoch(&mut self) -> Result<(), RecoveredServicePublicationReason> {
        let Some(epoch) = self.prepared_epoch.take() else {
            return Ok(());
        };
        match epoch.into_parts() {
            Ok((service, state)) => {
                self.attempt.adopted_service = Some(service);
                self.attempt.adopted_beryl_state = Some(state);
                Ok(())
            }
            Err(epoch) => {
                self.prepared_epoch = Some(epoch);
                Err(RecoveredServicePublicationReason::ProcessServiceSlotUnavailable)
            }
        }
    }

    fn finish_attempt_after_publication(&mut self) {
        for state in self.attempt.connection_states.drain(..) {
            if let ConnectionAdoptionState::Adopted(attachment) = state {
                attachment.finish_publication();
            }
        }
        self.attempt.connections.clear();
        self.attempt.rejected_connections.clear();
        self.attempt.connection_check_scratch.clear();
        self.attempt.inert_attachments.clear();
        self.attempt.replacement_candidate_holds.clear();
        self.attempt.failed_candidate_permits.clear();
        self.attempt.old_candidate_holds.clear();
        self.attempt.adopted_startup_gate.take();
    }
}

fn candidate_authority_reason(
    failure: CandidateSetRecoveryPublicationFailure,
) -> RecoveredServicePublicationReason {
    match failure {
        CandidateSetRecoveryPublicationFailure::AuthorityUnavailable => {
            RecoveredServicePublicationReason::StableConnectionAuthorityUnavailable
        }
        CandidateSetRecoveryPublicationFailure::ConnectionRetired => {
            RecoveredServicePublicationReason::StableConnectionRetired
        }
        CandidateSetRecoveryPublicationFailure::TopologyMismatch => {
            RecoveredServicePublicationReason::PublicationTopologyMismatch
        }
    }
}

fn connection_publication_reason(
    failure: RecoveredConnectionPublicationReason,
) -> RecoveredServicePublicationReason {
    match failure {
        RecoveredConnectionPublicationReason::TopologyMismatch => {
            RecoveredServicePublicationReason::PublicationTopologyMismatch
        }
        RecoveredConnectionPublicationReason::OldEpochUnavailable => {
            RecoveredServicePublicationReason::OldConnectionEpochRetirement
        }
        RecoveredConnectionPublicationReason::DriverArm(_)
        | RecoveredConnectionPublicationReason::IngesterArm
        | RecoveredConnectionPublicationReason::IngesterRegistryUnavailable => {
            RecoveredServicePublicationReason::ReplacementWorkerArm
        }
    }
}

fn old_service_retirement_reason(
    _failure: PersistentFailureOldServiceEpochRetirementReason,
) -> RecoveredServicePublicationReason {
    RecoveredServicePublicationReason::OldServiceEpochRetirement
}

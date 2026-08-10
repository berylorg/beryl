use super::super::super::error::ProjectionCoordinatorError;
use super::super::ProjectionConnectionServiceCloseError;
use super::{
    AdoptedUnpublishedProjectionConnectionService, ConnectionAdoptionState, ServiceAdoptionAttempt,
};

impl ServiceAdoptionAttempt {
    #[cfg(test)]
    pub(super) fn retain_late_after_publication_fence_for_test(&mut self) {
        self.retain_late_after_publication_fence = true;
    }

    #[cfg(test)]
    pub(super) fn fail_replacement_ingester_arm_for_test(&mut self, index: usize) {
        let ConnectionAdoptionState::Adopted(attachment) = &mut self.connection_states[index]
        else {
            panic!("a converged publication test retains adopted connection attachments");
        };
        attachment.fail_replacement_ingester_arm_for_test();
    }

    pub(super) fn make_inert(&mut self) {
        if self.publication_committed {
            return;
        }
        if let Some(replacement) = self.replacement.as_ref() {
            if let Some(startup_gate) = replacement.startup_gate.as_ref() {
                startup_gate.cancel();
            }
        }
        if let Some(startup_gate) = self.adopted_startup_gate.as_ref() {
            startup_gate.cancel();
        }
        #[cfg(test)]
        if let Some(checkout) = self.adversarial_checkout.as_mut() {
            checkout.make_adversarial_connections_inert_for_test();
            self.rejected_connections
                .retain(|connection| !checkout.owns_secondary_connection_for_test(connection));
        }
        if !self.inert_connections_terminalized {
            let source_count = if self.connections.is_empty() {
                self.inventory.as_ref().map_or(0, |inventory| {
                    inventory.retained_service_connection_slice().len()
                })
            } else {
                self.connections
                    .len()
                    .checked_add(self.rejected_connections.len())
                    .unwrap_or(usize::MAX)
            };
            let can_detach = self.inert_attachment_limit.is_some_and(|limit| {
                limit
                    .checked_sub(self.inert_attachments.len())
                    .is_some_and(|remaining| {
                        source_count <= remaining && self.inert_attachments.capacity() >= limit
                    })
            });
            if can_detach {
                if self.connections.is_empty()
                    && let Some(inventory) = self.inventory.as_ref()
                {
                    for connection in inventory.retained_service_connection_slice() {
                        let attachment = connection.make_adoption_inert_in_place(self.cut);
                        if !attachment.is_empty() {
                            self.inert_attachments.push(attachment);
                        }
                    }
                }
                for connection in self.connections.iter().chain(&self.rejected_connections) {
                    let attachment = connection.make_adoption_inert_in_place(self.cut);
                    if !attachment.is_empty() {
                        self.inert_attachments.push(attachment);
                    }
                }
            } else {
                // The first fallback reservation failed (or a later unwind invalidated its
                // capacity invariant). Keep every endpoint resident in its stable core so this
                // transition remains allocation-free and explicit disposition can still join it.
                if let Some(inventory) = self.inventory.as_ref() {
                    for connection in inventory.retained_service_connection_slice() {
                        connection.make_adoption_inert_retaining_epoch_in_place(self.cut);
                    }
                }
                for connection in self.connections.iter().chain(&self.rejected_connections) {
                    connection.make_adoption_inert_retaining_epoch_in_place(self.cut);
                }
            }
            self.inert_connections_terminalized = true;
        }
        if let Some(inventory) = self.inventory.as_mut() {
            inventory.disarm_reescrow_after_terminal_inert();
        }
    }

    pub(super) fn dispose_inert(&mut self) -> Result<(), ProjectionConnectionServiceCloseError> {
        self.make_inert();
        let mut connection_failed = false;
        for state in self.connection_states.drain(..) {
            connection_failed |= state.dispose_after_adoption_failure().is_err();
        }
        for attachment in self.inert_attachments.drain(..) {
            connection_failed |= attachment.dispose_after_adoption_failure().is_err();
        }
        #[cfg(test)]
        if let Some(checkout) = self.adversarial_checkout.as_mut() {
            connection_failed |= checkout.dispose_adversarial_connections_for_test();
        }
        if self.connections.is_empty()
            && let Some(inventory) = self.inventory.as_ref()
        {
            for connection in inventory.retained_service_connection_slice() {
                connection_failed |= connection
                    .make_adoption_inert_in_place(self.cut)
                    .dispose_after_adoption_failure()
                    .is_err();
                connection_failed |= connection
                    .dispose_inert_driver_after_adoption_failure()
                    .is_err();
            }
        }
        for connection in self.connections.iter().chain(&self.rejected_connections) {
            connection_failed |= connection
                .make_adoption_inert_in_place(self.cut)
                .dispose_after_adoption_failure()
                .is_err();
            connection_failed |= connection
                .dispose_inert_driver_after_adoption_failure()
                .is_err();
        }
        let mut service_failure = None;
        if let Some(replacement) = self.replacement.as_mut()
            && let Some(service) = replacement.service.as_mut()
        {
            service_failure = service.dispose_unpublished_inert().err();
        }
        if let Some(service) = self.adopted_service.as_mut() {
            let adopted_failure = service.dispose_unpublished_inert().err();
            if service_failure.is_none() {
                service_failure = adopted_failure;
            }
        }
        if connection_failed {
            Err(ProjectionConnectionServiceCloseError::ConnectionShutdown)
        } else if let Some(error) = service_failure {
            Err(error)
        } else {
            Ok(())
        }
    }
}

impl ConnectionAdoptionState {
    fn dispose_after_adoption_failure(self) -> Result<(), ProjectionCoordinatorError> {
        match self {
            Self::Stable | Self::Vacant => Ok(()),
            Self::Prepared(prepared) => prepared.dispose_after_adoption_failure(),
            Self::PreparationFailed(failure) => {
                failure.dispose_after_adoption_failure();
                Ok(())
            }
            Self::Parked(prepared, parked) => {
                drop(parked);
                prepared.dispose_after_adoption_failure()
            }
            Self::ParkFailed(prepared, failure) => {
                drop(failure);
                prepared.dispose_after_adoption_failure()
            }
            Self::Bound(bound) => bound.dispose_after_adoption_failure(),
            Self::BindFailed(failure) => failure.dispose_after_adoption_failure(),
            Self::Joined(bound, stopped) => {
                drop(stopped);
                bound.dispose_after_adoption_failure()
            }
            Self::JoinFailed(bound, failure) => {
                let replacement_result = bound.dispose_after_adoption_failure();
                let old_result = failure.dispose_after_adoption_failure();
                replacement_result.and(old_result)
            }
            Self::Adopted(adopted) => adopted.dispose_after_adoption_failure(),
        }
    }
}

impl Drop for AdoptedUnpublishedProjectionConnectionService {
    fn drop(&mut self) {
        if let Some(attempt) = self.attempt.as_mut() {
            attempt.make_inert();
        }
    }
}

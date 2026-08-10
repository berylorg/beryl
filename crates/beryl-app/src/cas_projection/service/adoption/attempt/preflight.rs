use std::sync::Arc;

use beryl_home_store::HomeHealthState;

use super::super::super::super::connection::ConnectionEpochIdentity;
use super::{
    ConnectionAdoptionState, PersistentFailureServiceAdoptionReason, ServiceAdoptionAttempt,
};

impl ServiceAdoptionAttempt {
    pub(super) fn snapshot_retained_connections(
        &mut self,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        let inventory = self
            .inventory
            .as_ref()
            .expect("an active adoption attempt retains its quarantine inventory");
        #[cfg(test)]
        {
            let fail = {
                let mut slot = super::super::FAIL_FIRST_INERT_RESERVATION
                    .get_or_init(|| std::sync::Mutex::new(None))
                    .lock()
                    .expect("the first inert-reservation test hook is usable");
                if *slot == Some(self.metadata.home_id) {
                    slot.take();
                    true
                } else {
                    false
                }
            };
            if fail {
                return Err(PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable);
            }
        }
        self.inert_attachments
            .try_reserve_exact(self.metadata.connection_count)
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.inert_attachment_limit = Some(self.metadata.connection_count);
        self.connections
            .try_reserve_exact(self.metadata.connection_count)
            .map_err(|_| PersistentFailureServiceAdoptionReason::ReplacementCapacityUnavailable)?;
        self.connections.extend(
            inventory
                .retained_service_connection_slice()
                .iter()
                .cloned(),
        );
        if self.connections.len() != self.metadata.connection_count {
            return Err(PersistentFailureServiceAdoptionReason::ConnectionSetMismatch);
        }
        Ok(())
    }

    pub(super) fn validate_service_pair(
        &self,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        if !self.quarantine_promotable {
            return Err(PersistentFailureServiceAdoptionReason::QuarantineNotPromotable);
        }
        let inventory = self
            .inventory
            .as_ref()
            .expect("an active adoption attempt retains its quarantine inventory");
        let replacement = self
            .replacement
            .as_ref()
            .expect("an active adoption attempt retains its replacement service");
        let service = replacement.service();
        let home = service
            .home
            .as_ref()
            .ok_or(PersistentFailureServiceAdoptionReason::HomeInstanceMismatch)?;
        if !Arc::ptr_eq(inventory.retained_home(), home) {
            return Err(PersistentFailureServiceAdoptionReason::HomeInstanceMismatch);
        }
        if inventory.retained_home().canonical_identity() != home.canonical_identity()
            || inventory.retained_home().canonical_path() != home.canonical_path()
            || inventory.home_id() != service.home_id()
        {
            return Err(PersistentFailureServiceAdoptionReason::HomeIdentityMismatch);
        }
        if service.home_generation() <= inventory.home_generation() {
            return Err(PersistentFailureServiceAdoptionReason::HomeGenerationNotNewer);
        }
        let health = home.health();
        if health.state() != HomeHealthState::Healthy
            || health.generation() != Some(service.home_generation())
        {
            return Err(PersistentFailureServiceAdoptionReason::ReplacementHomeNotHealthy);
        }
        if service.service_generation() <= inventory.service_generation() {
            return Err(PersistentFailureServiceAdoptionReason::ServiceGenerationNotNewer);
        }
        if service.config != inventory.retained_service_config() {
            return Err(PersistentFailureServiceAdoptionReason::ServiceConfigMismatch);
        }
        if !replacement.startup_gate().is_closed() {
            return Err(PersistentFailureServiceAdoptionReason::ReplacementStartupFenceUnavailable);
        }
        if !service.command_authorizer.is_open() {
            return Err(PersistentFailureServiceAdoptionReason::ReplacementCommandGateClosed);
        }
        if !inventory.old_gate_matches_cut() {
            return Err(PersistentFailureServiceAdoptionReason::OldCommandGateMismatch);
        }
        Ok(())
    }

    pub(super) fn checkout_topology(
        &mut self,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        let checkout = self
            .inventory
            .as_ref()
            .expect("an active adoption attempt retains its quarantine inventory")
            .checkout_pending_quarantine_for_adoption()
            .map_err(PersistentFailureServiceAdoptionReason::QuarantineCheckout)?;
        #[cfg(test)]
        let mut checkout = checkout;
        #[cfg(not(test))]
        let (topology, fence) = checkout.into_parts();
        #[cfg(test)]
        let (mut topology, fence) = checkout.take_parts();
        #[cfg(test)]
        let adversarial_result =
            checkout.apply_adversarial_topology_for_test(&mut topology, self.cut);
        self.topology = Some(topology);
        self.fence = Some(fence);
        #[cfg(test)]
        if checkout.adversary_was_applied_for_test() {
            self.adversarial_checkout = Some(checkout);
        }
        #[cfg(test)]
        adversarial_result.map_err(PersistentFailureServiceAdoptionReason::QuarantineCheckout)?;
        Ok(())
    }

    pub(super) fn park_and_bind_drivers(
        &mut self,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        for index in 0..self.connections.len() {
            let park = self.connections[index].park_driver_for_adoption(self.cut);
            let state = std::mem::replace(
                &mut self.connection_states[index],
                ConnectionAdoptionState::Vacant,
            );
            match (state, park) {
                (ConnectionAdoptionState::Prepared(prepared), Ok(parked)) => {
                    self.connection_states[index] =
                        ConnectionAdoptionState::Parked(prepared, parked);
                }
                (ConnectionAdoptionState::Prepared(prepared), Err(failure)) => {
                    self.connection_states[index] =
                        ConnectionAdoptionState::ParkFailed(prepared, failure);
                    return Err(PersistentFailureServiceAdoptionReason::DriverPark);
                }
                (state, _) => {
                    self.connection_states[index] = state;
                    return Err(PersistentFailureServiceAdoptionReason::ConnectionPreparation);
                }
            }
        }
        for index in 0..self.connections.len() {
            let state = std::mem::replace(
                &mut self.connection_states[index],
                ConnectionAdoptionState::Vacant,
            );
            let ConnectionAdoptionState::Parked(prepared, parked) = state else {
                self.connection_states[index] = state;
                return Err(PersistentFailureServiceAdoptionReason::ConnectionPreparation);
            };
            match prepared.bind_parked_driver(parked) {
                Ok(bound) => self.connection_states[index] = ConnectionAdoptionState::Bound(bound),
                Err(failure) => {
                    self.connection_states[index] = ConnectionAdoptionState::BindFailed(failure);
                    return Err(PersistentFailureServiceAdoptionReason::DriverBind);
                }
            }
        }
        Ok(())
    }

    pub(super) fn join_old_ingesters(
        &mut self,
    ) -> Result<(), PersistentFailureServiceAdoptionReason> {
        let old_identity = ConnectionEpochIdentity::new(
            self.cut.home_id,
            self.cut.home_generation,
            self.cut.service_generation,
        );
        for index in 0..self.connections.len() {
            let joined =
                self.connections[index].join_old_ingester_for_adoption(old_identity, self.cut);
            let state = std::mem::replace(
                &mut self.connection_states[index],
                ConnectionAdoptionState::Vacant,
            );
            let ConnectionAdoptionState::Bound(bound) = state else {
                self.connection_states[index] = state;
                return Err(PersistentFailureServiceAdoptionReason::ConnectionPreparation);
            };
            match joined {
                Ok(stopped) => {
                    self.connection_states[index] = ConnectionAdoptionState::Joined(bound, stopped)
                }
                Err(failure) => {
                    self.connection_states[index] =
                        ConnectionAdoptionState::JoinFailed(bound, failure);
                    return Err(PersistentFailureServiceAdoptionReason::OldIngesterJoin);
                }
            }
        }
        Ok(())
    }
}

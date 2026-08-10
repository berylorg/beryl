use super::{
    ConnectionAdoptionState, PersistentFailureServiceAdoptionError,
    ReplacementResourceFailureSelectorForTest,
};

/// Content-free ownership diagnostics for one injected replacement resource failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct ReplacementResourceFailureOwnershipDiagnosticsForTest {
    prepared_connection_count: usize,
    preparation_failure_count: usize,
    inert_attachment_count: usize,
    replacement_worker_count: usize,
    selector_consumed: bool,
    startup_fence_never_opened: bool,
    startup_fence_cancelled: bool,
    broker_spawn_resources_retained: bool,
}

/// Content-free exact-owner diagnostics for one adversarial topology rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct AdversarialTopologyOwnershipDiagnosticsForTest {
    reached_owner_count: usize,
    retained_topology_owner_count: usize,
    reached_connection_count: usize,
    inert_attachment_count: usize,
    connection_state_count: usize,
    secondary_inventory_reescrow_disarmed: bool,
    all_reached_connections_inert: bool,
    startup_fence_never_opened: bool,
    startup_fence_cancelled: bool,
    publication_committed: bool,
}

impl PersistentFailureServiceAdoptionError {
    pub(in crate::cas_projection) fn replacement_resource_failure_diagnostics_for_test(
        &self,
    ) -> ReplacementResourceFailureOwnershipDiagnosticsForTest {
        let attempt = self
            .attempt
            .as_ref()
            .expect("an adoption error retains its inert owning attempt");
        let prepared_connection_count = attempt
            .connection_states
            .iter()
            .filter(|state| matches!(state, ConnectionAdoptionState::Prepared(..)))
            .count();
        let preparation_failure_count = attempt
            .connection_states
            .iter()
            .filter(|state| matches!(state, ConnectionAdoptionState::PreparationFailed(..)))
            .count();
        let broker_spawn_resources_retained = attempt.connection_states.iter().any(|state| {
            let ConnectionAdoptionState::PreparationFailed(failure) = state else {
                return false;
            };
            failure.broker_spawn_resources_retained_for_test()
        });
        let replacement = attempt
            .replacement
            .as_ref()
            .expect("a pre-commit failure retains the dormant replacement service");
        ReplacementResourceFailureOwnershipDiagnosticsForTest {
            prepared_connection_count,
            preparation_failure_count,
            inert_attachment_count: attempt.inert_attachments.len(),
            replacement_worker_count: replacement.service().worker_pool_diagnostics().active(),
            selector_consumed: attempt
                .replacement_resource_failure
                .as_ref()
                .is_some_and(ReplacementResourceFailureSelectorForTest::is_consumed),
            startup_fence_never_opened: !replacement.startup_gate().is_open_for_test(),
            startup_fence_cancelled: replacement.startup_gate().is_cancelled_for_test(),
            broker_spawn_resources_retained,
        }
    }

    pub(in crate::cas_projection) fn adversarial_topology_diagnostics_for_test(
        &self,
    ) -> AdversarialTopologyOwnershipDiagnosticsForTest {
        let attempt = self
            .attempt
            .as_ref()
            .expect("an adoption error retains its inert owning attempt");
        let checkout = attempt
            .adversarial_checkout
            .as_ref()
            .expect("an adversarial rejection retains its consumed checkout payload");
        let replacement = attempt
            .replacement
            .as_ref()
            .expect("a pre-commit rejection retains the dormant replacement service");
        let primary_owner_count = attempt
            .topology
            .as_ref()
            .map_or(0, |topology| topology.connection_owner_count());
        let primary_connections_inert = attempt
            .connections
            .iter()
            .chain(&attempt.rejected_connections)
            .all(|connection| connection.forwarding_epoch_is_inert_and_detached_for_test());
        AdversarialTopologyOwnershipDiagnosticsForTest {
            reached_owner_count: checkout.reached_owner_count_for_test(),
            retained_topology_owner_count: primary_owner_count
                .checked_add(checkout.retained_owner_count_for_test())
                .expect("bounded adversarial owner counts fit in memory"),
            reached_connection_count: attempt
                .connections
                .len()
                .checked_add(checkout.secondary_connection_count_for_test())
                .expect("bounded adversarial connection counts fit in memory"),
            inert_attachment_count: attempt
                .inert_attachments
                .len()
                .checked_add(checkout.secondary_inert_attachment_count_for_test())
                .expect("bounded inert attachment counts fit in memory"),
            connection_state_count: attempt.connection_states.len(),
            secondary_inventory_reescrow_disarmed: checkout
                .secondary_inventory_reescrow_is_disarmed_for_test(),
            all_reached_connections_inert: primary_connections_inert
                && checkout.secondary_connections_are_inert_for_test(),
            startup_fence_never_opened: !replacement.startup_gate().is_open_for_test(),
            startup_fence_cancelled: replacement.startup_gate().is_cancelled_for_test(),
            publication_committed: attempt.publication_committed,
        }
    }
}

impl ReplacementResourceFailureOwnershipDiagnosticsForTest {
    pub(in crate::cas_projection) const fn prepared_connection_count(self) -> usize {
        self.prepared_connection_count
    }

    pub(in crate::cas_projection) const fn preparation_failure_count(self) -> usize {
        self.preparation_failure_count
    }

    pub(in crate::cas_projection) const fn inert_attachment_count(self) -> usize {
        self.inert_attachment_count
    }

    pub(in crate::cas_projection) const fn replacement_worker_count(self) -> usize {
        self.replacement_worker_count
    }

    pub(in crate::cas_projection) const fn selector_consumed(self) -> bool {
        self.selector_consumed
    }

    pub(in crate::cas_projection) const fn startup_fence_never_opened(self) -> bool {
        self.startup_fence_never_opened
    }

    pub(in crate::cas_projection) const fn startup_fence_cancelled(self) -> bool {
        self.startup_fence_cancelled
    }

    pub(in crate::cas_projection) const fn broker_spawn_resources_retained(self) -> bool {
        self.broker_spawn_resources_retained
    }
}

impl AdversarialTopologyOwnershipDiagnosticsForTest {
    pub(in crate::cas_projection) const fn reached_owner_count(self) -> usize {
        self.reached_owner_count
    }

    pub(in crate::cas_projection) const fn retained_topology_owner_count(self) -> usize {
        self.retained_topology_owner_count
    }

    pub(in crate::cas_projection) const fn reached_connection_count(self) -> usize {
        self.reached_connection_count
    }

    pub(in crate::cas_projection) const fn inert_attachment_count(self) -> usize {
        self.inert_attachment_count
    }

    pub(in crate::cas_projection) const fn connection_state_count(self) -> usize {
        self.connection_state_count
    }

    pub(in crate::cas_projection) const fn secondary_inventory_reescrow_disarmed(self) -> bool {
        self.secondary_inventory_reescrow_disarmed
    }

    pub(in crate::cas_projection) const fn all_reached_connections_inert(self) -> bool {
        self.all_reached_connections_inert
    }

    pub(in crate::cas_projection) const fn startup_fence_never_opened(self) -> bool {
        self.startup_fence_never_opened
    }

    pub(in crate::cas_projection) const fn startup_fence_cancelled(self) -> bool {
        self.startup_fence_cancelled
    }

    pub(in crate::cas_projection) const fn publication_committed(self) -> bool {
        self.publication_committed
    }
}

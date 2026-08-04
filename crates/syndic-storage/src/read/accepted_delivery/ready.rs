use beryl_home_store::HomeStore;
use beryl_model::{
    AcceptedInputRevision, CasLoadedSessionGeneration, ExecutionBinding, InputGateRevision,
    SyndicAcceptedInputId,
};

use crate::{
    AcceptedInputLifecycle, AcceptedInputRecord, AcceptedRouteGenerationHeadRecord,
    AcceptedRouteHeadProof, AcceptedRouteLeafRecord, AcceptedRouteLeafState, AcceptedRouteTarget,
    BindingHeadRecord, InputGateRecord, InputGateState, SteeringTargetProof, SyndicReadError,
    codec::*, domain::SyndicStorage,
};

use super::super::SyndicPointReadLimit;
use super::validation::{
    input_leaf_identity_agrees, is_ready, validate_execution, validate_ready_route,
};

const OPERATION: &str = "ready-steering-input read";

/// One exact currently ready accepted input and its live CAS steering authority.
///
/// The view retains only fixed-size records and proofs. Its accepted-input revision is the
/// revision of the exact route leaf; the immutable [`AcceptedInputRecord`] has no mutable revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicReadySteeringInput {
    input: AcceptedInputRecord,
    accepted_input_revision: AcceptedInputRevision,
    lifecycle: AcceptedInputLifecycle,
    gate_revision: InputGateRevision,
    route: AcceptedRouteHeadProof,
    target: SteeringTargetProof,
    execution: ExecutionBinding,
    loaded_generation: CasLoadedSessionGeneration,
}

impl SyndicReadySteeringInput {
    /// Returns the immutable admitted input and its replayable content authority.
    #[must_use]
    pub const fn input(&self) -> &AcceptedInputRecord {
        &self.input
    }

    /// Returns the exact current delivery-leaf revision.
    #[must_use]
    pub const fn accepted_input_revision(&self) -> AcceptedInputRevision {
        self.accepted_input_revision
    }

    /// Returns whether the ready leaf is newly admitted or explicitly retryable.
    #[must_use]
    pub const fn lifecycle(&self) -> AcceptedInputLifecycle {
        self.lifecycle
    }

    /// Returns the exact stabilized input-gate revision.
    #[must_use]
    pub const fn gate_revision(&self) -> InputGateRevision {
        self.gate_revision
    }

    /// Returns the selected route generation and revision.
    #[must_use]
    pub const fn route(&self) -> AcceptedRouteHeadProof {
        self.route
    }

    /// Returns the exact Syndic turn and CAS thread/turn steering target.
    #[must_use]
    pub const fn target(&self) -> &SteeringTargetProof {
        &self.target
    }

    /// Returns the exact runtime, root, and runtime-native root path.
    #[must_use]
    pub const fn execution(&self) -> &ExecutionBinding {
        &self.execution
    }

    /// Returns the exact managed-process and loaded-thread generation.
    #[must_use]
    pub const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.loaded_generation
    }
}

impl SyndicStorage {
    /// Resolves one exact currently ready steering input without scanning route pages.
    ///
    /// `limit` applies independently to every constituent point read. An eligible result uses
    /// twelve point reads and stabilizes the input gate, selected route head, and binding head with
    /// first/last reads. A missing input or a stable non-ready/non-current input returns `None`;
    /// inconsistent durable relationships fail as [`SyndicReadError::Invariant`].
    pub fn ready_steering_input(
        &self,
        store: &HomeStore,
        input_id: SyndicAcceptedInputId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicReadySteeringInput>, SyndicReadError> {
        let Some(input) = self.ready_point::<AcceptedInputsFamily>(store, input_id, limit)? else {
            return Ok(None);
        };
        let thread = input.thread_id();
        let Some(gate) = self.ready_point::<InputGatesFamily>(store, thread, limit)? else {
            return self.ready_missing_gate(store, thread, limit);
        };
        if !matches!(gate.state(), InputGateState::Steerable(_)) {
            return self.ready_stable_none(store, &gate, limit);
        }
        let Some(route_proof) = gate.selected_route() else {
            return self.ready_gate_invariant(
                store,
                &gate,
                limit,
                "steerable input gate has no selected route",
            );
        };
        if route_proof.generation() != input.route_generation() {
            return self.ready_stable_none(store, &gate, limit);
        }

        let Some(leaf) = self.ready_point::<AcceptedRouteLeavesFamily>(store, input_id, limit)?
        else {
            return self.ready_gate_invariant(
                store,
                &gate,
                limit,
                "selected accepted input is missing its route leaf",
            );
        };
        if !input_leaf_identity_agrees(&input, &leaf) {
            return self.ready_gate_invariant(
                store,
                &gate,
                limit,
                "accepted input and exact route leaf disagree",
            );
        }
        if !is_ready(leaf.lifecycle()) {
            return self.ready_stable_none(store, &gate, limit);
        }
        if leaf.state() != AcceptedRouteLeafState::Routed {
            return self.ready_stable_none(store, &gate, limit);
        }

        self.resolve_ready_steering(store, input, leaf, gate, route_proof, limit)
    }
}

impl SyndicStorage {
    fn resolve_ready_steering(
        &self,
        store: &HomeStore,
        input: AcceptedInputRecord,
        leaf: AcceptedRouteLeafRecord,
        gate: InputGateRecord,
        route_proof: AcceptedRouteHeadProof,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicReadySteeringInput>, SyndicReadError> {
        let Some(route_head) = self.ready_point::<AcceptedRouteGenerationHeadsFamily>(
            store,
            input.thread_id(),
            limit,
        )?
        else {
            return self.ready_route_invariant(
                store,
                &gate,
                None,
                limit,
                "selected accepted route head is missing",
            );
        };
        let Some(generation) = self.ready_point::<AcceptedRouteGenerationsFamily>(
            store,
            ThreadRouteKey {
                thread: input.thread_id(),
                generation: route_proof.generation(),
            },
            limit,
        )?
        else {
            return self.ready_route_invariant(
                store,
                &gate,
                Some(&route_head),
                limit,
                "selected accepted route generation is missing",
            );
        };
        let target = match generation.target() {
            AcceptedRouteTarget::Steering(target) => target.clone(),
            _ => {
                return self.ready_route_invariant(
                    store,
                    &gate,
                    Some(&route_head),
                    limit,
                    "steerable input gate selected a non-steering route",
                );
            }
        };

        let Some(binding_head) =
            self.ready_point::<BindingHeadsFamily>(store, input.thread_id(), limit)?
        else {
            return self.ready_binding_invariant(
                store,
                &gate,
                &route_head,
                None,
                limit,
                "ready steering target has no current binding",
            );
        };
        let binding = self.ready_point::<BindingsFamily>(
            store,
            BindingKey {
                thread: input.thread_id(),
                revision: binding_head.revision(),
            },
            limit,
        )?;
        let snapshot = self.ready_point::<ExecutionSnapshotsFamily>(
            store,
            target.pending().snapshot_id(),
            limit,
        )?;
        let active_turn =
            self.ready_point::<ActiveCasTurnsFamily>(store, target.pending().snapshot_id(), limit)?;

        let confirmed_route_head = self.ready_point::<AcceptedRouteGenerationHeadsFamily>(
            store,
            input.thread_id(),
            limit,
        )?;
        let confirmed_binding_head =
            self.ready_point::<BindingHeadsFamily>(store, input.thread_id(), limit)?;
        let confirmed_gate =
            self.ready_point::<InputGatesFamily>(store, input.thread_id(), limit)?;
        if confirmed_route_head.as_ref() != Some(&route_head)
            || confirmed_binding_head.as_ref() != Some(&binding_head)
            || confirmed_gate.as_ref() != Some(&gate)
        {
            return Err(concurrent());
        }

        let binding = binding.ok_or(SyndicReadError::Invariant(
            "current binding head selects a missing binding",
        ))?;
        let snapshot = snapshot.ok_or(SyndicReadError::Invariant(
            "ready steering target is missing its execution snapshot",
        ))?;
        let active_turn = active_turn.ok_or(SyndicReadError::Invariant(
            "ready steering target is missing its active CAS-turn publication",
        ))?;
        validate_ready_route(&input, &leaf, &gate, &route_head, &generation, &target)?;
        validate_execution(
            input.thread_id(),
            gate.revision(),
            &target,
            &binding_head,
            &binding,
            &snapshot,
            &active_turn,
        )?;

        Ok(Some(SyndicReadySteeringInput {
            accepted_input_revision: leaf.revision(),
            lifecycle: leaf.lifecycle(),
            gate_revision: gate.revision(),
            route: route_proof,
            execution: snapshot.execution().clone(),
            loaded_generation: snapshot.loaded_generation(),
            input,
            target,
        }))
    }

    fn ready_point<F: Family>(
        &self,
        store: &HomeStore,
        key: F::Key,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<F::Value>, SyndicReadError> {
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_ready_steering_point_read();
        self.point::<F>(store, key, limit)
    }

    fn ready_missing_gate(
        &self,
        store: &HomeStore,
        thread: beryl_model::SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicReadySteeringInput>, SyndicReadError> {
        if self
            .ready_point::<InputGatesFamily>(store, thread, limit)?
            .is_some()
        {
            Err(concurrent())
        } else {
            Err(SyndicReadError::Invariant(
                "accepted input owner is missing its input gate",
            ))
        }
    }

    fn ready_stable_none(
        &self,
        store: &HomeStore,
        gate: &InputGateRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicReadySteeringInput>, SyndicReadError> {
        let confirmed = self.ready_point::<InputGatesFamily>(store, gate.thread_id(), limit)?;
        if confirmed.as_ref() != Some(gate) {
            return Err(concurrent());
        }
        Ok(None)
    }

    fn ready_gate_invariant(
        &self,
        store: &HomeStore,
        gate: &InputGateRecord,
        limit: SyndicPointReadLimit,
        message: &'static str,
    ) -> Result<Option<SyndicReadySteeringInput>, SyndicReadError> {
        let confirmed = self.ready_point::<InputGatesFamily>(store, gate.thread_id(), limit)?;
        if confirmed.as_ref() != Some(gate) {
            return Err(concurrent());
        }
        Err(SyndicReadError::Invariant(message))
    }

    fn ready_route_invariant(
        &self,
        store: &HomeStore,
        gate: &InputGateRecord,
        route_head: Option<&AcceptedRouteGenerationHeadRecord>,
        limit: SyndicPointReadLimit,
        message: &'static str,
    ) -> Result<Option<SyndicReadySteeringInput>, SyndicReadError> {
        let confirmed_head =
            self.ready_point::<AcceptedRouteGenerationHeadsFamily>(store, gate.thread_id(), limit)?;
        let confirmed_gate =
            self.ready_point::<InputGatesFamily>(store, gate.thread_id(), limit)?;
        if confirmed_head.as_ref() != route_head || confirmed_gate.as_ref() != Some(gate) {
            return Err(concurrent());
        }
        Err(SyndicReadError::Invariant(message))
    }

    fn ready_binding_invariant(
        &self,
        store: &HomeStore,
        gate: &InputGateRecord,
        route_head: &AcceptedRouteGenerationHeadRecord,
        binding_head: Option<&BindingHeadRecord>,
        limit: SyndicPointReadLimit,
        message: &'static str,
    ) -> Result<Option<SyndicReadySteeringInput>, SyndicReadError> {
        let confirmed_route_head =
            self.ready_point::<AcceptedRouteGenerationHeadsFamily>(store, gate.thread_id(), limit)?;
        let confirmed_binding_head =
            self.ready_point::<BindingHeadsFamily>(store, gate.thread_id(), limit)?;
        let confirmed_gate =
            self.ready_point::<InputGatesFamily>(store, gate.thread_id(), limit)?;
        if confirmed_route_head.as_ref() != Some(route_head)
            || confirmed_binding_head.as_ref() != binding_head
            || confirmed_gate.as_ref() != Some(gate)
        {
            return Err(concurrent());
        }
        Err(SyndicReadError::Invariant(message))
    }
}

fn concurrent() -> SyndicReadError {
    SyndicReadError::ConcurrentChange {
        operation: OPERATION,
    }
}

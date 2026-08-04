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

use super::SyndicPointReadLimit;

mod validation;

use validation::{input_leaf_identity_agrees, validate_execution, validate_route};

const OPERATION: &str = "delivering-steering-input read";

/// One exact currently delivering accepted input and its live CAS steering authority.
///
/// The view retains only fixed-size records and proofs. Its accepted-input revision is the
/// revision of the exact route leaf; the immutable [`AcceptedInputRecord`] has no mutable revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyndicDeliveringSteeringInput {
    input: AcceptedInputRecord,
    accepted_input_revision: AcceptedInputRevision,
    gate_revision: InputGateRevision,
    route: AcceptedRouteHeadProof,
    target: SteeringTargetProof,
    execution: ExecutionBinding,
    loaded_generation: CasLoadedSessionGeneration,
}

impl SyndicDeliveringSteeringInput {
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
    /// Resolves one exact currently delivering steering input without scanning route pages.
    ///
    /// `limit` applies independently to every constituent point read. An eligible result uses
    /// twelve point reads and stabilizes the input gate, selected route head, and binding head with
    /// first/last reads. A missing input or a stable non-delivering/non-current input returns
    /// `None`; inconsistent durable relationships fail as [`SyndicReadError::Invariant`].
    pub fn delivering_steering_input(
        &self,
        store: &HomeStore,
        input_id: SyndicAcceptedInputId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicDeliveringSteeringInput>, SyndicReadError> {
        let Some(input) = self.delivering_point::<AcceptedInputsFamily>(store, input_id, limit)?
        else {
            return Ok(None);
        };
        let thread = input.thread_id();
        let Some(gate) = self.delivering_point::<InputGatesFamily>(store, thread, limit)? else {
            return self.missing_gate(store, thread, limit);
        };
        if !matches!(gate.state(), InputGateState::Steerable(_)) {
            return self.stable_none(store, &gate, limit);
        }
        let Some(route_proof) = gate.selected_route() else {
            return self.gate_invariant(
                store,
                &gate,
                limit,
                "steerable input gate has no selected route",
            );
        };
        if route_proof.generation() != input.route_generation() {
            return self.stable_none(store, &gate, limit);
        }

        let Some(leaf) =
            self.delivering_point::<AcceptedRouteLeavesFamily>(store, input_id, limit)?
        else {
            return self.gate_invariant(
                store,
                &gate,
                limit,
                "selected accepted input is missing its route leaf",
            );
        };
        if !input_leaf_identity_agrees(&input, &leaf) {
            return self.gate_invariant(
                store,
                &gate,
                limit,
                "accepted input and exact route leaf disagree",
            );
        }
        if leaf.lifecycle() != AcceptedInputLifecycle::Delivering {
            return self.stable_none(store, &gate, limit);
        }
        if leaf.state() != AcceptedRouteLeafState::Routed {
            return self.gate_invariant(
                store,
                &gate,
                limit,
                "delivering accepted input is not routed",
            );
        }

        self.resolve_delivering_steering(store, input, leaf, gate, route_proof, limit)
    }
}

impl SyndicStorage {
    fn resolve_delivering_steering(
        &self,
        store: &HomeStore,
        input: AcceptedInputRecord,
        leaf: AcceptedRouteLeafRecord,
        gate: InputGateRecord,
        route_proof: AcceptedRouteHeadProof,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicDeliveringSteeringInput>, SyndicReadError> {
        let Some(route_head) = self.delivering_point::<AcceptedRouteGenerationHeadsFamily>(
            store,
            input.thread_id(),
            limit,
        )?
        else {
            return self.route_invariant(
                store,
                &gate,
                None,
                limit,
                "selected accepted route head is missing",
            );
        };
        let Some(generation) = self.delivering_point::<AcceptedRouteGenerationsFamily>(
            store,
            ThreadRouteKey {
                thread: input.thread_id(),
                generation: route_proof.generation(),
            },
            limit,
        )?
        else {
            return self.route_invariant(
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
                return self.route_invariant(
                    store,
                    &gate,
                    Some(&route_head),
                    limit,
                    "steerable input gate selected a non-steering route",
                );
            }
        };

        let Some(binding_head) =
            self.delivering_point::<BindingHeadsFamily>(store, input.thread_id(), limit)?
        else {
            return self.binding_invariant(
                store,
                &gate,
                &route_head,
                None,
                limit,
                "delivering steering target has no current binding",
            );
        };
        let binding = self.delivering_point::<BindingsFamily>(
            store,
            BindingKey {
                thread: input.thread_id(),
                revision: binding_head.revision(),
            },
            limit,
        )?;
        let snapshot = self.delivering_point::<ExecutionSnapshotsFamily>(
            store,
            target.pending().snapshot_id(),
            limit,
        )?;
        let active_turn = self.delivering_point::<ActiveCasTurnsFamily>(
            store,
            target.pending().snapshot_id(),
            limit,
        )?;

        let confirmed_route_head = self.delivering_point::<AcceptedRouteGenerationHeadsFamily>(
            store,
            input.thread_id(),
            limit,
        )?;
        let confirmed_binding_head =
            self.delivering_point::<BindingHeadsFamily>(store, input.thread_id(), limit)?;
        let confirmed_gate =
            self.delivering_point::<InputGatesFamily>(store, input.thread_id(), limit)?;
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
            "delivering steering target is missing its execution snapshot",
        ))?;
        let active_turn = active_turn.ok_or(SyndicReadError::Invariant(
            "delivering steering target is missing its active CAS-turn publication",
        ))?;
        validate_route(&input, &leaf, &gate, &route_head, &generation, &target)?;
        validate_execution(
            input.thread_id(),
            gate.revision(),
            &target,
            &binding_head,
            &binding,
            &snapshot,
            &active_turn,
        )?;

        Ok(Some(SyndicDeliveringSteeringInput {
            accepted_input_revision: leaf.revision(),
            gate_revision: gate.revision(),
            route: route_proof,
            execution: snapshot.execution().clone(),
            loaded_generation: snapshot.loaded_generation(),
            input,
            target,
        }))
    }

    fn delivering_point<F: Family>(
        &self,
        store: &HomeStore,
        key: F::Key,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<F::Value>, SyndicReadError> {
        #[cfg(feature = "test-faults")]
        crate::test_faults::metrics::record_delivering_steering_point_read();
        self.point::<F>(store, key, limit)
    }

    fn missing_gate(
        &self,
        store: &HomeStore,
        thread: beryl_model::SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicDeliveringSteeringInput>, SyndicReadError> {
        if self
            .delivering_point::<InputGatesFamily>(store, thread, limit)?
            .is_some()
        {
            Err(concurrent())
        } else {
            Err(SyndicReadError::Invariant(
                "accepted input owner is missing its input gate",
            ))
        }
    }

    fn stable_none(
        &self,
        store: &HomeStore,
        gate: &InputGateRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SyndicDeliveringSteeringInput>, SyndicReadError> {
        let confirmed =
            self.delivering_point::<InputGatesFamily>(store, gate.thread_id(), limit)?;
        if confirmed.as_ref() != Some(gate) {
            return Err(concurrent());
        }
        Ok(None)
    }

    fn gate_invariant(
        &self,
        store: &HomeStore,
        gate: &InputGateRecord,
        limit: SyndicPointReadLimit,
        message: &'static str,
    ) -> Result<Option<SyndicDeliveringSteeringInput>, SyndicReadError> {
        let confirmed =
            self.delivering_point::<InputGatesFamily>(store, gate.thread_id(), limit)?;
        if confirmed.as_ref() != Some(gate) {
            return Err(concurrent());
        }
        Err(SyndicReadError::Invariant(message))
    }

    fn route_invariant(
        &self,
        store: &HomeStore,
        gate: &InputGateRecord,
        route_head: Option<&AcceptedRouteGenerationHeadRecord>,
        limit: SyndicPointReadLimit,
        message: &'static str,
    ) -> Result<Option<SyndicDeliveringSteeringInput>, SyndicReadError> {
        let confirmed_head = self.delivering_point::<AcceptedRouteGenerationHeadsFamily>(
            store,
            gate.thread_id(),
            limit,
        )?;
        let confirmed_gate =
            self.delivering_point::<InputGatesFamily>(store, gate.thread_id(), limit)?;
        if confirmed_head.as_ref() != route_head || confirmed_gate.as_ref() != Some(gate) {
            return Err(concurrent());
        }
        Err(SyndicReadError::Invariant(message))
    }

    fn binding_invariant(
        &self,
        store: &HomeStore,
        gate: &InputGateRecord,
        route_head: &AcceptedRouteGenerationHeadRecord,
        binding_head: Option<&BindingHeadRecord>,
        limit: SyndicPointReadLimit,
        message: &'static str,
    ) -> Result<Option<SyndicDeliveringSteeringInput>, SyndicReadError> {
        let confirmed_route_head = self.delivering_point::<AcceptedRouteGenerationHeadsFamily>(
            store,
            gate.thread_id(),
            limit,
        )?;
        let confirmed_binding_head =
            self.delivering_point::<BindingHeadsFamily>(store, gate.thread_id(), limit)?;
        let confirmed_gate =
            self.delivering_point::<InputGatesFamily>(store, gate.thread_id(), limit)?;
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

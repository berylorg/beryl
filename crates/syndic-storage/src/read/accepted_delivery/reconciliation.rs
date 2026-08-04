use beryl_home_store::HomeStore;
use beryl_model::{AcceptedInputRevision, SyndicAcceptedInputId, SyndicThreadId};

use crate::{
    AcceptedInputLifecycle, AcceptedInputRecord, AcceptedRouteLeafRecord, AcceptedRouteLeafState,
    AcceptedRouteLeafTransitionKind, AcceptedRouteLeafTransitionProof, AcceptedRouteLostTarget,
    AcceptedRouteTarget, BeginAcceptedInputDelivery, CompleteAcceptedInputDelivery,
    RetryAcceptedInputDelivery, SteeringRejection, SteeringTargetProof, SyndicReadError, codec::*,
    domain::SyndicStorage,
};

use super::super::SyndicPointReadLimit;
use super::validation::{input_leaf_identity_agrees, is_ready};

/// Exact post-commit classification for one accepted-input delivery transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptedInputDeliveryTransitionStatus {
    /// The exact source leaf and all current route authorities still admit the transition.
    Prior,
    /// The exact successor leaf proves that the transition committed atomically.
    Exact,
    /// Durable state proves neither the exact prior authority nor the exact successor.
    Collision,
}

#[derive(Clone, Copy)]
enum TransitionKind {
    Begin,
    Retry,
    Complete,
    Rejected,
}

#[derive(Clone, Copy)]
struct TransitionRequest<'a> {
    thread: SyndicThreadId,
    input: SyndicAcceptedInputId,
    expected_input_revision: AcceptedInputRevision,
    target: &'a SteeringTargetProof,
    kind: TransitionKind,
}

impl SyndicStorage {
    /// Reconciles an ambiguously surfaced delivery claim.
    pub fn begin_accepted_input_delivery_status(
        &self,
        store: &HomeStore,
        request: &BeginAcceptedInputDelivery,
        limit: SyndicPointReadLimit,
    ) -> Result<AcceptedInputDeliveryTransitionStatus, SyndicReadError> {
        self.accepted_input_delivery_transition_status(
            store,
            TransitionRequest {
                thread: request.thread_id(),
                input: request.input_id(),
                expected_input_revision: request.expected_input_revision(),
                target: request.target(),
                kind: TransitionKind::Begin,
            },
            limit,
        )
    }

    /// Reconciles an ambiguously surfaced proven-pre-dispatch retry.
    pub fn retry_accepted_input_delivery_status(
        &self,
        store: &HomeStore,
        request: &RetryAcceptedInputDelivery,
        limit: SyndicPointReadLimit,
    ) -> Result<AcceptedInputDeliveryTransitionStatus, SyndicReadError> {
        self.accepted_input_delivery_transition_status(
            store,
            TransitionRequest {
                thread: request.thread_id(),
                input: request.input_id(),
                expected_input_revision: request.expected_input_revision(),
                target: request.target(),
                kind: TransitionKind::Retry,
            },
            limit,
        )
    }

    /// Reconciles an ambiguously surfaced successful steering completion.
    pub fn complete_accepted_input_delivery_status(
        &self,
        store: &HomeStore,
        request: &CompleteAcceptedInputDelivery,
        limit: SyndicPointReadLimit,
    ) -> Result<AcceptedInputDeliveryTransitionStatus, SyndicReadError> {
        self.accepted_input_delivery_transition_status(
            store,
            TransitionRequest {
                thread: request.thread_id(),
                input: request.input_id(),
                expected_input_revision: request.expected_input_revision(),
                target: request.target(),
                kind: TransitionKind::Complete,
            },
            limit,
        )
    }

    /// Reconciles an ambiguously surfaced closed structured steering rejection.
    pub fn steering_rejection_status(
        &self,
        store: &HomeStore,
        request: &SteeringRejection,
        limit: SyndicPointReadLimit,
    ) -> Result<AcceptedInputDeliveryTransitionStatus, SyndicReadError> {
        self.accepted_input_delivery_transition_status(
            store,
            TransitionRequest {
                thread: request.thread_id(),
                input: request.input_id(),
                expected_input_revision: request.expected_input_revision(),
                target: request.target(),
                kind: TransitionKind::Rejected,
            },
            limit,
        )
    }
}

impl SyndicStorage {
    fn accepted_input_delivery_transition_status(
        &self,
        store: &HomeStore,
        request: TransitionRequest<'_>,
        limit: SyndicPointReadLimit,
    ) -> Result<AcceptedInputDeliveryTransitionStatus, SyndicReadError> {
        let input = self.point::<AcceptedInputsFamily>(store, request.input, limit)?;
        let leaf = self.point::<AcceptedRouteLeavesFamily>(store, request.input, limit)?;
        let (Some(input), Some(leaf)) = (input, leaf) else {
            return Ok(AcceptedInputDeliveryTransitionStatus::Collision);
        };
        if input.thread_id() != request.thread || !input_leaf_identity_agrees(&input, &leaf) {
            return Ok(AcceptedInputDeliveryTransitionStatus::Collision);
        }

        let next_revision = request
            .expected_input_revision
            .checked_next()
            .map_err(|_| {
                SyndicReadError::Invariant(
                    "accepted-input delivery reconciliation revision frontier is exhausted",
                )
            })?;
        let (next_state, next_lifecycle) = request.kind.successor();
        let direct_successor = leaf.revision() == next_revision
            && leaf.state() == next_state
            && leaf.lifecycle() == next_lifecycle;
        let promoted_successor = leaf.promotion().is_some_and(|promotion| {
            leaf.lifecycle() == AcceptedInputLifecycle::Promoted
                && promotion.expected_input_revision() == next_revision
        });
        if let Some(proof) = leaf.last_transition()
            && (direct_successor || promoted_successor)
            && request.matches_transition_proof(proof)
        {
            return self.classify_stable_exact_authority(store, request, &input, proof, limit);
        }
        if leaf.revision() != request.expected_input_revision || !request.kind.admits_prior(&leaf) {
            return Ok(AcceptedInputDeliveryTransitionStatus::Collision);
        }

        self.classify_stable_prior_authority(store, request, &input, limit)
    }

    fn classify_stable_exact_authority(
        &self,
        store: &HomeStore,
        request: TransitionRequest<'_>,
        input: &AcceptedInputRecord,
        proof: AcceptedRouteLeafTransitionProof,
        limit: SyndicPointReadLimit,
    ) -> Result<AcceptedInputDeliveryTransitionStatus, SyndicReadError> {
        let generation = self.point::<AcceptedRouteGenerationsFamily>(
            store,
            ThreadRouteKey {
                thread: request.thread,
                generation: input.route_generation(),
            },
            limit,
        )?;
        let Some(generation) = generation else {
            return Ok(AcceptedInputDeliveryTransitionStatus::Collision);
        };
        Ok(
            if proof.expected_route().generation() == input.route_generation()
                && proof.expected_route().revision() < generation.revision()
                && generation.thread_id() == request.thread
                && generation.generation() == input.route_generation()
                && route_preserves_target(generation.target(), request.target)
            {
                AcceptedInputDeliveryTransitionStatus::Exact
            } else {
                AcceptedInputDeliveryTransitionStatus::Collision
            },
        )
    }

    fn classify_stable_prior_authority(
        &self,
        store: &HomeStore,
        request: TransitionRequest<'_>,
        input: &AcceptedInputRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<AcceptedInputDeliveryTransitionStatus, SyndicReadError> {
        let gate = self.point::<InputGatesFamily>(store, request.thread, limit)?;
        let generation = self.point::<AcceptedRouteGenerationsFamily>(
            store,
            ThreadRouteKey {
                thread: request.thread,
                generation: input.route_generation(),
            },
            limit,
        )?;
        let (Some(gate), Some(generation)) = (gate, generation) else {
            return Ok(AcceptedInputDeliveryTransitionStatus::Collision);
        };
        let Some(proof) = gate.selected_route() else {
            return Ok(AcceptedInputDeliveryTransitionStatus::Collision);
        };
        let target = match generation.target() {
            AcceptedRouteTarget::Steering(target) => target,
            _ => return Ok(AcceptedInputDeliveryTransitionStatus::Collision),
        };
        let gate_turn = match gate.state() {
            crate::InputGateState::Steerable(turn) => *turn,
            _ => return Ok(AcceptedInputDeliveryTransitionStatus::Collision),
        };
        let in_interval = generation
            .first_ordinal()
            .zip(generation.last_ordinal())
            .is_some_and(|(first, last)| first <= input.ordinal() && input.ordinal() <= last);
        let source_count = match request.kind {
            TransitionKind::Begin => generation.ready_retryable_count(),
            TransitionKind::Retry | TransitionKind::Complete | TransitionKind::Rejected => {
                generation.delivering_count()
            }
        };
        Ok(
            if gate.thread_id() == request.thread
                && input.admission_gate_revision() < gate.revision()
                && gate.live_steering_count() > 0
                && proof.generation() == input.route_generation()
                && generation.thread_id() == request.thread
                && generation.generation() == input.route_generation()
                && source_count > 0
                && in_interval
                && target == request.target
                && target.pending().active_turn_id() == gate_turn
            {
                AcceptedInputDeliveryTransitionStatus::Prior
            } else {
                AcceptedInputDeliveryTransitionStatus::Collision
            },
        )
    }
}

impl TransitionKind {
    fn admits_prior(self, leaf: &AcceptedRouteLeafRecord) -> bool {
        if leaf.state() != AcceptedRouteLeafState::Routed {
            return false;
        }
        match self {
            Self::Begin => is_ready(leaf.lifecycle()),
            Self::Retry | Self::Complete | Self::Rejected => {
                matches!(leaf.lifecycle(), AcceptedInputLifecycle::Delivering)
            }
        }
    }

    const fn successor(self) -> (AcceptedRouteLeafState, AcceptedInputLifecycle) {
        match self {
            Self::Begin => (
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Delivering,
            ),
            Self::Retry => (
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Retryable,
            ),
            Self::Complete => (
                AcceptedRouteLeafState::Routed,
                AcceptedInputLifecycle::Delivered,
            ),
            Self::Rejected => (
                AcceptedRouteLeafState::NextTurn(crate::NextTurnReason::SteeringRejected),
                AcceptedInputLifecycle::Retryable,
            ),
        }
    }

    const fn persisted(self) -> AcceptedRouteLeafTransitionKind {
        match self {
            Self::Begin => AcceptedRouteLeafTransitionKind::Begin,
            Self::Retry => AcceptedRouteLeafTransitionKind::Retry,
            Self::Complete => AcceptedRouteLeafTransitionKind::Complete,
            Self::Rejected => AcceptedRouteLeafTransitionKind::SteeringRejected,
        }
    }
}

impl TransitionRequest<'_> {
    fn matches_transition_proof(self, proof: AcceptedRouteLeafTransitionProof) -> bool {
        proof.expected_input_revision() == self.expected_input_revision
            && proof.kind() == self.kind.persisted()
    }
}

fn route_preserves_target(route: &AcceptedRouteTarget, expected: &SteeringTargetProof) -> bool {
    match route {
        AcceptedRouteTarget::Steering(target) => target == expected,
        AcceptedRouteTarget::AwaitingTerminal(target) => target == expected,
        AcceptedRouteTarget::ProjectionLost(lost) => {
            matches!(
                lost.prior_target(),
                AcceptedRouteLostTarget::Steering(target)
                    | AcceptedRouteLostTarget::AwaitingTerminal(target)
                    if target == expected
            )
        }
        AcceptedRouteTarget::AwaitingSteering(_) | AcceptedRouteTarget::NextTurn(_) => false,
    }
}

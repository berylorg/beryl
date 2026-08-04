use super::*;
use crate::{
    AcceptedInputLifecycle, AcceptedInputRecord, AcceptedRouteGenerationRecord,
    AcceptedRouteHeadProof, AcceptedRouteLeafRecord, AcceptedRouteLeafState,
    AcceptedRouteLeafTransitionKind, AcceptedRouteLeafTransitionProof, AcceptedRouteLostTarget,
    AcceptedRouteTarget, ActiveCasBinding, ActiveCasTurnRecord, BindingState,
    ExecutionSnapshotRecord, InputGateRecord, InputGateState, NextTurnReason,
    PendingSteeringTargetProof, SteeringTargetProof,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct AbandonmentLeafSnapshot {
    input: Option<AcceptedInputRecord>,
    leaf: Option<AcceptedRouteLeafRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AbandonmentRouteSnapshot {
    gate: Option<InputGateRecord>,
    generation: Option<AcceptedRouteGenerationRecord>,
    rejected: Option<AbandonmentLeafSnapshot>,
}

impl SyndicStorage {
    /// Reconciles one valid-binding publication through its immutable next revision.
    pub fn valid_binding_publication_status(
        &self,
        store: &HomeStore,
        request: &PublishValidBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let revision = next_binding_revision(request.expected_binding_revision())?;
        let usable = UsableCasBinding::new(
            request.execution().clone(),
            request.cas_thread_id().clone(),
            request.represented_prefix(),
            request.native_turn_count(),
            request.tool_profile(),
            request.lineage(),
        );
        let status = self.classify_binding_publication(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            BindingRecord::new(
                request.thread_id(),
                revision,
                request.selected_path(),
                BindingState::valid(usable),
            ),
            limit,
        )?;
        self.classify_cas_thread_reservation(
            store,
            CasThreadReservationPublication {
                status,
                thread: request.thread_id(),
                cas_thread: request.cas_thread_id(),
                revision,
                stale: false,
            },
            limit,
        )
    }

    /// Reconciles one stale-binding publication through its immutable next revision.
    pub fn stale_binding_publication_status(
        &self,
        store: &HomeStore,
        request: &PublishStaleBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let revision = next_binding_revision(request.expected_binding_revision())?;
        let status = self.classify_binding_publication(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            BindingRecord::new(
                request.thread_id(),
                revision,
                request.selected_path(),
                BindingState::stale(request.stale().clone()),
            ),
            limit,
        )?;
        self.classify_cas_thread_reservation(
            store,
            CasThreadReservationPublication {
                status,
                thread: request.thread_id(),
                cas_thread: request.stale().cas_thread_id(),
                revision,
                stale: true,
            },
            limit,
        )
    }

    /// Reconciles one atomic active-projection abandonment through its stale binding revision.
    pub fn abandoned_active_binding_publication_status(
        &self,
        store: &HomeStore,
        request: &AbandonActiveBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let binding_status = self.stale_binding_publication_status(
            store,
            &PublishStaleBinding::new(
                request.thread_id(),
                request.expected_binding_revision(),
                request.selected_path(),
                request.stale().clone(),
            ),
            limit,
        )?;
        if binding_status == BindingPublicationStatus::Collision {
            return Ok(BindingPublicationStatus::Collision);
        }
        self.classify_abandoned_active_route(store, request, binding_status, limit)
    }

    /// Reconciles one unbound-binding publication through its immutable next revision.
    pub fn unbound_binding_publication_status(
        &self,
        store: &HomeStore,
        request: &PublishUnboundBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let revision = next_binding_revision(request.expected_binding_revision())?;
        self.classify_binding_publication(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            BindingRecord::new(
                request.thread_id(),
                revision,
                request.selected_path(),
                request.state().clone(),
            ),
            limit,
        )
    }
}

fn abandonment_execution_matches(
    request: &AbandonActiveBinding,
    active: &ActiveCasBinding,
    snapshot: &ExecutionSnapshotRecord,
) -> bool {
    let usable = active.usable();
    snapshot.thread_id() == request.thread_id()
        && snapshot.id() == active.snapshot_id()
        && snapshot.binding_revision() == request.expected_binding_revision()
        && snapshot.active_turn_id() == active.turn_id()
        && snapshot.cas_thread_id() == usable.cas_thread_id()
        && snapshot.selected_path() == request.selected_path()
        && snapshot.represented_base_prefix() == usable.represented_prefix()
        && snapshot.represented_base_native_turn_count() == usable.native_turn_count()
        && snapshot.tool_profile() == usable.tool_profile()
        && snapshot.lineage() == usable.lineage()
        && snapshot.execution() == usable.execution()
        && request.stale().execution() == usable.execution()
        && request.stale().cas_thread_id() == usable.cas_thread_id()
        && request.stale().observed_tool_profile() == Some(usable.tool_profile())
        && request.stale().observed_prefix() == Some(usable.represented_prefix())
        && request.stale().observed_lineage() == Some(usable.lineage())
        && request.stale().observed_native_turn_count() == Some(usable.native_turn_count())
        && request.stale().loaded_generation() == Some(snapshot.loaded_generation())
        && request.stale().observed_at() >= active.started_at()
}

fn pending_target_matches(
    pending: &PendingSteeringTargetProof,
    request: &AbandonActiveBinding,
    active: &ActiveCasBinding,
) -> bool {
    pending.binding_revision() == request.expected_binding_revision()
        && pending.snapshot_id() == active.snapshot_id()
        && pending.active_turn_id() == active.turn_id()
        && pending.cas_thread_id() == active.usable().cas_thread_id()
}

fn steering_target_matches(
    target: &SteeringTargetProof,
    request: &AbandonActiveBinding,
    active: &ActiveCasBinding,
    active_turn: Option<&ActiveCasTurnRecord>,
) -> bool {
    pending_target_matches(target.pending(), request, active)
        && active_turn.is_some_and(|published| {
            published.snapshot_id() == active.snapshot_id()
                && published.thread_id() == request.thread_id()
                && published.turn_id() == active.turn_id()
                && published.binding_revision() == request.expected_binding_revision()
                && published.cas_thread_id() == active.usable().cas_thread_id()
                && published.cas_turn_id() == target.cas_turn_id()
        })
}

fn route_target_matches(
    target: &AcceptedRouteTarget,
    request: &AbandonActiveBinding,
    active: &ActiveCasBinding,
    active_turn: Option<&ActiveCasTurnRecord>,
) -> bool {
    match (target, request.target()) {
        (
            AcceptedRouteTarget::AwaitingSteering(current),
            AcceptedRouteLostTarget::AwaitingSteering(expected),
        ) => {
            request.exact_rejected_delivery().is_none()
                && current == expected
                && pending_target_matches(current, request, active)
        }
        (AcceptedRouteTarget::Steering(current), AcceptedRouteLostTarget::Steering(expected)) => {
            current == expected && steering_target_matches(current, request, active, active_turn)
        }
        (
            AcceptedRouteTarget::AwaitingTerminal(current),
            AcceptedRouteLostTarget::AwaitingTerminal(expected),
        ) => {
            request.exact_rejected_delivery().is_none()
                && current == expected
                && steering_target_matches(current, request, active, active_turn)
        }
        _ => false,
    }
}

fn abandonment_prior_matches(
    request: &AbandonActiveBinding,
    active: &ActiveCasBinding,
    active_turn: Option<&ActiveCasTurnRecord>,
    observed: &AbandonmentRouteSnapshot,
) -> bool {
    let (Some(gate), Some(generation)) = (&observed.gate, &observed.generation) else {
        return false;
    };
    let gate_turn_matches = match gate.state() {
        InputGateState::AwaitingSteering(turn)
        | InputGateState::Steerable(turn)
        | InputGateState::AwaitingTerminal(turn) => *turn == active.turn_id(),
        InputGateState::Idle
        | InputGateState::PendingTurn(_)
        | InputGateState::Compacting { .. }
        | InputGateState::Stopping { .. }
        | InputGateState::FinalizingHistory(_) => false,
    };
    let Some(route) = gate.selected_route() else {
        return false;
    };
    gate.thread_id() == request.thread_id()
        && route.generation() == request.route_generation()
        && gate_turn_matches
        && generation.thread_id() == request.thread_id()
        && generation.generation() == request.route_generation()
        && route_target_matches(generation.target(), request, active, active_turn)
        && rejected_leaf_matches(request, None, route, observed.rejected.as_ref(), false)
}

fn abandonment_exact_matches(
    request: &AbandonActiveBinding,
    active: &ActiveCasBinding,
    active_turn: Option<&ActiveCasTurnRecord>,
    observed: &AbandonmentRouteSnapshot,
) -> Result<bool, SyndicReadError> {
    let Some(generation) = &observed.generation else {
        return Ok(false);
    };
    let AcceptedRouteTarget::ProjectionLost(lost) = generation.target() else {
        return Ok(false);
    };
    let abandonment = lost.abandonment();
    let route_revision = abandonment
        .expected_route()
        .revision()
        .checked_next()
        .map_err(|_| {
            SyndicReadError::Invariant("abandonment reconciliation route frontier is exhausted")
        })?;
    Ok(generation.thread_id() == request.thread_id()
        && generation.generation() == request.route_generation()
        && route_revision <= generation.revision()
        && generation.ready_retryable_count() == 0
        && generation.delivering_count() == 0
        && generation.delivering_logical_utf8_bytes() == 0
        && projection_lost_target_matches(generation.target(), request, active, active_turn)?
        && rejected_leaf_matches(
            request,
            Some(abandonment.expected_gate_revision()),
            abandonment.expected_route(),
            observed.rejected.as_ref(),
            true,
        ))
}

fn rejected_leaf_matches(
    request: &AbandonActiveBinding,
    source_gate_revision: Option<beryl_model::InputGateRevision>,
    source_route: AcceptedRouteHeadProof,
    observed: Option<&AbandonmentLeafSnapshot>,
    published: bool,
) -> bool {
    let Some(expected) = request.exact_rejected_delivery() else {
        return observed.is_none();
    };
    let Some(AbandonmentLeafSnapshot {
        input: Some(input),
        leaf: Some(leaf),
    }) = observed
    else {
        return false;
    };
    let revision = if published {
        let Ok(revision) = expected.expected_input_revision().checked_next() else {
            return false;
        };
        revision
    } else {
        expected.expected_input_revision()
    };
    input.id() == expected.input_id()
        && input.thread_id() == request.thread_id()
        && input.route_generation() == request.route_generation()
        && leaf.input_id() == input.id()
        && leaf.thread_id() == input.thread_id()
        && leaf.generation() == input.route_generation()
        && leaf.ordinal() == input.ordinal()
        && leaf.revision() == revision
        && if published {
            leaf.state() == AcceptedRouteLeafState::NextTurn(NextTurnReason::ProjectionLost)
                && leaf.lifecycle() == AcceptedInputLifecycle::Retryable
                && leaf.last_transition()
                    == Some(AcceptedRouteLeafTransitionProof::new(
                        source_gate_revision
                            .expect("published abandonment supplies its source gate"),
                        source_route,
                        expected.expected_input_revision(),
                        AcceptedRouteLeafTransitionKind::ProjectionLostExactRejection,
                    ))
        } else {
            leaf.state() == AcceptedRouteLeafState::Routed
                && leaf.lifecycle() == AcceptedInputLifecycle::Delivering
        }
}

fn projection_lost_target_matches(
    target: &AcceptedRouteTarget,
    request: &AbandonActiveBinding,
    active: &ActiveCasBinding,
    active_turn: Option<&ActiveCasTurnRecord>,
) -> Result<bool, SyndicReadError> {
    let AcceptedRouteTarget::ProjectionLost(lost) = target else {
        return Ok(false);
    };
    let retirement_revision = request
        .expected_binding_revision()
        .checked_next()
        .map_err(|_| {
            SyndicReadError::Invariant("abandonment reconciliation binding frontier is exhausted")
        })?;
    let prior_matches = match (lost.prior_target(), request.target()) {
        (
            AcceptedRouteLostTarget::AwaitingSteering(current),
            AcceptedRouteLostTarget::AwaitingSteering(expected),
        ) => {
            request.exact_rejected_delivery().is_none()
                && current == expected
                && pending_target_matches(current, request, active)
        }
        (
            AcceptedRouteLostTarget::Steering(current),
            AcceptedRouteLostTarget::Steering(expected),
        ) => current == expected && steering_target_matches(current, request, active, active_turn),
        (
            AcceptedRouteLostTarget::AwaitingTerminal(current),
            AcceptedRouteLostTarget::AwaitingTerminal(expected),
        ) => current == expected && steering_target_matches(current, request, active, active_turn),
        _ => false,
    };
    let abandonment = lost.abandonment();
    Ok(prior_matches
        && abandonment.expected_binding_revision() == request.expected_binding_revision()
        && abandonment.expected_route().generation() == request.route_generation()
        && abandonment.kind() == request.route_abandonment_kind()
        && lost.retirement_binding_revision() == retirement_revision
        && lost.snapshot_id() == active.snapshot_id()
        && lost.cas_thread_id() == active.usable().cas_thread_id())
}

impl SyndicStorage {
    fn classify_abandoned_active_route(
        &self,
        store: &HomeStore,
        request: &AbandonActiveBinding,
        binding_status: BindingPublicationStatus,
        limit: SyndicPointReadLimit,
    ) -> Result<BindingPublicationStatus, SyndicReadError> {
        let Some(prior_binding) = self.binding(
            store,
            request.thread_id(),
            request.expected_binding_revision(),
            limit,
        )?
        else {
            return Ok(BindingPublicationStatus::Collision);
        };
        let BindingState::Active(active) = prior_binding.state() else {
            return Ok(BindingPublicationStatus::Collision);
        };
        if prior_binding.selected_path() != request.selected_path() {
            return Ok(BindingPublicationStatus::Collision);
        }
        let Some(snapshot) = self.execution_snapshot(store, active.snapshot_id(), limit)? else {
            return Ok(BindingPublicationStatus::Collision);
        };
        if !abandonment_execution_matches(request, active, &snapshot) {
            return Ok(BindingPublicationStatus::Collision);
        }
        let active_turn = self.active_cas_turn(store, active.snapshot_id(), limit)?;

        let observed = self.abandonment_route_snapshot(store, request, limit)?;
        let exact = match binding_status {
            BindingPublicationStatus::Prior => {
                abandonment_prior_matches(request, active, active_turn.as_ref(), &observed)
            }
            BindingPublicationStatus::Exact => {
                abandonment_exact_matches(request, active, active_turn.as_ref(), &observed)?
            }
            BindingPublicationStatus::Collision => false,
        };
        Ok(if exact {
            binding_status
        } else {
            BindingPublicationStatus::Collision
        })
    }

    fn abandonment_route_snapshot(
        &self,
        store: &HomeStore,
        request: &AbandonActiveBinding,
        limit: SyndicPointReadLimit,
    ) -> Result<AbandonmentRouteSnapshot, SyndicReadError> {
        let rejected = match request.exact_rejected_delivery() {
            Some(rejected) => Some(AbandonmentLeafSnapshot {
                input: self.point::<AcceptedInputsFamily>(store, rejected.input_id(), limit)?,
                leaf: self.point::<AcceptedRouteLeavesFamily>(store, rejected.input_id(), limit)?,
            }),
            None => None,
        };
        Ok(AbandonmentRouteSnapshot {
            gate: self.point::<InputGatesFamily>(store, request.thread_id(), limit)?,
            generation: self.point::<AcceptedRouteGenerationsFamily>(
                store,
                ThreadRouteKey {
                    thread: request.thread_id(),
                    generation: request.route_generation(),
                },
                limit,
            )?,
            rejected,
        })
    }
}

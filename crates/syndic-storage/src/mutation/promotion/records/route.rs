use super::*;

pub(super) struct PromotionRouteRecords {
    pub(super) source_key: ThreadRouteKey,
    pub(super) route_head: Option<AcceptedRouteGenerationHeadRecord>,
    pub(super) generation: AcceptedRouteGenerationRecord,
    pub(super) leaf: AcceptedRouteLeafRecord,
    pub(super) next_source: Option<AcceptedNextSourceRecord>,
}

pub(super) fn validate_promotion_source(
    basis: &AcceptedNextCandidateBasis,
    promotion: &PromoteAcceptedInput,
) -> Result<(), SyndicMutationError> {
    let source = basis.source();
    let gate = basis.gate();
    let generation = basis.generation();
    let leaf = basis.leaf();
    let input = basis.input();
    let order = basis.order();
    let expected_reason = effective_next_reason(generation.target(), leaf)
        .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?;
    if !matches!(gate.state(), InputGateState::Idle)
        || gate.live_steering_count() != 0
        || gate.live_next_turn_count() == 0
        || promotion.promoted_at() < basis.summary().last_activity_at()
        || promotion.promoted_at() < input.admitted_at()
        || promotion.candidate().next_turn_reason() != expected_reason
        || source.thread_id() != basis.thread().id()
        || source.generation() != generation.generation()
        || source.generation_revision() != generation.revision()
        || source.first_ordinal()
            != generation
                .first_ordinal()
                .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?
        || source.last_ordinal()
            != generation
                .last_ordinal()
                .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?
        || generation.next_turn_count() == 0
        || input.id() != promotion.accepted_input_id()
        || input.thread_id() != basis.thread().id()
        || input.route_generation() != generation.generation()
        || input.ordinal() != order.ordinal()
        || order.input_id() != input.id()
        || order.thread_id() != input.thread_id()
        || order.route_generation() != generation.generation()
        || leaf.input_id() != input.id()
        || leaf.thread_id() != input.thread_id()
        || leaf.generation() != generation.generation()
        || leaf.ordinal() != input.ordinal()
        || leaf.promotion().is_some()
        || basis.draft_by_thread().thread_id() != basis.thread().id()
        || basis.draft_by_thread().draft_id() != basis.thread().current_draft_id()
        || basis.draft_by_thread().thread_revision() != basis.thread().revision()
        || basis.summary().thread_id() != basis.thread().id()
        || basis.summary().thread_revision() != basis.thread().revision()
        || basis.summary().committed_tail() != basis.thread().committed_tail()
        || basis.summary().selected_path_digest() != basis.thread().selected_path_digest()
        || basis.binding_head().thread_id() != basis.thread().id()
        || basis.binding().thread_id() != basis.thread().id()
        || basis.binding_head().revision() != basis.binding().revision()
        || basis.binding_head().lifecycle() != basis.binding().state().lifecycle()
        || basis.binding().selected_path().tail() != basis.thread().committed_tail()
        || basis.binding().selected_path().digest() != basis.thread().selected_path_digest()
        || basis.binding().selected_path().thread_revision() > basis.thread().revision()
        || matches!(basis.binding().state(), BindingState::Active(_))
    {
        return Err(SyndicMutationError::AcceptedInputPromotionConflict);
    }
    Ok(())
}

fn effective_next_reason(
    target: &AcceptedRouteTarget,
    leaf: &AcceptedRouteLeafRecord,
) -> Option<NextTurnReason> {
    if !matches!(
        leaf.lifecycle(),
        AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable
    ) {
        return None;
    }
    match leaf.state() {
        AcceptedRouteLeafState::NextTurn(reason) => Some(reason),
        AcceptedRouteLeafState::Routed => match target {
            AcceptedRouteTarget::NextTurn(reason) => Some(*reason),
            AcceptedRouteTarget::ProjectionLost(_) => Some(NextTurnReason::ProjectionLost),
            AcceptedRouteTarget::AwaitingTerminal(_) => Some(NextTurnReason::UnknownTerminal),
            AcceptedRouteTarget::AwaitingSteering(_) | AcceptedRouteTarget::Steering(_) => None,
        },
    }
}

pub(super) fn promotion_route_records(
    basis: &AcceptedNextCandidateBasis,
    promotion: &PromoteAcceptedInput,
) -> Result<PromotionRouteRecords, SyndicMutationError> {
    let current = basis.generation();
    let revision = current.revision().checked_next()?;
    let logical_bytes = basis.input().content().summary().logical_utf8_bytes();
    let next_turn_count = current
        .next_turn_count()
        .checked_sub(1)
        .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?;
    let terminal_count = current
        .terminal_count()
        .checked_add(1)
        .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?;
    let live_bytes = current
        .live_logical_utf8_bytes()
        .checked_sub(logical_bytes)
        .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?;
    let generation = AcceptedRouteGenerationRecord::new(
        current.thread_id(),
        current.generation(),
        revision,
        current.target().clone(),
        current.first_ordinal(),
        current.last_ordinal(),
        current.input_count(),
        current.ready_retryable_count(),
        current.delivering_count(),
        next_turn_count,
        terminal_count,
        live_bytes,
        current.delivering_logical_utf8_bytes(),
    )?;
    let proof = promotion.proof();
    let mut leaf = AcceptedRouteLeafRecord::new(
        basis.leaf().input_id(),
        basis.leaf().thread_id(),
        basis.leaf().generation(),
        basis.leaf().ordinal(),
        basis.leaf().revision().checked_next()?,
        AcceptedRouteLeafState::Routed,
        AcceptedInputLifecycle::Promoted,
    );
    if let Some(transition) = basis.leaf().last_transition() {
        leaf = leaf.with_transition_proof(transition);
    }
    leaf = leaf.with_promotion_proof(proof);
    let route_head = basis.route_head().and_then(|head| {
        (head.proof().generation() == current.generation()).then(|| {
            AcceptedRouteGenerationHeadRecord::new(
                current.thread_id(),
                AcceptedRouteHeadProof::new(current.generation(), revision),
            )
        })
    });
    let next_source = (next_turn_count > 0).then(|| {
        AcceptedNextSourceRecord::new(
            current.thread_id(),
            current.generation(),
            revision,
            current
                .first_ordinal()
                .expect("validated promotion generation is nonempty"),
            current
                .last_ordinal()
                .expect("validated promotion generation is nonempty"),
        )
    });
    Ok(PromotionRouteRecords {
        source_key: ThreadRouteKey {
            thread: current.thread_id(),
            generation: current.generation(),
        },
        route_head,
        generation,
        leaf,
        next_source,
    })
}

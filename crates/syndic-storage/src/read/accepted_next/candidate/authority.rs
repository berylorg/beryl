use super::*;

pub(super) fn validate_next_authority(
    expected: AcceptedNextSourceRecord,
    snapshot: &NextAuthoritySnapshot,
) -> Result<(), SyndicReadError> {
    let (Some(source), Some(gate), Some(generation)) =
        (&snapshot.source, &snapshot.gate, &snapshot.generation)
    else {
        return Err(SyndicReadError::Invariant(
            "accepted-next source is missing current route authority",
        ));
    };
    if source != &expected
        || gate.thread_id() != expected.thread_id()
        || generation.thread_id() != expected.thread_id()
        || generation.generation() != expected.generation()
        || generation.revision() != expected.generation_revision()
        || generation.first_ordinal() != Some(expected.first_ordinal())
        || generation.last_ordinal() != Some(expected.last_ordinal())
        || generation.next_turn_count() == 0
        || gate.accepted_high_water() < expected.last_ordinal().get()
        || gate
            .route_generation_high_water()
            .is_none_or(|high_water| high_water < expected.generation())
        || (gate.state() == &InputGateState::Idle && gate.live_steering_count() != 0)
        || gate.live_next_turn_count() < generation.next_turn_count()
        || gate.live_logical_utf8_bytes() < generation.live_logical_utf8_bytes()
    {
        return Err(SyndicReadError::Invariant(
            "accepted-next source, gate, and generation disagree",
        ));
    }
    if route_head_is_coherent(expected.thread_id(), gate, snapshot.head.as_ref()) {
        Ok(())
    } else {
        Err(SyndicReadError::Invariant(
            "accepted-next gate and current route head disagree",
        ))
    }
}

pub(super) fn route_head_is_coherent(
    thread_id: SyndicThreadId,
    gate: &InputGateRecord,
    head: Option<&AcceptedRouteGenerationHeadRecord>,
) -> bool {
    if head.is_some_and(|head| {
        head.thread_id() != thread_id
            || gate
                .route_generation_high_water()
                .is_none_or(|high_water| head.proof().generation() > high_water)
    }) {
        return false;
    }
    gate.selected_route()
        .is_none_or(|proof| head.is_some_and(|head| head.proof() == proof))
}

pub(super) fn validate_next_leaf(
    source: AcceptedNextSourceRecord,
    generation: &AcceptedRouteGenerationRecord,
    order: &AcceptedOrderIndexRecord,
    leaf: &AcceptedRouteLeafRecord,
) -> Result<(), SyndicReadError> {
    if order.thread_id() != source.thread_id()
        || order.route_generation() != source.generation()
        || leaf.input_id() != order.input_id()
        || leaf.thread_id() != source.thread_id()
        || leaf.generation() != source.generation()
        || leaf.ordinal() != order.ordinal()
        || (matches!(leaf.state(), AcceptedRouteLeafState::NextTurn(_))
            && leaf.lifecycle().is_terminal())
        || !route_member_is_coherent(generation, leaf)
    {
        return Err(SyndicReadError::Invariant(
            "accepted-next order and route leaf disagree",
        ));
    }
    Ok(())
}

fn route_member_is_coherent(
    generation: &AcceptedRouteGenerationRecord,
    leaf: &AcceptedRouteLeafRecord,
) -> bool {
    match (generation.target(), leaf.state(), leaf.lifecycle()) {
        (
            AcceptedRouteTarget::ProjectionLost(_),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Admitted
            | AcceptedInputLifecycle::Retryable
            | AcceptedInputLifecycle::Delivering,
        )
        | (
            AcceptedRouteTarget::NextTurn(_),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable,
        )
        | (
            AcceptedRouteTarget::AwaitingTerminal(_),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable,
        )
        | (
            _,
            AcceptedRouteLeafState::NextTurn(_),
            AcceptedInputLifecycle::Admitted | AcceptedInputLifecycle::Retryable,
        )
        | (
            AcceptedRouteTarget::AwaitingSteering(_) | AcceptedRouteTarget::Steering(_),
            AcceptedRouteLeafState::Routed,
            AcceptedInputLifecycle::Admitted
            | AcceptedInputLifecycle::Retryable
            | AcceptedInputLifecycle::Delivering,
        ) => true,
        (_, AcceptedRouteLeafState::Routed, lifecycle) => lifecycle.is_terminal(),
        _ => false,
    }
}

pub(super) fn effective_next_turn_reason(
    generation: &AcceptedRouteGenerationRecord,
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
        AcceptedRouteLeafState::Routed => match generation.target() {
            AcceptedRouteTarget::NextTurn(reason) => Some(*reason),
            AcceptedRouteTarget::ProjectionLost(_) => Some(NextTurnReason::ProjectionLost),
            AcceptedRouteTarget::AwaitingTerminal(_) => Some(NextTurnReason::UnknownTerminal),
            AcceptedRouteTarget::AwaitingSteering(_) | AcceptedRouteTarget::Steering(_) => None,
        },
    }
}

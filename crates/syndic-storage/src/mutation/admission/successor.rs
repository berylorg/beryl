use beryl_home_store::{
    FirstAcceptancePromotionProtocolV1, ReconciliationReader, SuccessorObservation, SuccessorSource,
};
use beryl_model::{AcceptedInputRevision, FirstAcceptancePromotionSuccessorV1};

use crate::{
    AcceptedInputLifecycle, AcceptedRouteLeafState,
    codec::{AcceptedInputsCodec, AcceptedOrderCodec, AcceptedRouteLeavesCodec},
    domain::SyndicDomain,
    error::SyndicValidationError,
};

#[derive(Clone, Copy)]
pub(super) struct FirstAcceptancePromotionSourceV1;

impl SuccessorSource<SyndicDomain, FirstAcceptancePromotionProtocolV1>
    for FirstAcceptancePromotionSourceV1
{
    const MAX_RETAINED_BYTES: usize = 1;

    fn authenticate(
        &self,
        reader: &ReconciliationReader<'_, SyndicDomain>,
    ) -> Result<SuccessorObservation<FirstAcceptancePromotionSuccessorV1>, SyndicValidationError>
    {
        let inputs = reader.records::<AcceptedInputsCodec>()?;
        let orders = reader.records::<AcceptedOrderCodec>()?;
        let leaves = reader.records::<AcceptedRouteLeavesCodec>()?;
        let ([input], [order], [leaf]) = (inputs.as_slice(), orders.as_slice(), leaves.as_slice())
        else {
            return Ok(SuccessorObservation::Collision);
        };
        let (None, Some(intended_input), Some(current_input)) =
            (input.old(), input.new(), input.current())
        else {
            return Ok(SuccessorObservation::Collision);
        };
        if intended_input != current_input {
            return Ok(SuccessorObservation::Collision);
        }
        let (None, Some(intended_order), Some(current_order)) =
            (order.old(), order.new(), order.current())
        else {
            return Ok(SuccessorObservation::Collision);
        };
        if intended_order != current_order
            || intended_order.thread_id() != intended_input.thread_id()
            || intended_order.ordinal() != intended_input.ordinal()
            || intended_order.input_id() != intended_input.id()
            || intended_order.route_generation() != intended_input.route_generation()
        {
            return Ok(SuccessorObservation::Collision);
        }
        let (None, Some(intended_leaf), Some(current_leaf)) =
            (leaf.old(), leaf.new(), leaf.current())
        else {
            return Ok(SuccessorObservation::Collision);
        };
        let Ok(initial_revision) = AcceptedInputRevision::new(1) else {
            return Ok(SuccessorObservation::Collision);
        };
        if intended_leaf.input_id() != intended_input.id()
            || intended_leaf.thread_id() != intended_input.thread_id()
            || intended_leaf.generation() != intended_input.route_generation()
            || intended_leaf.ordinal() != intended_input.ordinal()
            || intended_leaf.revision() != initial_revision
            || intended_leaf.lifecycle() != AcceptedInputLifecycle::Admitted
            || intended_leaf.last_transition().is_some()
            || intended_leaf.promotion().is_some()
        {
            return Ok(SuccessorObservation::Collision);
        }
        let Some(promotion) = current_leaf.promotion() else {
            return Ok(SuccessorObservation::Collision);
        };
        let Ok(promoted_revision) = intended_leaf.revision().checked_next() else {
            return Ok(SuccessorObservation::Collision);
        };
        if current_leaf.input_id() != intended_leaf.input_id()
            || current_leaf.thread_id() != intended_leaf.thread_id()
            || current_leaf.generation() != intended_leaf.generation()
            || current_leaf.ordinal() != intended_leaf.ordinal()
            || current_leaf.revision() != promoted_revision
            || current_leaf.state() != AcceptedRouteLeafState::Routed
            || current_leaf.lifecycle() != AcceptedInputLifecycle::Promoted
            || current_leaf.last_transition() != intended_leaf.last_transition()
            || promotion.expected_input_revision() != intended_leaf.revision()
            || promotion.expected_route().generation() != intended_leaf.generation()
        {
            return Ok(SuccessorObservation::Collision);
        }
        Ok(SuccessorObservation::Authenticated(
            FirstAcceptancePromotionSuccessorV1::new(
                intended_input.id(),
                promotion.successor_item_id(),
                intended_input.asset_reference_set(),
            ),
        ))
    }
}

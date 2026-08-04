use beryl_home_store::DomainReader;

use crate::{
    AcceptedInputLifecycle, AcceptedInputRecord, AcceptedRouteLeafRecord, AcceptedRouteLeafState,
    CanonicalItemKind, ConversationParent, TurnItemOrdinal, TurnKind, codec::*,
    domain::SyndicDomain, error::SyndicValidationError,
};

use super::super::scan::require;
use super::util::invariant;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
    input: &AcceptedInputRecord,
    leaf: &AcceptedRouteLeafRecord,
) -> Result<(), SyndicValidationError> {
    let Some(proof) = leaf.promotion() else {
        return if leaf.lifecycle() == AcceptedInputLifecycle::Promoted {
            invariant("promoted accepted-route leaf is missing its successor witness")
        } else {
            Ok(())
        };
    };
    if leaf.lifecycle() != AcceptedInputLifecycle::Promoted
        || leaf.state() != AcceptedRouteLeafState::Routed
        || proof.expected_route().generation() != leaf.generation()
        || proof.expected_input_revision().checked_next().ok() != Some(leaf.revision())
        || proof.promoted_at() < input.admitted_at()
    {
        return invariant("accepted-input promotion witness disagrees with its route leaf");
    }

    let gate = require::<InputGatesFamily>(
        reader,
        &input.thread_id(),
        "accepted-input promotion witness references a missing gate",
    )?;
    let generation = require::<AcceptedRouteGenerationsFamily>(
        reader,
        &ThreadRouteKey {
            thread: input.thread_id(),
            generation: leaf.generation(),
        },
        "accepted-input promotion witness references a missing generation",
    )?;
    if input.admission_gate_revision() >= proof.expected_gate_revision()
        || proof.expected_gate_revision() >= gate.revision()
        || proof.expected_route().revision() >= generation.revision()
    {
        return invariant("accepted-input promotion witness authority is not ancestral");
    }

    let turn = require::<TurnsFamily>(
        reader,
        &proof.successor_turn_id(),
        "accepted-input promotion successor turn is missing",
    )?;
    let state = require::<TurnStatesFamily>(
        reader,
        &proof.successor_turn_id(),
        "accepted-input promotion successor turn state is missing",
    )?;
    let item = require::<CanonicalItemsFamily>(
        reader,
        &proof.successor_item_id(),
        "accepted-input promotion successor item is missing",
    )?;
    let item_index = require::<TurnItemsFamily>(
        reader,
        &TurnItemKey {
            owner: proof.successor_turn_id(),
            ordinal: TurnItemOrdinal::FIRST,
        },
        "accepted-input promotion successor item membership is missing",
    )?;
    if turn.origin_thread_id() != input.thread_id()
        || turn.kind() != TurnKind::OrdinaryUser
        || turn.submitted_at() != proof.promoted_at()
        || state.turn_id() != turn.id()
        || item.id() != proof.successor_item_id()
        || item.turn_id() != turn.id()
        || item.ordinal() != TurnItemOrdinal::FIRST
        || item.kind() != CanonicalItemKind::UserInput
        || item.presentation_content() != Some(input.content())
        || item.presentation().asset_reference_set() != input.asset_reference_set()
        || item_index.turn_id() != turn.id()
        || item_index.ordinal() != TurnItemOrdinal::FIRST
        || item_index.item_id() != item.id()
        || item_index.item_revision() != item.revision()
    {
        return invariant("accepted-input promotion successor records disagree");
    }
    if let ConversationParent::Turn(parent) = turn.parent() {
        let child = require::<TurnChildrenFamily>(
            reader,
            &TurnPairKey {
                parent,
                child: turn.id(),
            },
            "accepted-input promotion successor child membership is missing",
        )?;
        if child.parent_id() != parent
            || child.child_id() != turn.id()
            || child.child_depth() != turn.depth()
            || child.child_digest() != turn.chain_digest()
        {
            return invariant("accepted-input promotion successor child membership disagrees");
        }
    }
    Ok(())
}

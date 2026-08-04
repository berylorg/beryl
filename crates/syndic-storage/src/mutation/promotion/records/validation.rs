use super::*;

pub(super) fn validate_current_basis(
    reader: &DomainReader<'_, SyndicDomain>,
    basis: &AcceptedNextCandidateBasis,
) -> Result<(), SyndicMutationError> {
    let route_key = ThreadRouteKey {
        thread: basis.source().thread_id(),
        generation: basis.source().generation(),
    };
    let order_key = ThreadAcceptedKey {
        owner: basis.order().thread_id(),
        ordinal: basis.order().ordinal(),
    };
    let binding_key = BindingKey {
        thread: basis.binding().thread_id(),
        revision: basis.binding().revision(),
    };
    let exact = point::<AcceptedNextSourcesFamily>(reader, &route_key)?.as_ref()
        == Some(basis.source())
        && point::<InputGatesFamily>(reader, &basis.gate().thread_id())?.as_ref()
            == Some(basis.gate())
        && point::<ThreadsFamily>(reader, &basis.thread().id())?.as_ref() == Some(basis.thread())
        && point::<DraftByThreadFamily>(reader, &basis.thread().id())?.as_ref()
            == Some(basis.draft_by_thread())
        && point::<AcceptedRouteGenerationHeadsFamily>(reader, &basis.thread().id())?.as_ref()
            == basis.route_head()
        && point::<AcceptedRouteGenerationsFamily>(reader, &route_key)?.as_ref()
            == Some(basis.generation())
        && point::<AcceptedRouteLeavesFamily>(reader, &basis.leaf().input_id())?.as_ref()
            == Some(basis.leaf())
        && point::<AcceptedInputsFamily>(reader, &basis.input().id())?.as_ref()
            == Some(basis.input())
        && point::<AcceptedOrderFamily>(reader, &order_key)?.as_ref() == Some(basis.order())
        && point::<BindingHeadsFamily>(reader, &basis.thread().id())?.as_ref()
            == Some(basis.binding_head())
        && point::<BindingsFamily>(reader, &binding_key)?.as_ref() == Some(basis.binding())
        && point::<TranscriptHeadsFamily>(reader, &basis.thread().id())?.as_ref()
            == Some(basis.transcript_head())
        && point::<HistorySummariesFamily>(reader, &basis.thread().id())?.as_ref()
            == Some(basis.summary())
        && point::<ActivityQueryHeadsFamily>(reader, &basis.thread().id())?.as_ref()
            == Some(basis.activity_head());
    if exact {
        Ok(())
    } else {
        Err(SyndicMutationError::AcceptedInputPromotionConflict)
    }
}

pub(super) fn validate_fresh_identities(
    reader: &DomainReader<'_, SyndicDomain>,
    basis: &AcceptedNextCandidateBasis,
    promotion: &PromoteAcceptedInput,
) -> Result<(), SyndicMutationError> {
    let turn_id = promotion.successor_turn_id();
    let item_id = promotion.successor_item_id();
    let raw_draft = SyndicDraftId::from_bytes(*turn_id.as_bytes());
    let raw_accepted = SyndicAcceptedInputId::from_bytes(*turn_id.as_bytes());
    if point::<TurnsFamily>(reader, &turn_id)?.is_some()
        || point::<TurnStatesFamily>(reader, &turn_id)?.is_some()
        || point::<DraftsFamily>(reader, &raw_draft)?.is_some()
        || point::<AcceptedInputsFamily>(reader, &raw_accepted)?.is_some()
        || point::<CanonicalItemsFamily>(reader, &item_id)?.is_some()
        || point::<TurnItemsFamily>(
            reader,
            &TurnItemKey {
                owner: turn_id,
                ordinal: TurnItemOrdinal::FIRST,
            },
        )?
        .is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    let parent = basis
        .thread()
        .committed_tail()
        .ok_or(SyndicMutationError::AcceptedInputPromotionConflict)?;
    if point::<TurnChildrenFamily>(
        reader,
        &TurnPairKey {
            parent,
            child: turn_id,
        },
    )?
    .is_some()
    {
        return Err(SyndicMutationError::AdmissionIdentityCollision);
    }
    required::<TurnsFamily>(reader, &parent)?;
    let current_state = required::<TurnStatesFamily>(reader, &parent)?;
    if !current_state.lifecycle().is_proven_terminal() {
        return Err(SyndicMutationError::AcceptedInputPromotionConflict);
    }
    Ok(())
}

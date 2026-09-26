use super::*;

#[derive(Default)]
struct FrontierMembers {
    floor: Option<DraftEditHistoryTransitionV1>,
    journal: Option<DraftEditHistoryTransitionV1>,
    undo: Option<DraftEditHistoryTransitionV1>,
    redo: Option<DraftEditHistoryTransitionV1>,
}

pub(super) fn authenticate_frontier_for_mutation(
    reader: &DomainReader<'_, SyndicDomain>,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<(), SyndicMutationError> {
    let mut members = FrontierMembers::default();
    read_mutation_floor(reader, frontier, &mut members.floor)?;
    read_mutation_members(reader, frontier, &mut members)?;
    authenticate_mutation_members(reader, frontier, &members)?;
    authenticate_mutation_retained_bytes(frontier, members.floor.as_ref())
}

fn read_mutation_floor(
    reader: &DomainReader<'_, SyndicDomain>,
    frontier: &DraftEditHistoryFrontierV1,
    floor: &mut Option<DraftEditHistoryTransitionV1>,
) -> Result<(), SyndicMutationError> {
    *floor = match frontier.oldest_eligible() {
        Some(reference) => Some(authenticated_transition_reference(reader, reference)?),
        None => None,
    };
    Ok(())
}

fn read_mutation_members(
    reader: &DomainReader<'_, SyndicDomain>,
    frontier: &DraftEditHistoryFrontierV1,
    members: &mut FrontierMembers,
) -> Result<(), SyndicMutationError> {
    for reference in [
        frontier.journal_head(),
        frontier.undo_head(),
        frontier.redo_head(),
    ]
    .into_iter()
    .flatten()
    {
        let value = authenticated_transition_reference(reader, reference)?;
        if members.floor.as_ref().is_some_and(|floor| {
            value.cumulative_encoded_bytes() < floor.cumulative_encoded_bytes()
        }) {
            return Err(SyndicMutationError::IdentityCollision);
        }
        authenticate_root_pin(reader, value.predecessor_root())?;
        authenticate_root_pin(reader, value.successor_root())?;
        if Some(reference) == frontier.journal_head()
            && value.successor_root() != frontier.reference().root()
        {
            return Err(SyndicMutationError::IdentityCollision);
        }
        if Some(reference) == frontier.journal_head() {
            members.journal = Some(value.clone());
        }
        if Some(reference) == frontier.undo_head() {
            members.undo = Some(value.clone());
        }
        if Some(reference) == frontier.redo_head() {
            members.redo = Some(value);
        }
    }
    Ok(())
}

fn authenticate_mutation_members(
    reader: &DomainReader<'_, SyndicDomain>,
    frontier: &DraftEditHistoryFrontierV1,
    members: &FrontierMembers,
) -> Result<(), SyndicMutationError> {
    let Some(head) = members.journal.as_ref() else {
        return Ok(());
    };
    for member in [
        members.floor.as_ref(),
        members.undo.as_ref(),
        members.redo.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if !transition_is_ancestor_of(reader, head, member)? {
            return Err(SyndicMutationError::IdentityCollision);
        }
    }
    if head.successor_root() != frontier.reference().root()
        || members
            .undo
            .as_ref()
            .is_some_and(|value| value.successor_root() != frontier.reference().root())
        || members
            .redo
            .as_ref()
            .is_some_and(|value| value.successor_root() != frontier.reference().root())
    {
        return Err(SyndicMutationError::IdentityCollision);
    }
    authenticate_mutation_stack(reader, frontier, head, members.floor.as_ref())
}

fn authenticate_mutation_stack(
    reader: &DomainReader<'_, SyndicDomain>,
    frontier: &DraftEditHistoryFrontierV1,
    head: &DraftEditHistoryTransitionV1,
    floor: Option<&DraftEditHistoryTransitionV1>,
) -> Result<(), SyndicMutationError> {
    match head.kind() {
        DraftEditHistoryTransitionKindV1::OrdinaryEdit => {
            if frontier.undo_head() != Some(head.reference()) || frontier.redo_head().is_some() {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        DraftEditHistoryTransitionKindV1::Undo => {
            let reference = head
                .prior_undo()
                .ok_or(SyndicMutationError::IdentityCollision)?;
            let selected = authenticated_transition_reference(reader, reference)?;
            if frontier.redo_head() != Some(head.reference())
                || frontier.undo_head() != retained_stack_link(floor, selected.prior_undo())
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
        DraftEditHistoryTransitionKindV1::Redo => {
            let reference = head
                .prior_redo()
                .ok_or(SyndicMutationError::IdentityCollision)?;
            let selected = authenticated_transition_reference(reader, reference)?;
            if frontier.undo_head() != Some(head.reference())
                || frontier.redo_head() != retained_stack_link(floor, selected.prior_redo())
            {
                return Err(SyndicMutationError::IdentityCollision);
            }
        }
    }
    Ok(())
}

fn authenticate_mutation_retained_bytes(
    frontier: &DraftEditHistoryFrontierV1,
    floor: Option<&DraftEditHistoryTransitionV1>,
) -> Result<(), SyndicMutationError> {
    let frontier_charge =
        stored_frontier_charge(frontier).map_err(|_| SyndicMutationError::IdentityCollision)?;
    let retained = match (frontier.journal_head(), floor) {
        (None, None) => frontier_charge,
        (Some(head), Some(floor)) => {
            if head.cumulative_encoded_bytes() != frontier.cumulative_encoded_bytes() {
                return Err(SyndicMutationError::IdentityCollision);
            }
            let floor_charge = stored_transition_charge(floor)
                .map_err(|_| SyndicMutationError::IdentityCollision)?;
            frontier_charge
                .checked_add(
                    frontier
                        .cumulative_encoded_bytes()
                        .checked_sub(
                            floor
                                .cumulative_encoded_bytes()
                                .checked_sub(floor_charge)
                                .ok_or(SyndicMutationError::IdentityCollision)?,
                        )
                        .ok_or(SyndicMutationError::IdentityCollision)?,
                )
                .ok_or(SyndicMutationError::IdentityCollision)?
        }
        _ => return Err(SyndicMutationError::IdentityCollision),
    };
    if retained != frontier.retained_encoded_bytes() || retained > frontier.byte_budget() {
        return Err(SyndicMutationError::IdentityCollision);
    }
    Ok(())
}

pub(super) fn authenticate_frontier(
    storage: &SyndicStorage,
    store: &HomeStore,
    frontier: &DraftEditHistoryFrontierV1,
) -> Result<bool, SyndicReadError> {
    let mut members = FrontierMembers::default();
    if !read_members(storage, store, frontier, &mut members)? {
        return Ok(false);
    }
    if !members_are_exact(storage, store, frontier, &members)? {
        return Ok(false);
    }
    retained_bytes_are_exact(frontier, members.floor.as_ref())
}

fn read_members(
    storage: &SyndicStorage,
    store: &HomeStore,
    frontier: &DraftEditHistoryFrontierV1,
    members: &mut FrontierMembers,
) -> Result<bool, SyndicReadError> {
    members.floor = match frontier.oldest_eligible() {
        Some(reference) => {
            match transition_reference_is_authenticated(storage, store, reference)? {
                Some(value) => Some(value),
                None => return Ok(false),
            }
        }
        None => None,
    };
    for reference in [
        frontier.journal_head(),
        frontier.undo_head(),
        frontier.redo_head(),
    ]
    .into_iter()
    .flatten()
    {
        let Some(value) = transition_reference_is_authenticated(storage, store, reference)? else {
            return Ok(false);
        };
        if members.floor.as_ref().is_some_and(|floor| {
            value.cumulative_encoded_bytes() < floor.cumulative_encoded_bytes()
        }) || Some(reference) == frontier.journal_head()
            && value.successor_root() != frontier.reference().root()
        {
            return Ok(false);
        }
        if Some(reference) == frontier.journal_head() {
            members.journal = Some(value.clone());
        }
        if Some(reference) == frontier.undo_head() {
            members.undo = Some(value.clone());
        }
        if Some(reference) == frontier.redo_head() {
            members.redo = Some(value);
        }
    }
    Ok(true)
}

fn members_are_exact(
    storage: &SyndicStorage,
    store: &HomeStore,
    frontier: &DraftEditHistoryFrontierV1,
    members: &FrontierMembers,
) -> Result<bool, SyndicReadError> {
    let Some(head) = members.journal.as_ref() else {
        return Ok(true);
    };
    for member in [
        members.floor.as_ref(),
        members.undo.as_ref(),
        members.redo.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if !transition_is_ancestor_of_read(storage, store, head, member)? {
            return Ok(false);
        }
    }
    if head.successor_root() != frontier.reference().root()
        || members
            .undo
            .as_ref()
            .is_some_and(|value| value.successor_root() != frontier.reference().root())
        || members
            .redo
            .as_ref()
            .is_some_and(|value| value.successor_root() != frontier.reference().root())
    {
        return Ok(false);
    }
    stack_is_exact(storage, store, frontier, head, members.floor.as_ref())
}

fn stack_is_exact(
    storage: &SyndicStorage,
    store: &HomeStore,
    frontier: &DraftEditHistoryFrontierV1,
    head: &DraftEditHistoryTransitionV1,
    floor: Option<&DraftEditHistoryTransitionV1>,
) -> Result<bool, SyndicReadError> {
    Ok(match head.kind() {
        DraftEditHistoryTransitionKindV1::OrdinaryEdit => {
            frontier.undo_head() == Some(head.reference()) && frontier.redo_head().is_none()
        }
        DraftEditHistoryTransitionKindV1::Undo => {
            let Some(reference) = head.prior_undo() else {
                return Ok(false);
            };
            let Some(selected) = transition_reference_is_authenticated(storage, store, reference)?
            else {
                return Ok(false);
            };
            frontier.redo_head() == Some(head.reference())
                && frontier.undo_head() == retained_stack_link(floor, selected.prior_undo())
        }
        DraftEditHistoryTransitionKindV1::Redo => {
            let Some(reference) = head.prior_redo() else {
                return Ok(false);
            };
            let Some(selected) = transition_reference_is_authenticated(storage, store, reference)?
            else {
                return Ok(false);
            };
            frontier.undo_head() == Some(head.reference())
                && frontier.redo_head() == retained_stack_link(floor, selected.prior_redo())
        }
    })
}

fn retained_bytes_are_exact(
    frontier: &DraftEditHistoryFrontierV1,
    floor: Option<&DraftEditHistoryTransitionV1>,
) -> Result<bool, SyndicReadError> {
    let Ok(frontier_charge) = stored_frontier_charge(frontier) else {
        return Ok(false);
    };
    let retained = match (frontier.journal_head(), floor) {
        (None, None) => frontier_charge,
        (Some(head), Some(floor)) => {
            if head.cumulative_encoded_bytes() != frontier.cumulative_encoded_bytes() {
                return Ok(false);
            }
            let Ok(floor_charge) = stored_transition_charge(floor) else {
                return Ok(false);
            };
            let Some(before_floor) = floor.cumulative_encoded_bytes().checked_sub(floor_charge)
            else {
                return Ok(false);
            };
            let Some(transition_bytes) = frontier
                .cumulative_encoded_bytes()
                .checked_sub(before_floor)
            else {
                return Ok(false);
            };
            let Some(retained) = frontier_charge.checked_add(transition_bytes) else {
                return Ok(false);
            };
            retained
        }
        _ => return Ok(false),
    };
    Ok(retained == frontier.retained_encoded_bytes() && retained <= frontier.byte_budget())
}

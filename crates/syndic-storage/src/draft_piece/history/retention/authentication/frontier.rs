use super::*;

#[derive(Default)]
struct FrontierMembers {
    floor: Option<DraftEditHistoryTransitionV1>,
    journal: Option<DraftEditHistoryTransitionV1>,
    undo: Option<DraftEditHistoryTransitionV1>,
    redo: Option<DraftEditHistoryTransitionV1>,
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

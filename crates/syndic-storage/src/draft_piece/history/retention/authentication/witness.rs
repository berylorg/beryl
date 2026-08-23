use beryl_home_store::{DomainReader, HomeStore};

use crate::domain::SyndicDomain;
use crate::draft_piece::history::witness::{
    DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS, ancestor_bitmap_for_depth,
};
use crate::draft_piece::{
    DraftEditHistoryAncestorWitnessV1, DraftEditHistoryTransitionReferenceV1,
    DraftEditHistoryTransitionV1,
};
use crate::{SyndicMutationError, SyndicReadError, SyndicStorage};

use super::{exact_transition_reference, transition_reference_is_exact};

pub(crate) fn build_ancestor_witness(
    reader: &DomainReader<'_, SyndicDomain>,
    prior: Option<DraftEditHistoryTransitionReferenceV1>,
) -> Result<(u64, DraftEditHistoryAncestorWitnessV1), SyndicMutationError> {
    let Some(prior) = prior else {
        return Ok((1, DraftEditHistoryAncestorWitnessV1::EMPTY));
    };
    let depth = prior
        .journal_depth()
        .checked_add(1)
        .ok_or(SyndicMutationError::IdentityCollision)?;
    let mut slots = [None; DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS];
    slots[0] = Some(prior);
    for level in 1..DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS {
        if 1_u64 << level >= depth {
            break;
        }
        let lower = exact_transition_reference(
            reader,
            slots[level - 1].ok_or(SyndicMutationError::IdentityCollision)?,
        )?;
        slots[level] = Some(
            lower
                .ancestor_witness()
                .ancestor(level - 1)
                .ok_or(SyndicMutationError::IdentityCollision)?,
        );
    }
    Ok((
        depth,
        DraftEditHistoryAncestorWitnessV1::from_parts(ancestor_bitmap_for_depth(depth), slots),
    ))
}

pub(crate) fn transition_is_ancestor_of(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &DraftEditHistoryTransitionV1,
    candidate: &DraftEditHistoryTransitionV1,
) -> Result<bool, SyndicMutationError> {
    let Some(mut difference) = head.journal_depth().checked_sub(candidate.journal_depth()) else {
        return Ok(false);
    };
    let mut current = head.clone();
    for level in 0..DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS {
        if difference & 1 != 0 {
            let Some(reference) = current.ancestor_witness().ancestor(level) else {
                return Ok(false);
            };
            current = exact_transition_reference(reader, reference)?;
        }
        difference >>= 1;
        if difference == 0 {
            break;
        }
    }
    Ok(current.reference() == candidate.reference())
}

pub(super) fn transition_is_ancestor_of_read(
    storage: &SyndicStorage,
    store: &HomeStore,
    head: &DraftEditHistoryTransitionV1,
    candidate: &DraftEditHistoryTransitionV1,
) -> Result<bool, SyndicReadError> {
    let Some(mut difference) = head.journal_depth().checked_sub(candidate.journal_depth()) else {
        return Ok(false);
    };
    let mut current = head.clone();
    for level in 0..DRAFT_EDIT_HISTORY_ANCESTOR_LEVELS {
        if difference & 1 != 0 {
            let Some(reference) = current.ancestor_witness().ancestor(level) else {
                return Ok(false);
            };
            let Some(next) = transition_reference_is_exact(storage, store, reference)? else {
                return Ok(false);
            };
            current = next;
        }
        difference >>= 1;
        if difference == 0 {
            break;
        }
    }
    Ok(current.reference() == candidate.reference())
}

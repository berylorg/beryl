use super::*;
use crate::draft_piece::history::{stored_frontier_charge, stored_transition_charge};

pub(super) fn terminal(
    reader: &mut OutcomeReader<'_>,
    selected: &CapturedState,
    settlement: &DraftPieceSettlementV1,
    current_history: &DraftEditHistoryFrontierV1,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    match settlement.closure() {
        DraftPieceSettlementClosureV1::Committed(adoption) => {
            if selected.session != *adoption.adopted_session()
                || current_history != adoption.adopted_history()
                || selected.session.newest_root() != adoption.adopted_root().reference()
            {
                return Err(invariant("adopted root/history/session"));
            }
            let transition = reader.required::<DraftEditHistoryTransitionsFamily>(
                adoption.transition().key(),
                "adopted history transition",
            )?;
            if &transition != adoption.transition() {
                return Err(invariant("adopted transition bytes"));
            }
            let source = adoption.predecessor_history();
            if source.reference().key() != current_history.reference().key() {
                let stored = reader.required::<DraftEditHistoryFrontiersFamily>(
                    source.reference().key(),
                    "predecessor history",
                )?;
                if &stored != source {
                    return Err(invariant("predecessor history bytes"));
                }
            }
            references::combined_root(reader, source.reference().root())?;
            retained_floor(reader, source, &transition, current_history)
        }
        DraftPieceSettlementClosureV1::Noncommit(noncommit) => {
            if selected.session != *noncommit.observed_session()
                || current_history != noncommit.observed_history()
            {
                return Err(invariant("noncommit observed history/session"));
            }
            if let Some(proof) = noncommit.occupied_identity() {
                let OccupiedIdentityDifferenceV1::Root { key, occupied, .. } = proof.difference()
                else {
                    return Err(invariant("occupied root evidence kind"));
                };
                if reader.point::<DraftPieceRootsFamily>(*key)?.as_ref() != Some(occupied) {
                    return Err(invariant("occupied root evidence"));
                }
            } else if let Some(successor) = noncommit.proposed_successor() {
                if reader
                    .point::<DraftPieceRootsFamily>(successor.key())?
                    .is_some()
                {
                    return Err(invariant("noncommit published successor"));
                }
            }
            Ok(())
        }
    }
}

fn retained_floor(
    reader: &mut OutcomeReader<'_>,
    source: &DraftEditHistoryFrontierV1,
    transition: &DraftEditHistoryTransitionV1,
    target: &DraftEditHistoryFrontierV1,
) -> Result<(), StagedDraftPieceOutcomeErrorV1> {
    let source_charge =
        stored_frontier_charge(source).map_err(|_| invariant("source history charge"))?;
    let target_charge =
        stored_frontier_charge(target).map_err(|_| invariant("target history charge"))?;
    let transition_charge =
        stored_transition_charge(transition).map_err(|_| invariant("history transition charge"))?;
    let cumulative = source
        .cumulative_encoded_bytes()
        .checked_add(transition_charge)
        .ok_or(invariant("history cumulative overflow"))?;
    let retain_all = source
        .retained_encoded_bytes()
        .checked_sub(source_charge)
        .and_then(|bytes| bytes.checked_add(target_charge))
        .and_then(|bytes| bytes.checked_add(transition_charge))
        .ok_or(invariant("history retained charge overflow"))?;
    if cumulative != transition.cumulative_encoded_bytes()
        || cumulative != target.cumulative_encoded_bytes()
        || source.byte_budget() != target.byte_budget()
        || source.retention_policy_revision() != target.retention_policy_revision()
    {
        return Err(invariant("history accounting source"));
    }
    let floor = if retain_all <= source.byte_budget() {
        match source.oldest_eligible() {
            Some(reference) => exact_transition(reader, reference)?,
            None => transition.clone(),
        }
    } else {
        let allowance = source
            .byte_budget()
            .checked_sub(target_charge)
            .ok_or(invariant("history capacity"))?;
        let threshold = cumulative
            .checked_sub(allowance)
            .ok_or(invariant("history threshold"))?;
        let floor_depth = source
            .oldest_eligible()
            .map_or(1, |reference| reference.journal_depth());
        let mut selected = transition.clone();
        for level in (0..64).rev() {
            let Some(reference) = selected.ancestor_witness().ancestor(level) else {
                continue;
            };
            if reference.journal_depth() < floor_depth {
                continue;
            }
            let candidate = exact_transition(reader, reference)?;
            let charge = stored_transition_charge(&candidate)
                .map_err(|_| invariant("history candidate charge"))?;
            let start = candidate
                .cumulative_encoded_bytes()
                .checked_sub(charge)
                .ok_or(invariant("history candidate boundary"))?;
            if start >= threshold {
                selected = candidate;
            }
        }
        let charge =
            stored_transition_charge(&selected).map_err(|_| invariant("selected floor charge"))?;
        if selected
            .cumulative_encoded_bytes()
            .checked_sub(charge)
            .is_none_or(|start| start < threshold)
        {
            return Err(invariant("selected floor threshold"));
        }
        selected
    };
    if target.oldest_eligible() != Some(floor.reference()) {
        return Err(invariant("history floor selection"));
    }
    let floor_charge =
        stored_transition_charge(&floor).map_err(|_| invariant("history floor charge"))?;
    let retained = floor
        .cumulative_encoded_bytes()
        .checked_sub(floor_charge)
        .and_then(|before_floor| cumulative.checked_sub(before_floor))
        .and_then(|bytes| bytes.checked_add(target_charge))
        .ok_or(invariant("retained history accounting"))?;
    if retained != target.retained_encoded_bytes() || retained > target.byte_budget() {
        return Err(invariant("retained history limit"));
    }
    Ok(())
}

fn exact_transition(
    reader: &mut OutcomeReader<'_>,
    reference: DraftEditHistoryTransitionReferenceV1,
) -> Result<DraftEditHistoryTransitionV1, StagedDraftPieceOutcomeErrorV1> {
    let value = reader
        .required::<DraftEditHistoryTransitionsFamily>(reference.key(), "history lineage target")?;
    if value.reference() != reference || !value.is_locally_valid() {
        return Err(invariant("history lineage target reference"));
    }
    Ok(value)
}

fn invariant(label: &'static str) -> StagedDraftPieceOutcomeErrorV1 {
    StagedDraftPieceOutcomeErrorV1::Invariant(label)
}

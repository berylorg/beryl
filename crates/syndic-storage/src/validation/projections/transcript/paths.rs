use beryl_home_store::DomainReader;

use crate::{TranscriptBuildPhase, codec::*, domain::SyndicDomain, error::SyndicValidationError};

use crate::validation::scan::{require, scan};

use super::{
    invariant,
    snapshot::{path_snapshot_must_match_current, validate_path_state_snapshot},
};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    let mut group = None;
    let mut first_depth = 0_u64;
    let mut first_turn = None;
    let mut previous_turn = None;
    let mut previous_depth = 0_u64;
    let mut observed = 0_u64;
    let mut history_complete = true;
    let mut require_current_snapshot = false;
    scan::<TranscriptPathTurnsFamily>(reader, |key, path| {
        let next_group = (key.thread, key.generation);
        if group != Some(next_group) {
            finish_path_group(
                reader,
                group,
                first_depth,
                first_turn,
                previous_turn,
                observed,
                history_complete,
            )?;
            group = Some(next_group);
            first_depth = key.depth.get();
            first_turn = Some(path.turn_id());
            previous_turn = None;
            previous_depth = 0;
            observed = 0;
            history_complete = true;
            let build = require::<TranscriptBuildsFamily>(
                reader,
                &ThreadTranscriptBuildKey {
                    thread: key.thread,
                    generation: key.generation,
                },
                "transcript path build owner is missing",
            )?;
            require_current_snapshot = path_snapshot_must_match_current(reader, &build)?;
        }
        if key.thread != path.thread_id()
            || key.generation != path.generation()
            || key.depth != path.depth()
            || (previous_depth != 0 && key.depth.get() != previous_depth + 1)
        {
            return invariant("transcript path key or contiguous depth disagrees");
        }
        let turn =
            require::<TurnsFamily>(reader, &path.turn_id(), "transcript path turn is missing")?;
        if turn.depth() != path.depth()
            || turn.chain_digest() != path.turn_path_digest()
            || previous_turn.is_some() && turn.parent().turn() != previous_turn
        {
            return invariant("transcript path turn provenance disagrees");
        }
        let state = require::<TurnStatesFamily>(
            reader,
            &path.turn_id(),
            "transcript path turn state is missing",
        )?;
        validate_path_state_snapshot(reader, path, &turn, &state, require_current_snapshot)?;
        history_complete &= crate::selected_path::turn_history_is_complete(
            path.lifecycle(),
            path.item_count(),
            path.finalized_item_count(),
            state.open_item_count(),
            state.history_blocking_item_count(),
            state.incomplete_reason(),
        );
        previous_depth = key.depth.get();
        previous_turn = Some(path.turn_id());
        observed = observed
            .checked_add(1)
            .ok_or(SyndicValidationError::Invariant(
                "transcript path count exhausted",
            ))?;
        Ok(())
    })?;
    finish_path_group(
        reader,
        group,
        first_depth,
        first_turn,
        previous_turn,
        observed,
        history_complete,
    )
}

fn finish_path_group(
    reader: &DomainReader<'_, SyndicDomain>,
    group: Option<(beryl_model::SyndicThreadId, crate::TranscriptGeneration)>,
    first_depth: u64,
    first_turn: Option<beryl_model::SyndicTurnId>,
    last_turn: Option<beryl_model::SyndicTurnId>,
    observed: u64,
    history_complete: bool,
) -> Result<(), SyndicValidationError> {
    let Some((thread, generation)) = group else {
        return Ok(());
    };
    let build = require::<TranscriptBuildsFamily>(
        reader,
        &ThreadTranscriptBuildKey { thread, generation },
        "transcript path build owner is missing",
    )?;
    let tail = build
        .committed_tail()
        .ok_or(SyndicValidationError::Invariant(
            "nonempty transcript path has no committed tail",
        ))?;
    let tail_depth =
        require::<TurnsFamily>(reader, &tail, "transcript path committed tail is missing")?
            .depth()
            .get();
    let expected_first_depth = tail_depth.checked_sub(observed.saturating_sub(1)).ok_or(
        SyndicValidationError::Invariant("transcript path depth frontier underflowed"),
    )?;
    if observed != build.path_turn_count()
        || last_turn != Some(tail)
        || first_depth != expected_first_depth
        || build.history_complete() != history_complete
    {
        return invariant("transcript path build frontier disagrees");
    }
    match build.phase() {
        TranscriptBuildPhase::Collecting { next_turn } => {
            let first = require::<TurnsFamily>(
                reader,
                &first_turn.ok_or(SyndicValidationError::Invariant(
                    "collecting transcript path has no first turn",
                ))?,
                "collecting transcript path first turn is missing",
            )?;
            let next_turn = next_turn.ok_or(SyndicValidationError::Invariant(
                "collecting transcript path has no next turn",
            ))?;
            let next = require::<TurnsFamily>(
                reader,
                &next_turn,
                "collecting transcript path cursor turn is missing",
            )?;
            if first.parent().turn() != Some(next_turn)
                || next.depth().get().checked_add(1) != Some(first.depth().get())
            {
                return invariant("collecting transcript path cursor disagrees");
            }
        }
        TranscriptBuildPhase::Publishing { .. } | TranscriptBuildPhase::Complete
            if first_depth != 1 =>
        {
            return invariant("complete transcript path does not begin at the root");
        }
        TranscriptBuildPhase::Publishing { .. }
        | TranscriptBuildPhase::Complete
        | TranscriptBuildPhase::Superseded => {}
    }
    Ok(())
}

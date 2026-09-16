use beryl_home_store::{DomainReader, ReadError};
use beryl_model::SyndicThreadId;

use crate::{
    ProviderObservationBuildLifecycle, RepairRequiredTarget, SourceEventPayload, TurnKind,
    codec::*, domain::SyndicDomain,
};

pub(crate) fn retained_repair_target_matches(
    reader: &DomainReader<'_, SyndicDomain>,
    thread_id: SyndicThreadId,
    target: &RepairRequiredTarget,
) -> Result<bool, ReadError> {
    let Some(thread) = point::<ThreadsFamily>(reader, &thread_id)? else {
        return Ok(false);
    };
    if thread.id() != thread_id || thread.committed_tail() != Some(target.turn_id()) {
        return Ok(false);
    }
    let Some(turn) = point::<TurnsFamily>(reader, &target.turn_id())? else {
        return Ok(false);
    };
    if turn.id() != target.turn_id()
        || turn.origin_thread_id() != thread_id
        || matches!(turn.kind(), TurnKind::ProviderOperation(_))
    {
        return Ok(false);
    }
    let Some(state) = point::<TurnStatesFamily>(reader, &target.turn_id())? else {
        return Ok(false);
    };
    if state.turn_id() != target.turn_id()
        || state.resolved_repair().is_some()
        || !state.lifecycle().is_proven_terminal()
        || state.end_status() != Some(target.gap().status())
        || state.source_event_count() != target.gap().terminal().sequence().get()
    {
        return Ok(false);
    }
    let source = target.source();
    let Some(thread_index) =
        point::<CasThreadIndexFamily>(reader, &CasThreadKey::Record(source.thread_id().clone()))?
    else {
        return Ok(false);
    };
    if thread_index.cas_thread_id() != source.thread_id() || thread_index.thread_id() != thread_id {
        return Ok(false);
    }
    let Some(turn_index) = point::<CasTurnIndexFamily>(
        reader,
        &CasTurnKey::Record(source.thread_id().clone(), source.turn_id().clone()),
    )?
    else {
        return Ok(false);
    };
    if turn_index.cas_thread_id() != source.thread_id()
        || turn_index.cas_turn_id() != source.turn_id()
        || turn_index.thread_id() != thread_id
        || turn_index.turn_id() != target.turn_id()
    {
        return Ok(false);
    }
    let mut issue = None;
    if !super::repair_source_events_match(target, |owner, ordinal| {
        let event = point::<SourceEventsFamily>(reader, &TurnEventKey { owner, ordinal })?;
        if let Some(event) = &event
            && let SourceEventPayload::ProviderObservationIssue(value) = event.payload()
        {
            issue = Some(value.clone());
        }
        Ok::<_, ReadError>(event)
    })? {
        return Ok(false);
    }
    if let Some(issue) = issue {
        if state.provider_observation_issue().is_none()
            || target.gap().status().incomplete_reason()
                != Some(crate::TurnIncompleteReason::CompletionMismatch)
        {
            return Ok(false);
        }
        let reference = issue.observation();
        let Some(build) = point::<ProviderObservationBuildsFamily>(reader, &reference.identity())?
        else {
            return Ok(false);
        };
        if build.lifecycle() != ProviderObservationBuildLifecycle::Sealed
            || !reference.matches_build(&build)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn point<F: Family>(
    reader: &DomainReader<'_, SyndicDomain>,
    key: &F::Key,
) -> Result<Option<F::Value>, ReadError> {
    reader.point::<ExactCodec<F>>(key, family_point_limit::<F>())
}

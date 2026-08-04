use beryl_home_store::DomainReader;

use crate::{ProjectionLifecycle, codec::*, domain::SyndicDomain, error::SyndicValidationError};

use crate::validation::scan::{point, require, scan};

use super::invariant;

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ThreadsFamily>(reader, |_, thread| {
        let summary = require::<HistorySummariesFamily>(
            reader,
            &thread.id(),
            "thread history summary is missing",
        )?;
        if summary.thread_id() != thread.id()
            || summary.thread_revision() != thread.revision()
            || summary.committed_tail() != thread.committed_tail()
            || summary.selected_path_digest() != thread.selected_path_digest()
        {
            return invariant("thread history summary disagrees");
        }
        let head = require::<TranscriptHeadsFamily>(
            reader,
            &thread.id(),
            "history summary transcript head is missing",
        )?;
        let draft = require::<DraftsFamily>(
            reader,
            &thread.current_draft_id(),
            "history summary current draft is missing",
        )?;
        let mut fold = crate::selected_path::SelectedPathFold::empty();
        let mut cursor = thread.committed_tail();
        while let Some(turn_id) = cursor {
            let turn = require::<TurnsFamily>(
                reader,
                &turn_id,
                "history summary selected turn is missing",
            )?;
            let state = require::<TurnStatesFamily>(
                reader,
                &turn_id,
                "history summary selected turn state is missing",
            )?;
            fold = fold.include(
                turn.submitted_at(),
                state.lifecycle(),
                state.item_count(),
                state.finalized_item_count(),
                state.open_item_count(),
                state.history_blocking_item_count(),
                state.provider_observation_issue(),
                state.incomplete_reason(),
                state.updated_at(),
            );
            cursor = turn.parent().turn();
        }
        let expected_complete = fold.summary_complete(head.lifecycle());
        if head.lifecycle() == ProjectionLifecycle::Current {
            let build = require::<TranscriptBuildsFamily>(
                reader,
                &ThreadTranscriptBuildKey {
                    thread: head.thread_id(),
                    generation: head.generation(),
                },
                "current history summary transcript build is missing",
            )?;
            if build.history_complete() != fold.all_finalized() {
                return invariant("current transcript history fold disagrees");
            }
        }
        if summary.complete() != expected_complete {
            return invariant("history summary completeness derivation disagrees");
        }
        let last_activity = fold
            .last_activity_at()
            .map_or(draft.updated_at(), |activity| {
                activity.max(draft.updated_at())
            });
        if summary.last_activity_at() != last_activity {
            return invariant("history summary last-activity derivation disagrees");
        }
        Ok(())
    })?;
    scan::<HistorySummariesFamily>(reader, |key, summary| {
        if *key != summary.thread_id() || point::<ThreadsFamily>(reader, key)?.is_none() {
            return invariant("history summary has no matching thread");
        }
        Ok(())
    })
}

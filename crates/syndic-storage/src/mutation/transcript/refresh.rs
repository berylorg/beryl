use beryl_home_store::{DomainReader, MutationBuilder};

use crate::mutation::{point, required};
use crate::{
    ProjectionLifecycle, SyndicMutationError, TranscriptPathTurnRecord, TurnRecord,
    TurnStateRecord, codec::*, domain::SyndicDomain,
};

#[derive(Default)]
pub(in crate::mutation) struct SelectedTranscriptStateEffect {
    head: Option<crate::TranscriptViewHeadRecord>,
    build: Option<crate::TranscriptBuildRecord>,
    path: Option<TranscriptPathTurnRecord>,
}

impl SelectedTranscriptStateEffect {
    pub(in crate::mutation) fn invalidate(
        reader: &DomainReader<'_, SyndicDomain>,
        thread: &crate::ThreadRecord,
    ) -> Result<Self, SyndicMutationError> {
        let (head, build) = super::invalidate::invalidate_transcript_projection(reader, thread)?;
        Ok(Self {
            head,
            build,
            path: None,
        })
    }

    pub(in crate::mutation) fn contribute(
        &self,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        if let Some(head) = &self.head {
            mutations.put::<TranscriptHeadsCodec>(&head.thread_id(), head)?;
        }
        if let Some(build) = &self.build {
            mutations.put::<TranscriptBuildsCodec>(
                &ThreadTranscriptBuildKey {
                    thread: build.thread_id(),
                    generation: build.generation(),
                },
                build,
            )?;
        }
        if let Some(path) = &self.path {
            mutations.put::<TranscriptPathTurnsCodec>(
                &ThreadTranscriptPathKey {
                    thread: path.thread_id(),
                    generation: path.generation(),
                    depth: path.depth(),
                },
                path,
            )?;
        }
        Ok(())
    }
}

pub(in crate::mutation) fn refresh_selected_path_state(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    turn: &TurnRecord,
    state: &TurnStateRecord,
) -> Result<SelectedTranscriptStateEffect, SyndicMutationError> {
    if !crate::mutation::turn_is_on_selected_path(reader, thread, turn)? {
        return Ok(SelectedTranscriptStateEffect::default());
    }
    let head = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
    if head.lifecycle() == ProjectionLifecycle::Current {
        return Ok(SelectedTranscriptStateEffect {
            path: refresh_current_path_state(reader, thread, turn, state)?,
            ..SelectedTranscriptStateEffect::default()
        });
    }
    let key = ThreadTranscriptPathKey {
        thread: thread.id(),
        generation: head.generation(),
        depth: turn.depth(),
    };
    let Some(path) = point::<TranscriptPathTurnsFamily>(reader, &key)? else {
        return Ok(SelectedTranscriptStateEffect::default());
    };
    if path.turn_id() != turn.id() || path.turn_path_digest() != turn.chain_digest() {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let Some(build) = super::invalidate::latest_active_build(reader, thread.id())? else {
        return Ok(SelectedTranscriptStateEffect::default());
    };
    if build.generation() != head.generation()
        || build.revision() != head.revision()
        || head.entry_count() != 0
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    if path.state_revision() >= state.revision() {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    SelectedTranscriptStateEffect::invalidate(reader, thread)
}

/// Refreshes one selected current transcript path row after a presentation-neutral state advance.
pub(in crate::mutation) fn refresh_current_path_state(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
    turn: &TurnRecord,
    state: &TurnStateRecord,
) -> Result<Option<TranscriptPathTurnRecord>, SyndicMutationError> {
    if !crate::mutation::turn_is_on_selected_path(reader, thread, turn)? {
        return Ok(None);
    }
    let head = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
    if head.lifecycle() != ProjectionLifecycle::Current {
        return Ok(None);
    }
    let key = ThreadTranscriptPathKey {
        thread: thread.id(),
        generation: head.generation(),
        depth: turn.depth(),
    };
    let Some(path) = point::<TranscriptPathTurnsFamily>(reader, &key)? else {
        return Ok(None);
    };
    if path.turn_id() != turn.id() || path.turn_path_digest() != turn.chain_digest() {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    Ok(Some(TranscriptPathTurnRecord::new(
        path.thread_id(),
        path.generation(),
        path.depth(),
        path.turn_id(),
        path.turn_path_digest(),
        state.revision(),
        state.lifecycle(),
        state.source_event_count(),
        state.item_count(),
        state.finalized_item_count(),
        state.updated_at(),
    )))
}

use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, DomainReader};
use beryl_model::SyndicThreadId;

use crate::{
    ProjectionLifecycle, SyndicMutationError, TranscriptBuildPhase, TranscriptBuildRecord,
    codec::*, domain::SyndicDomain,
};

use super::super::required;

pub(super) fn latest_active_build(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: SyndicThreadId,
) -> Result<Option<TranscriptBuildRecord>, SyndicMutationError> {
    let page = reader.cursor::<TranscriptBuildsCodec>(
        &CursorRange::closed(
            ThreadTranscriptBuildKey::first_for_thread(thread),
            ThreadTranscriptBuildKey::last_for_thread(thread),
        ),
        CursorDirection::Reverse,
        CursorReadLimits::new(1, 1024 * 1024).expect("latest transcript-build bounds are nonzero"),
    )?;
    Ok(page
        .records()
        .first()
        .map(|record| *record.value())
        .filter(|build| {
            matches!(
                build.phase(),
                TranscriptBuildPhase::Collecting { .. } | TranscriptBuildPhase::Publishing { .. }
            )
        }))
}

fn superseded_build(
    build: TranscriptBuildRecord,
) -> Result<TranscriptBuildRecord, SyndicMutationError> {
    Ok(TranscriptBuildRecord::new(
        build.thread_id(),
        build.generation(),
        build.revision().checked_next()?,
        build.source_thread_revision(),
        build.committed_tail(),
        build.selected_path_digest(),
        build.path_turn_count(),
        build.entry_count(),
        build.entry_digest(),
        build.history_complete(),
        TranscriptBuildPhase::Superseded,
    ))
}

pub(in crate::mutation) fn supersede_active_transcript_build(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
) -> Result<Option<TranscriptBuildRecord>, SyndicMutationError> {
    let Some(build) = latest_active_build(reader, thread.id())? else {
        return Ok(None);
    };
    if build.source_thread_revision() > thread.revision()
        || build.committed_tail() != thread.committed_tail()
        || build.selected_path_digest() != thread.selected_path_digest()
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    superseded_build(build).map(Some)
}

pub(in crate::mutation) fn invalidate_transcript_projection(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: &crate::ThreadRecord,
) -> Result<
    (
        Option<crate::TranscriptViewHeadRecord>,
        Option<TranscriptBuildRecord>,
    ),
    SyndicMutationError,
> {
    let head = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
    if head.committed_tail() != thread.committed_tail()
        || head.selected_path_digest() != thread.selected_path_digest()
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let build = supersede_active_transcript_build(reader, thread)?;
    if let Some(active) = &build
        && (active.generation() != head.generation() || active.revision() != head.revision())
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    let next_head = if head.lifecycle() == ProjectionLifecycle::Current || build.is_some() {
        Some(crate::TranscriptViewHeadRecord::new(
            head.thread_id(),
            head.generation().checked_next()?,
            head.revision().checked_next()?,
            0,
            head.committed_tail(),
            head.selected_path_digest(),
            ProjectionLifecycle::Stale,
        ))
    } else {
        None
    };
    Ok((next_head, build))
}

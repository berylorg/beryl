use beryl_home_store::HomeStore;
use beryl_model::SyndicThreadId;
use syndic_storage::{
    AdvanceTranscriptBuild, ProjectionLifecycle, StartTranscriptBuild, SyndicPointReadLimit,
    SyndicStorage, TranscriptBuildPhase, TranscriptBuildRecord,
};

use super::super::OrdinaryTurnExecutionError;
use super::{command, snapshot};

pub(super) fn converge_selected_transcript(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    loop {
        let current = snapshot::transcript(store, storage, thread_id, limit)?;
        if validate_snapshot(&current)? {
            return Ok(());
        }
        match current.build.as_ref() {
            Some(build) => {
                if !active_build(&current, build) {
                    return Err(OrdinaryTurnExecutionError::Invariant(
                        "selected transcript build is not resumable",
                    ));
                }
                advance_build(store, storage, thread_id, limit, &current, build)?;
            }
            None => start_build(store, storage, thread_id, limit, &current)?,
        }
    }
}

/// Returns true exactly when this stable snapshot is already current.
fn validate_snapshot(
    current: &snapshot::TranscriptSnapshot,
) -> Result<bool, OrdinaryTurnExecutionError> {
    if current.head.committed_tail() != current.thread.committed_tail()
        || current.head.selected_path_digest() != current.thread.selected_path_digest()
    {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "selected transcript head does not describe the selected thread path",
        ));
    }
    if current.head.lifecycle() == ProjectionLifecycle::Current {
        let exact = current.build.as_ref().is_some_and(|build| {
            build.thread_id() == current.thread.id()
                && build.generation() == current.head.generation()
                && build.revision() == current.head.revision()
                && build.source_thread_revision() <= current.thread.revision()
                && build.committed_tail() == current.thread.committed_tail()
                && build.selected_path_digest() == current.thread.selected_path_digest()
                && build.entry_count() == current.head.entry_count()
                && matches!(build.phase(), TranscriptBuildPhase::Complete)
        });
        return if exact {
            Ok(true)
        } else {
            Err(OrdinaryTurnExecutionError::Invariant(
                "current selected transcript build is incoherent",
            ))
        };
    }
    if current.head.entry_count() != 0 {
        return Err(OrdinaryTurnExecutionError::Invariant(
            "stale selected transcript has a published entry frontier",
        ));
    }
    Ok(false)
}

fn active_build(current: &snapshot::TranscriptSnapshot, build: &TranscriptBuildRecord) -> bool {
    build.thread_id() == current.thread.id()
        && build.generation() == current.head.generation()
        && build.revision() == current.head.revision()
        && build.source_thread_revision() <= current.thread.revision()
        && build.committed_tail() == current.thread.committed_tail()
        && build.selected_path_digest() == current.thread.selected_path_digest()
        && matches!(
            build.phase(),
            TranscriptBuildPhase::Collecting { .. } | TranscriptBuildPhase::Publishing { .. }
        )
}

fn start_build(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    _limit: SyndicPointReadLimit,
    before: &snapshot::TranscriptSnapshot,
) -> Result<(), OrdinaryTurnExecutionError> {
    let request =
        StartTranscriptBuild::new(thread_id, before.thread.revision(), before.head.revision());
    command::dispatch(store, storage.current_start_transcript_build(request))
}

fn advance_build(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    _limit: SyndicPointReadLimit,
    before: &snapshot::TranscriptSnapshot,
    build: &TranscriptBuildRecord,
) -> Result<(), OrdinaryTurnExecutionError> {
    let request =
        AdvanceTranscriptBuild::new(thread_id, before.head.generation(), build.revision());
    command::dispatch(store, storage.current_advance_transcript_build(request))
}

use super::access::HistoryAccess;

use beryl_model::SyndicThreadId;
use syndic_storage::{
    AdvanceTranscriptBuild, ProjectionLifecycle, StartTranscriptBuild, SyndicPointReadLimit,
    SyndicStorage, TranscriptBuildPhase, TranscriptBuildRecord,
};

use super::super::OrdinaryTurnExecutionError;
use super::{command, snapshot};

pub(super) fn converge_selected_transcript(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
) -> Result<(), OrdinaryTurnExecutionError> {
    loop {
        match step(store, storage, thread_id, limit) {
            Ok(true) => return Ok(()),
            Ok(false) => (),
            Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id: changed })
                if changed == thread_id =>
            {
                ()
            }
            Err(error) => return Err(error),
        }
    }
}

pub(super) fn step(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
) -> Result<bool, OrdinaryTurnExecutionError> {
    step_with_observation(store, storage, thread_id, limit, || ())
}

fn step_with_observation(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    limit: SyndicPointReadLimit,
    observed: impl FnOnce(),
) -> Result<bool, OrdinaryTurnExecutionError> {
    let current = snapshot::transcript(store, storage, thread_id, limit)?;
    if validate_snapshot(&current)? {
        return Ok(true);
    }
    observed();
    let action = match current.build.as_ref() {
        Some(build) => {
            if !active_build(&current, build) {
                return Err(OrdinaryTurnExecutionError::Invariant(
                    "selected transcript build is not resumable",
                ));
            }
            advance_build(store, storage, thread_id, limit, &current, build)
        }
        None => start_build(store, storage, thread_id, limit, &current),
    };
    if let Err(error) = action {
        if matches!(&error,
            OrdinaryTurnExecutionError::HomeCommandNotCommitted(evidence)
            if crate::cas_projection::service::selected_transcript_refusal_is_conflict(evidence))
        {
            let after = snapshot::transcript(store, storage, thread_id, limit)?;
            validate_snapshot(&after)?;
            if after.head != current.head
                || after.build != current.build
                || after.thread.committed_tail() != current.thread.committed_tail()
                || after.thread.selected_path_digest() != current.thread.selected_path_digest()
            {
                return Ok(false);
            }
        }
        return Err(error);
    }
    Ok(false)
}

#[cfg(test)]
pub(crate) fn step_selected_transcript_after_observation_for_test(
    home: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    observed: impl FnOnce(),
) -> Result<bool, OrdinaryTurnExecutionError> {
    step_with_observation(
        HistoryAccess::Ordinary(home),
        storage,
        thread_id,
        SyndicPointReadLimit::new(65_536).expect("fixed nonzero point-read limit"),
        observed,
    )
}

#[cfg(test)]
pub(crate) fn converge_selected_transcript_after_observation_for_test(
    home: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    observed: impl FnOnce(),
) -> Result<(), OrdinaryTurnExecutionError> {
    match step_selected_transcript_after_observation_for_test(home, storage, thread_id, observed) {
        Ok(true) => return Ok(()),
        Ok(false) => (),
        Err(OrdinaryTurnExecutionError::ConcurrentChange { thread_id: changed })
            if changed == thread_id =>
        {
            ()
        }
        Err(error) => return Err(error),
    }
    converge_selected_transcript(
        HistoryAccess::Ordinary(home),
        storage,
        thread_id,
        SyndicPointReadLimit::new(65_536).expect("fixed nonzero point-read limit"),
    )
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
        let exact = current.build.as_ref().map_or_else(
            || current.thread.committed_tail().is_none() && current.head.entry_count() == 0,
            |build| {
                build.thread_id() == current.thread.id()
                    && build.generation() == current.head.generation()
                    && build.revision() == current.head.revision()
                    && build.source_thread_revision() <= current.thread.revision()
                    && build.committed_tail() == current.thread.committed_tail()
                    && build.selected_path_digest() == current.thread.selected_path_digest()
                    && build.entry_count() == current.head.entry_count()
                    && matches!(build.phase(), TranscriptBuildPhase::Complete)
            },
        );
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
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    _limit: SyndicPointReadLimit,
    before: &snapshot::TranscriptSnapshot,
) -> Result<(), OrdinaryTurnExecutionError> {
    let request =
        StartTranscriptBuild::new(thread_id, before.thread.revision(), before.head.revision());
    command::dispatch(store, storage.current_start_transcript_build(request))
}

fn advance_build(
    store: HistoryAccess<'_>,
    storage: &SyndicStorage,
    thread_id: SyndicThreadId,
    _limit: SyndicPointReadLimit,
    before: &snapshot::TranscriptSnapshot,
    build: &TranscriptBuildRecord,
) -> Result<(), OrdinaryTurnExecutionError> {
    let request =
        AdvanceTranscriptBuild::new(thread_id, before.head.generation(), build.revision());
    command::dispatch(store, storage.current_advance_transcript_build(request))
}

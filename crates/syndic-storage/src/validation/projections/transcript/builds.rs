use beryl_home_store::DomainReader;

use crate::{
    ProjectionLifecycle, TranscriptBuildPhase, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use crate::validation::scan::{point, require, scan};

use super::{cursor, invariant};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<TranscriptBuildsFamily>(reader, |key, build| {
        if key.thread != build.thread_id() || key.generation != build.generation() {
            return invariant("transcript build key disagrees");
        }
        let thread = require::<ThreadsFamily>(
            reader,
            &build.thread_id(),
            "transcript build source thread is missing",
        )?;
        let tail_depth = validate_build_path_proof(reader, build)?;
        let first_path = ThreadTranscriptPathKey {
            thread: build.thread_id(),
            generation: build.generation(),
            depth: crate::TurnDepth::FIRST,
        };
        let first_entry = ThreadTranscriptKey {
            thread: build.thread_id(),
            generation: build.generation(),
            position: crate::TranscriptPosition::FIRST,
        };
        if (build.entry_count() == 0)
            == point::<TranscriptEntriesFamily>(reader, &first_entry)?.is_some()
        {
            return invariant("transcript build entry zero frontier disagrees");
        }
        if build.entry_count() == 0
            && build.entry_digest() != crate::projection::transcript_entry_digest_seed()
        {
            return invariant("empty transcript build digest disagrees");
        }
        if build.path_turn_count() != 0 {
            let tail_depth = tail_depth.ok_or(SyndicValidationError::Invariant(
                "nonempty transcript path has no committed tail",
            ))?;
            let first_collected_depth = tail_depth
                .get()
                .checked_sub(build.path_turn_count() - 1)
                .ok_or(SyndicValidationError::Invariant(
                "transcript build path frontier exceeds its committed tail",
            ))?;
            let first_collected_depth =
                crate::TurnDepth::new(first_collected_depth).map_err(|_| {
                    SyndicValidationError::Invariant(
                        "transcript build path frontier exceeds its committed tail",
                    )
                })?;
            let first_collected = ThreadTranscriptPathKey {
                thread: build.thread_id(),
                generation: build.generation(),
                depth: first_collected_depth,
            };
            if point::<TranscriptPathTurnsFamily>(reader, &first_collected)?.is_none() {
                return invariant("transcript build first collected path record is missing");
            }
        }
        if build.path_turn_count() == 0 && !build.history_complete() {
            return invariant("empty transcript path history fold disagrees");
        }
        match build.phase() {
            TranscriptBuildPhase::Collecting { next_turn } => {
                let Some(tail_depth) = tail_depth else {
                    return invariant("collecting transcript build has no committed tail");
                };
                if build.entry_count() != 0
                    || next_turn.is_none()
                    || build.path_turn_count() >= tail_depth.get()
                    || build.path_turn_count() == 0 && next_turn != build.committed_tail()
                {
                    return invariant("collecting transcript build frontier is invalid");
                }
                validate_active_build(reader, build, &thread)?;
            }
            TranscriptBuildPhase::Publishing {
                next_depth,
                next_item,
                next_projection,
            } => {
                let Some(tail_depth) = tail_depth else {
                    return invariant("publishing transcript build has no committed tail");
                };
                if build.path_turn_count() == 0
                    || build.path_turn_count() != tail_depth.get()
                    || next_depth.get() > build.path_turn_count()
                    || point::<TranscriptPathTurnsFamily>(reader, &first_path)?.is_none()
                {
                    return invariant("publishing transcript build frontier is invalid");
                }
                validate_active_build(reader, build, &thread)?;
                cursor::validate_publishing(reader, build, next_depth, next_item, next_projection)?;
            }
            TranscriptBuildPhase::Complete => {
                if build.source_thread_revision() > thread.revision()
                    || build.path_turn_count() != tail_depth.map_or(0, crate::TurnDepth::get)
                    || (build.path_turn_count() == 0)
                        != point::<TranscriptPathTurnsFamily>(reader, &first_path)?.is_none()
                {
                    return invariant("completed transcript build manifest is invalid");
                }
            }
            TranscriptBuildPhase::Superseded => {
                if build.path_turn_count() > tail_depth.map_or(0, crate::TurnDepth::get) {
                    return invariant("superseded transcript path frontier is invalid");
                }
            }
        }
        Ok(())
    })
}

fn validate_active_build(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &crate::TranscriptBuildRecord,
    thread: &crate::ThreadRecord,
) -> Result<(), SyndicValidationError> {
    let head = require::<TranscriptHeadsFamily>(
        reader,
        &build.thread_id(),
        "active transcript build head is missing",
    )?;
    if build.source_thread_revision() != thread.revision()
        || build.committed_tail() != thread.committed_tail()
        || build.selected_path_digest() != thread.selected_path_digest()
        || head.generation() != build.generation()
        || head.revision() != build.revision()
        || head.lifecycle() != ProjectionLifecycle::Stale
    {
        return invariant("active transcript build authority disagrees");
    }
    Ok(())
}

fn validate_build_path_proof(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &crate::TranscriptBuildRecord,
) -> Result<Option<crate::TurnDepth>, SyndicValidationError> {
    match build.committed_tail() {
        Some(tail) => {
            let turn = require::<TurnsFamily>(reader, &tail, "transcript build tail is missing")?;
            if turn.chain_digest() != build.selected_path_digest() {
                return invariant("transcript build path digest disagrees");
            }
            Ok(Some(turn.depth()))
        }
        None if build.selected_path_digest() == crate::empty_selected_path_digest() => Ok(None),
        None => invariant("empty transcript build path digest is invalid"),
    }
}

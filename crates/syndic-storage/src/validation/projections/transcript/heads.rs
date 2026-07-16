use beryl_home_store::DomainReader;

use crate::{
    ProjectionLifecycle, TranscriptBuildPhase, codec::*, domain::SyndicDomain,
    error::SyndicValidationError,
};

use crate::validation::scan::{point, require, scan, scan_range};

use super::{invariant, snapshot::path_state_snapshot_equals, visibility::is_transcript_visible};

pub(super) fn validate(
    reader: &DomainReader<'_, SyndicDomain>,
) -> Result<(), SyndicValidationError> {
    scan::<ThreadsFamily>(reader, |_, thread| {
        require::<TranscriptHeadsFamily>(
            reader,
            &thread.id(),
            "thread transcript head is missing",
        )?;
        Ok(())
    })?;
    scan::<TranscriptHeadsFamily>(reader, |key, head| {
        let thread =
            require::<ThreadsFamily>(reader, key, "transcript head has no matching thread")?;
        if *key != head.thread_id() {
            return invariant("transcript head has no matching thread");
        }
        validate_head_path(reader, head)?;
        if head.committed_tail() != thread.committed_tail()
            || head.selected_path_digest() != thread.selected_path_digest()
        {
            return invariant("selected transcript generation disagrees with thread path");
        }
        if head.lifecycle() == ProjectionLifecycle::Current {
            let build = require::<TranscriptBuildsFamily>(
                reader,
                &ThreadTranscriptBuildKey {
                    thread: head.thread_id(),
                    generation: head.generation(),
                },
                "current transcript head build manifest is missing",
            )?;
            if build.phase() != TranscriptBuildPhase::Complete
                || build.revision() != head.revision()
                || build.source_thread_revision() > thread.revision()
                || build.entry_count() != head.entry_count()
                || build.committed_tail() != head.committed_tail()
                || build.selected_path_digest() != head.selected_path_digest()
            {
                return invariant("current transcript head build manifest disagrees");
            }
            let expected = count_visible_current_projections(reader, &build)?;
            if expected != head.entry_count() {
                return invariant("current transcript head is not a complete visible projection");
            }
        } else {
            if head.entry_count() != 0 {
                return invariant("stale transcript head exposes partial entries");
            }
            if let Some(build) = point::<TranscriptBuildsFamily>(
                reader,
                &ThreadTranscriptBuildKey {
                    thread: head.thread_id(),
                    generation: head.generation(),
                },
            )? && (build.revision() != head.revision()
                || !matches!(
                    build.phase(),
                    TranscriptBuildPhase::Collecting { .. }
                        | TranscriptBuildPhase::Publishing { .. }
                ))
            {
                return invariant("stale transcript head build frontier disagrees");
            }
        }
        let first = ThreadTranscriptKey {
            thread: head.thread_id(),
            generation: head.generation(),
            position: crate::TranscriptPosition::FIRST,
        };
        if head.lifecycle() == ProjectionLifecycle::Current
            && (head.entry_count() == 0)
                == point::<TranscriptEntriesFamily>(reader, &first)?.is_some()
        {
            return invariant("transcript head zero frontier disagrees");
        }
        Ok(())
    })
}

fn validate_head_path(
    reader: &DomainReader<'_, SyndicDomain>,
    head: &crate::TranscriptViewHeadRecord,
) -> Result<(), SyndicValidationError> {
    match head.committed_tail() {
        Some(tail) => {
            let turn = require::<TurnsFamily>(
                reader,
                &tail,
                "transcript head selected-path tail is missing",
            )?;
            if turn.chain_digest() != head.selected_path_digest() {
                return invariant("transcript head selected-path digest disagrees");
            }
        }
        None if head.selected_path_digest() == crate::empty_selected_path_digest() => {}
        None => return invariant("empty transcript head has a noncanonical path digest"),
    }
    Ok(())
}

fn count_visible_current_projections(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &crate::TranscriptBuildRecord,
) -> Result<u64, SyndicValidationError> {
    let mut total = 0_u64;
    if build.path_turn_count() == 0 {
        return Ok(0);
    }
    scan_range::<TranscriptPathTurnsFamily>(
        reader,
        ThreadTranscriptPathKey {
            thread: build.thread_id(),
            generation: build.generation(),
            depth: crate::TurnDepth::FIRST,
        },
        ThreadTranscriptPathKey {
            thread: build.thread_id(),
            generation: build.generation(),
            depth: crate::TurnDepth::new(u64::MAX).expect("maximum is nonzero"),
        },
        |_, path| {
            let id = path.turn_id();
            let state = require::<TurnStatesFamily>(
                reader,
                &id,
                "selected transcript turn state is missing",
            )?;
            if !path_state_snapshot_equals(path, &state) {
                return invariant("current transcript path state snapshot is stale");
            }
            scan_range::<TurnItemsFamily>(
                reader,
                TurnItemKey {
                    owner: id,
                    ordinal: crate::TurnItemOrdinal::FIRST,
                },
                TurnItemKey {
                    owner: id,
                    ordinal: crate::TurnItemOrdinal::new(u64::MAX).expect("maximum is nonzero"),
                },
                |_, item_index| {
                    if item_index.ordinal().get() > path.finalized_item_count() {
                        return Ok(());
                    }
                    let item = require::<CanonicalItemsFamily>(
                        reader,
                        &item_index.item_id(),
                        "selected transcript item is missing",
                    )?;
                    if is_transcript_visible(item.kind()) {
                        let head = require::<ItemProjectionHeadsFamily>(
                            reader,
                            &item.id(),
                            "visible current transcript item head is missing",
                        )?;
                        if head.lifecycle() != ProjectionLifecycle::Current {
                            return invariant("visible current transcript item head is stale");
                        }
                        let set = require::<ItemProjectionSetsFamily>(
                            reader,
                            &ItemProjectionSetKey {
                                item: item.id(),
                                generation: head.generation(),
                            },
                            "visible current transcript item set is missing",
                        )?;
                        if set.source_item_revision() != item.revision()
                            || item.payload().content() != Some(set.source_content())
                        {
                            return invariant("visible current transcript item set is not current");
                        }
                        let current = set.projection_count();
                        total =
                            total
                                .checked_add(current)
                                .ok_or(SyndicValidationError::Invariant(
                                    "current transcript projection count exhausted",
                                ))?;
                    }
                    Ok(())
                },
            )?;
            Ok(())
        },
    )?;
    Ok(total)
}

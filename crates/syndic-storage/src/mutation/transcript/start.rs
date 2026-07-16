use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainMutation, DomainReader, MutationBuilder,
};
use beryl_model::SyndicThreadId;

use crate::{
    ProjectionLifecycle, SyndicMutationError, TranscriptBuildPhase, TranscriptBuildRecord,
    TranscriptGeneration, TranscriptPosition, TurnDepth, codec::*, domain::SyndicDomain,
};

use super::{StartTranscriptBuild, invalidate};
use crate::mutation::{point, required};

pub(super) struct StartMutation {
    request: StartTranscriptBuild,
}

impl StartMutation {
    pub(super) const fn new(request: StartTranscriptBuild) -> Self {
        Self { request }
    }
}

impl DomainMutation<SyndicDomain> for StartMutation {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.records(reader).map(|_| ())
    }

    fn contribute(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = self.records(reader)?;
        mutations.put::<TranscriptBuildsCodec>(
            &ThreadTranscriptBuildKey {
                thread: records.build.thread_id(),
                generation: records.build.generation(),
            },
            &records.build,
        )?;
        if let Some(head) = &records.head {
            mutations.put::<TranscriptHeadsCodec>(&head.thread_id(), head)?;
        }
        if let Some(summary) = &records.summary {
            mutations.put::<HistorySummariesCodec>(&summary.thread_id(), summary)?;
        }
        Ok(())
    }
}

fn generation_has_path(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
) -> Result<bool, SyndicMutationError> {
    Ok(!reader
        .cursor::<TranscriptPathTurnsCodec>(
            &CursorRange::closed(
                ThreadTranscriptPathKey {
                    thread,
                    generation,
                    depth: TurnDepth::FIRST,
                },
                ThreadTranscriptPathKey {
                    thread,
                    generation,
                    depth: TurnDepth::new(u64::MAX).expect("maximum is nonzero"),
                },
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(1, 1024 * 1024)
                .expect("transcript path collision bounds are nonzero"),
        )?
        .records()
        .is_empty())
}

fn generation_has_entries(
    reader: &DomainReader<'_, SyndicDomain>,
    thread: SyndicThreadId,
    generation: TranscriptGeneration,
) -> Result<bool, SyndicMutationError> {
    Ok(!reader
        .cursor::<TranscriptEntriesCodec>(
            &CursorRange::closed(
                ThreadTranscriptKey {
                    thread,
                    generation,
                    position: TranscriptPosition::FIRST,
                },
                ThreadTranscriptKey {
                    thread,
                    generation,
                    position: TranscriptPosition::new(u64::MAX).expect("maximum is nonzero"),
                },
            ),
            CursorDirection::Forward,
            CursorReadLimits::new(1, 1024 * 1024)
                .expect("transcript entry collision bounds are nonzero"),
        )?
        .records()
        .is_empty())
}

struct StartRecords {
    build: TranscriptBuildRecord,
    head: Option<crate::TranscriptViewHeadRecord>,
    summary: Option<crate::HistorySummaryRecord>,
}

impl StartMutation {
    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<StartRecords, SyndicMutationError> {
        let thread = required::<ThreadsFamily>(reader, &self.request.thread_id)?;
        let head = required::<TranscriptHeadsFamily>(reader, &thread.id())?;
        let summary = required::<HistorySummariesFamily>(reader, &thread.id())?;
        if thread.revision() != self.request.expected_thread_revision
            || head.revision() != self.request.expected_head_revision
            || head.committed_tail() != thread.committed_tail()
            || head.selected_path_digest() != thread.selected_path_digest()
            || summary.thread_revision() != thread.revision()
            || summary.committed_tail() != thread.committed_tail()
            || summary.selected_path_digest() != thread.selected_path_digest()
        {
            return Err(SyndicMutationError::TranscriptBuildConflict);
        }
        if head.lifecycle() == ProjectionLifecycle::Current {
            return Err(SyndicMutationError::TranscriptAlreadyCurrent);
        }
        if head.entry_count() != 0 {
            return Err(SyndicMutationError::TranscriptBuildConflict);
        }
        let key = ThreadTranscriptBuildKey {
            thread: thread.id(),
            generation: head.generation(),
        };
        if point::<TranscriptBuildsFamily>(reader, &key)?.is_some()
            || generation_has_path(reader, thread.id(), head.generation())?
            || generation_has_entries(reader, thread.id(), head.generation())?
            || invalidate::latest_active_build(reader, thread.id())?.is_some()
        {
            return Err(SyndicMutationError::TranscriptBuildConflict);
        }
        let complete = thread.committed_tail().is_none();
        let revision = head.revision().checked_next()?;
        let build = TranscriptBuildRecord::new(
            thread.id(),
            head.generation(),
            revision,
            thread.revision(),
            thread.committed_tail(),
            thread.selected_path_digest(),
            0,
            0,
            crate::projection::transcript_entry_digest_seed(),
            true,
            if complete {
                TranscriptBuildPhase::Complete
            } else {
                TranscriptBuildPhase::Collecting {
                    next_turn: thread.committed_tail(),
                }
            },
        );
        let selected_head = Some(crate::TranscriptViewHeadRecord::new(
            head.thread_id(),
            head.generation(),
            revision,
            0,
            thread.committed_tail(),
            thread.selected_path_digest(),
            if complete {
                ProjectionLifecycle::Current
            } else {
                ProjectionLifecycle::Stale
            },
        ));
        let selected_summary = complete.then(|| {
            crate::HistorySummaryRecord::new(
                summary.thread_id(),
                thread.revision(),
                None,
                thread.selected_path_digest(),
                true,
                summary.last_activity_at(),
            )
        });
        Ok(StartRecords {
            build,
            head: selected_head,
            summary: selected_summary,
        })
    }
}

use beryl_home_store::{DomainMutation, DomainReader, MutationBuilder, ReconciliationReservation};

use crate::{
    ProjectionLifecycle, SyndicMutationError, TranscriptBuildPhase, TranscriptBuildRecord,
    codec::*, domain::SyndicDomain,
};

use super::AdvanceTranscriptBuild;
use crate::mutation::required;

pub(super) struct AdvanceMutation {
    request: AdvanceTranscriptBuild,
}

impl AdvanceMutation {
    pub(super) const fn new(request: AdvanceTranscriptBuild) -> Self {
        Self { request }
    }

    fn records(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<AdvanceRecords, SyndicMutationError> {
        let key = ThreadTranscriptBuildKey {
            thread: self.request.thread_id,
            generation: self.request.generation,
        };
        let build = required::<TranscriptBuildsFamily>(reader, &key)?;
        if build.revision() != self.request.expected_build_revision
            || matches!(
                build.phase(),
                TranscriptBuildPhase::Complete | TranscriptBuildPhase::Superseded
            )
        {
            return Err(SyndicMutationError::TranscriptBuildConflict);
        }
        require_current_source(reader, &build)?;
        match build.phase() {
            TranscriptBuildPhase::Collecting { next_turn } => {
                super::path::collect_path_turn(reader, build, next_turn)
            }
            TranscriptBuildPhase::Publishing {
                next_depth,
                next_item,
                next_projection,
            } => super::publication::publish_entries(
                reader,
                build,
                next_depth,
                next_item,
                next_projection,
            ),
            TranscriptBuildPhase::Complete | TranscriptBuildPhase::Superseded => {
                Err(SyndicMutationError::TranscriptBuildConflict)
            }
        }
    }
}

fn require_current_source(
    reader: &DomainReader<'_, SyndicDomain>,
    build: &TranscriptBuildRecord,
) -> Result<(), SyndicMutationError> {
    let thread = required::<ThreadsFamily>(reader, &build.thread_id())?;
    let head = required::<TranscriptHeadsFamily>(reader, &build.thread_id())?;
    let summary = required::<HistorySummariesFamily>(reader, &build.thread_id())?;
    if thread.revision() < build.source_thread_revision()
        || thread.committed_tail() != build.committed_tail()
        || thread.selected_path_digest() != build.selected_path_digest()
        || head.generation() != build.generation()
        || head.revision() != build.revision()
        || head.lifecycle() != ProjectionLifecycle::Stale
        || head.entry_count() != 0
        || head.committed_tail() != build.committed_tail()
        || head.selected_path_digest() != build.selected_path_digest()
        || summary.thread_revision() != thread.revision()
        || summary.committed_tail() != thread.committed_tail()
        || summary.selected_path_digest() != thread.selected_path_digest()
    {
        return Err(SyndicMutationError::TranscriptBuildConflict);
    }
    Ok(())
}

pub(super) struct AdvanceRecords {
    pub(super) build: TranscriptBuildRecord,
    pub(super) path: Option<crate::TranscriptPathTurnRecord>,
    pub(super) entries: Vec<crate::TranscriptViewEntryRecord>,
    pub(super) head: Option<crate::TranscriptViewHeadRecord>,
    pub(super) summary: Option<crate::HistorySummaryRecord>,
}

impl DomainMutation<SyndicDomain> for AdvanceMutation {
    type Error = SyndicMutationError;
    type Prepared = AdvanceRecords;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        self.records(reader)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<TranscriptPathTurnsCodec>(1)?;
        reservation.reserve_records::<TranscriptEntriesCodec>(64)?;
        reservation.reserve_records::<TranscriptBuildsCodec>(1)?;
        reservation.reserve_records::<TranscriptHeadsCodec>(1)?;
        reservation.reserve_records::<HistorySummariesCodec>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let records = prepared;
        if let Some(path) = &records.path {
            mutations.put::<TranscriptPathTurnsCodec>(
                &ThreadTranscriptPathKey {
                    thread: path.thread_id(),
                    generation: path.generation(),
                    depth: path.depth(),
                },
                path,
            )?;
        }
        for entry in &records.entries {
            mutations.put::<TranscriptEntriesCodec>(
                &ThreadTranscriptKey {
                    thread: entry.thread_id(),
                    generation: entry.generation(),
                    position: entry.position(),
                },
                entry,
            )?;
        }
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

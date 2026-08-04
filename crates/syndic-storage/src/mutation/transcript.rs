use beryl_home_store::{CurrentDomainCommand, MutationContribution};
use beryl_model::{DomainRevision, ProjectionRevision, SyndicThreadId, ThreadRevision};

use crate::{SyndicStorage, TranscriptGeneration};

mod advance;
mod invalidate;
mod path;
mod publication;
mod start;

use advance::AdvanceMutation;
pub(super) use invalidate::{invalidate_transcript_projection, supersede_active_transcript_build};
use start::StartMutation;

/// Starts the stale generation selected by one exact head and non-future thread-revision proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartTranscriptBuild {
    thread_id: SyndicThreadId,
    expected_thread_revision: ThreadRevision,
    expected_head_revision: ProjectionRevision,
}

impl StartTranscriptBuild {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        expected_thread_revision: ThreadRevision,
        expected_head_revision: ProjectionRevision,
    ) -> Self {
        Self {
            thread_id,
            expected_thread_revision,
            expected_head_revision,
        }
    }
}

/// Advances one exact incomplete transcript generation by one bounded step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdvanceTranscriptBuild {
    thread_id: SyndicThreadId,
    generation: TranscriptGeneration,
    expected_build_revision: ProjectionRevision,
}

impl AdvanceTranscriptBuild {
    #[must_use]
    pub const fn new(
        thread_id: SyndicThreadId,
        generation: TranscriptGeneration,
        expected_build_revision: ProjectionRevision,
    ) -> Self {
        Self {
            thread_id,
            generation,
            expected_build_revision,
        }
    }
}

impl SyndicStorage {
    #[must_use]
    pub fn current_start_transcript_build(
        &self,
        request: StartTranscriptBuild,
    ) -> CurrentDomainCommand {
        self.handle.current_command(StartMutation::new(request))
    }

    #[must_use]
    pub fn start_transcript_build(
        &self,
        expected_domain_revision: DomainRevision,
        request: StartTranscriptBuild,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, StartMutation::new(request))
    }

    #[must_use]
    pub fn advance_transcript_build(
        &self,
        expected_domain_revision: DomainRevision,
        request: AdvanceTranscriptBuild,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, AdvanceMutation::new(request))
    }

    #[must_use]
    pub fn current_advance_transcript_build(
        &self,
        request: AdvanceTranscriptBuild,
    ) -> CurrentDomainCommand {
        self.handle.current_command(AdvanceMutation::new(request))
    }
}

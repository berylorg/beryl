use beryl_home_store::{
    CommandOutcome, CurrentDomainCommand, HomeCandidateRecoveryAccess, HomeStore,
};
use beryl_model::SyndicThreadId;
use syndic_storage::{
    CompactionAdmissionRead, CompactionOperationId, CompactionRecoveryCase, SyndicPointReadLimit,
    SyndicReadError, SyndicStorage,
};

#[derive(Clone, Copy)]
pub(super) enum CompactionAccess<'a> {
    Ordinary(&'a HomeStore),
    Candidate(&'a HomeCandidateRecoveryAccess<'a>),
}

impl CompactionAccess<'_> {
    pub(super) fn admission(
        self,
        storage: &SyndicStorage,
        thread: SyndicThreadId,
        limit: SyndicPointReadLimit,
    ) -> Result<CompactionAdmissionRead, SyndicReadError> {
        match self {
            Self::Ordinary(home) => storage.compaction_admission_read(home, thread, limit),
            Self::Candidate(home) => {
                storage.compaction_admission_read_candidate(home, thread, limit)
            }
        }
    }

    pub(super) fn recovery(
        self,
        storage: &SyndicStorage,
        operation: CompactionOperationId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<CompactionRecoveryCase>, SyndicReadError> {
        match self {
            Self::Ordinary(home) => storage.compaction_recovery_read(home, operation, limit),
            Self::Candidate(home) => {
                storage.compaction_recovery_read_candidate(home, operation, limit)
            }
        }
    }

    pub(super) fn execute_current(self, command: CurrentDomainCommand) -> CommandOutcome {
        match self {
            Self::Ordinary(home) => home.execute_current(command),
            Self::Candidate(home) => home.execute_current(command),
        }
    }
}

use super::*;

impl ProjectionCandidateId {
    pub(in super::super) const fn new(group_index: usize, candidate_index: usize) -> Self {
        Self {
            group_index,
            candidate_index,
        }
    }

    #[must_use]
    pub const fn group_index(self) -> usize {
        self.group_index
    }

    #[must_use]
    pub const fn candidate_index(self) -> usize {
        self.candidate_index
    }
}

impl RecoveredProjectionCandidateMetadata {
    pub(in super::super) fn new(
        candidate_id: ProjectionCandidateId,
        adoption: PersistentFailureServiceAdoptionMetadata,
    ) -> Self {
        Self {
            candidate_id,
            home_id: adoption.home_id(),
            home_generation: adoption.new_home_generation(),
            service_generation: adoption.new_service_generation(),
        }
    }

    #[must_use]
    pub const fn candidate_id(self) -> ProjectionCandidateId {
        self.candidate_id
    }

    #[must_use]
    pub const fn home_id(self) -> BerylHomeId {
        self.home_id
    }

    #[must_use]
    pub const fn home_generation(self) -> HomeGeneration {
        self.home_generation
    }

    #[must_use]
    pub const fn service_generation(self) -> ProjectionServiceGeneration {
        self.service_generation
    }
}

impl ProjectionCandidateMetadata {
    pub(in super::super) const fn new(
        candidate_id: ProjectionCandidateId,
        status: ProjectionCandidateReauthenticationStatus,
        rejection_reason: Option<ProjectionCandidateReauthenticationReason>,
        recovered: Option<RecoveredProjectionCandidateMetadata>,
    ) -> Self {
        Self {
            candidate_id,
            status,
            rejection_reason,
            recovered,
        }
    }

    #[must_use]
    pub const fn candidate_id(self) -> ProjectionCandidateId {
        self.candidate_id
    }

    #[must_use]
    pub const fn status(self) -> ProjectionCandidateReauthenticationStatus {
        self.status
    }

    #[must_use]
    pub const fn rejection_reason(self) -> Option<ProjectionCandidateReauthenticationReason> {
        self.rejection_reason
    }

    #[must_use]
    pub const fn recovered(self) -> Option<RecoveredProjectionCandidateMetadata> {
        self.recovered
    }
}

impl ProjectionCandidateReauthenticationOutcome {
    pub(in super::super) const fn new(candidate: ProjectionCandidateMetadata) -> Self {
        Self { candidate }
    }

    #[must_use]
    pub const fn candidate(self) -> ProjectionCandidateMetadata {
        self.candidate
    }

    #[must_use]
    pub const fn candidate_id(self) -> ProjectionCandidateId {
        self.candidate.candidate_id()
    }

    #[must_use]
    pub const fn status(self) -> ProjectionCandidateReauthenticationStatus {
        self.candidate.status()
    }

    #[must_use]
    pub const fn rejection_reason(self) -> Option<ProjectionCandidateReauthenticationReason> {
        self.candidate.rejection_reason()
    }
}

impl ProjectionCandidateDispositionOutcome {
    pub(in super::super) const fn new(candidate_id: ProjectionCandidateId) -> Self {
        Self { candidate_id }
    }

    #[must_use]
    pub const fn candidate_id(self) -> ProjectionCandidateId {
        self.candidate_id
    }
}

impl ProjectionCandidateLedgerMetadata {
    pub(in super::super) const fn new(
        adoption: PersistentFailureServiceAdoptionMetadata,
        connection_owner_count: usize,
        terminal_reason: Option<TerminalAdoptedProjectionConnectionServiceReason>,
        unprocessed_count: usize,
        rejected_count: usize,
        accepted_count: usize,
        disposed_count: usize,
    ) -> Self {
        Self {
            adoption,
            connection_owner_count,
            terminal_reason,
            unprocessed_count,
            rejected_count,
            accepted_count,
            disposed_count,
        }
    }

    #[must_use]
    pub const fn adoption(self) -> PersistentFailureServiceAdoptionMetadata {
        self.adoption
    }

    #[must_use]
    pub const fn connection_owner_count(self) -> usize {
        self.connection_owner_count
    }

    #[must_use]
    pub const fn terminal_reason(self) -> Option<TerminalAdoptedProjectionConnectionServiceReason> {
        self.terminal_reason
    }

    #[must_use]
    pub const fn unprocessed_count(self) -> usize {
        self.unprocessed_count
    }

    #[must_use]
    pub const fn rejected_count(self) -> usize {
        self.rejected_count
    }

    #[must_use]
    pub const fn accepted_count(self) -> usize {
        self.accepted_count
    }

    #[must_use]
    pub const fn disposed_count(self) -> usize {
        self.disposed_count
    }

    #[must_use]
    pub const fn is_ready_to_seal(self) -> bool {
        self.terminal_reason.is_none() && self.unprocessed_count == 0 && self.rejected_count == 0
    }
}

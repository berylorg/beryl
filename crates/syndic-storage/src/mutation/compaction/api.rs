use super::*;

impl SyndicStorage {
    #[must_use]
    pub fn settle_lifecycle_compaction(
        &self,
        expected_domain_revision: DomainRevision,
        request: SettleLifecycleCompaction,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, SettleLifecycleMutation(request))
    }

    #[must_use]
    pub fn current_settle_lifecycle_compaction(
        &self,
        request: SettleLifecycleCompaction,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(SettleLifecycleMutation(request))
    }

    #[must_use]
    pub fn seal_lifecycle_continuation_content(
        &self,
        expected_domain_revision: DomainRevision,
        request: SealLifecycleContinuationContent,
    ) -> MutationContribution {
        self.handle.contribution(
            expected_domain_revision,
            SealLifecycleContentMutation(request),
        )
    }

    #[must_use]
    pub fn current_seal_lifecycle_continuation_content(
        &self,
        request: SealLifecycleContinuationContent,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(SealLifecycleContentMutation(request))
    }

    #[must_use]
    pub fn admit_compaction_operation(
        &self,
        expected_domain_revision: DomainRevision,
        request: AdmitCompactionOperation,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, AdmitMutation(request))
    }

    #[must_use]
    pub fn current_admit_compaction_operation(
        &self,
        request: AdmitCompactionOperation,
    ) -> CurrentDomainCommand {
        self.handle.current_command(AdmitMutation(request))
    }

    #[must_use]
    pub fn claim_compaction_dispatch(
        &self,
        expected_domain_revision: DomainRevision,
        request: ClaimCompactionDispatch,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, ClaimMutation(request))
    }

    #[must_use]
    pub fn current_claim_compaction_dispatch(
        &self,
        request: ClaimCompactionDispatch,
    ) -> CurrentDomainCommand {
        self.handle.current_command(ClaimMutation(request))
    }

    #[must_use]
    pub fn publish_compaction_request_disposition(
        &self,
        expected_domain_revision: DomainRevision,
        request: PublishCompactionRequestDisposition,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, RequestMutation(request))
    }

    #[must_use]
    pub fn current_publish_compaction_request_disposition(
        &self,
        request: PublishCompactionRequestDisposition,
    ) -> CurrentDomainCommand {
        self.handle.current_command(RequestMutation(request))
    }

    #[must_use]
    pub fn publish_compaction_provider_event(
        &self,
        expected_domain_revision: DomainRevision,
        request: PublishCompactionProviderEvent,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, ProviderMutation(request))
    }

    #[must_use]
    pub fn current_publish_compaction_provider_event(
        &self,
        request: PublishCompactionProviderEvent,
    ) -> CurrentDomainCommand {
        self.handle.current_command(ProviderMutation(request))
    }

    #[must_use]
    pub fn settle_compaction_operation(
        &self,
        expected_domain_revision: DomainRevision,
        request: SettleCompactionOperation,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, SettleMutation(request))
    }

    #[must_use]
    pub fn current_settle_compaction_operation(
        &self,
        request: SettleCompactionOperation,
    ) -> CurrentDomainCommand {
        self.handle.current_command(SettleMutation(request))
    }

    #[must_use]
    pub fn abandon_compaction_operation(
        &self,
        expected_domain_revision: DomainRevision,
        request: AbandonCompactionOperation,
    ) -> MutationContribution {
        self.settle_compaction_operation(
            expected_domain_revision,
            SettleCompactionOperation::new(
                request.operation_id(),
                request.expected_operation_revision(),
                CompactionSettlement::Abandoned(request.reason()),
            ),
        )
    }

    #[must_use]
    pub fn current_abandon_compaction_operation(
        &self,
        request: AbandonCompactionOperation,
    ) -> CurrentDomainCommand {
        self.current_settle_compaction_operation(SettleCompactionOperation::new(
            request.operation_id(),
            request.expected_operation_revision(),
            CompactionSettlement::Abandoned(request.reason()),
        ))
    }
}

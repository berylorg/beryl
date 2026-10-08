use beryl_home_store::{
    DomainReader, DomainValidator, HomeGenerationIdentity, HomeStore, ValidationContribution,
};
use beryl_model::{BerylHomeId, DomainRevision, ExecutionBinding, SyndicDraftId, SyndicThreadId};

use crate::{
    PristineThreadAudit, PristineThreadCandidate, SyndicMutationError, SyndicReadError,
    SyndicStorage, SyndicTimestamp, ThreadCatalogSummaryPreparation, domain::SyndicDomain,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibleEmptyThreadCandidate {
    source: PristineThreadCandidate,
    home: BerylHomeId,
    generation: HomeGenerationIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibleEmptyThreadOutcome {
    expected: PristineThreadCandidate,
    home: BerylHomeId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EligibleEmptyThreadOutcomeAudit {
    Missing,
    Exact(EligibleEmptyThreadCandidate),
    Conflict,
}

struct ValidateEligibleEmptyThread {
    candidate: EligibleEmptyThreadCandidate,
}

impl DomainValidator<SyndicDomain> for ValidateEligibleEmptyThread {
    type Error = SyndicMutationError;

    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.candidate.validate(reader)
    }
}

impl EligibleEmptyThreadCandidate {
    pub fn thread_id(&self) -> SyndicThreadId {
        self.source.thread_id()
    }
    pub fn draft_id(&self) -> SyndicDraftId {
        self.source.draft_id()
    }
    pub fn created_at(&self) -> SyndicTimestamp {
        self.source.created_at()
    }
    pub(crate) fn source_revision(&self) -> DomainRevision {
        self.source.source_revision()
    }
    pub(crate) fn validate(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        self.source.validate_for_reuse(reader)
    }
}

impl SyndicStorage {
    pub fn inspect_eligible_empty_thread(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
        expected_execution: &ExecutionBinding,
    ) -> Result<Option<EligibleEmptyThreadCandidate>, SyndicReadError> {
        let generation = store.generation_identity()?;
        let candidate = self
            .inspect_eligible_empty_thread_candidate(store, thread_id, expected_execution)?
            .map(|source| EligibleEmptyThreadCandidate {
                source,
                home: store.home_id(),
                generation,
            });
        if store.generation_identity()? != generation {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "eligible-empty-thread inspection",
            });
        }
        Ok(candidate)
    }

    pub fn prepare_eligible_empty_thread_outcome(
        &self,
        store: &HomeStore,
        candidate: EligibleEmptyThreadCandidate,
        summary: ThreadCatalogSummaryPreparation,
    ) -> Result<EligibleEmptyThreadOutcome, SyndicMutationError> {
        self.qualify_eligible_candidate(store, &candidate)?;
        Ok(EligibleEmptyThreadOutcome {
            expected: candidate.source.with_catalog_successor(&summary)?,
            home: candidate.home,
        })
    }

    pub fn audit_eligible_empty_thread_outcome(
        &self,
        store: &HomeStore,
        outcome: &EligibleEmptyThreadOutcome,
    ) -> Result<EligibleEmptyThreadOutcomeAudit, SyndicReadError> {
        self.audit_eligible_empty_thread_source(store, outcome.home, &outcome.expected)
    }

    pub fn audit_eligible_empty_thread(
        &self,
        store: &HomeStore,
        candidate: &EligibleEmptyThreadCandidate,
    ) -> Result<EligibleEmptyThreadOutcomeAudit, SyndicReadError> {
        self.audit_eligible_empty_thread_source(store, candidate.home, &candidate.source)
    }

    pub fn validate_eligible_empty_thread(
        &self,
        store: &HomeStore,
        candidate: EligibleEmptyThreadCandidate,
    ) -> Result<ValidationContribution, SyndicMutationError> {
        self.qualify_eligible_candidate(store, &candidate)?;
        Ok(self.handle.validation(
            candidate.source_revision(),
            ValidateEligibleEmptyThread { candidate },
        ))
    }

    pub(crate) fn qualify_eligible_candidate(
        &self,
        store: &HomeStore,
        candidate: &EligibleEmptyThreadCandidate,
    ) -> Result<(), SyndicMutationError> {
        if candidate.home != store.home_id()
            || candidate.generation != store.generation_identity()?
            || candidate.source_revision() != self.revision(store)?
        {
            return Err(SyndicMutationError::PristineThreadConflict);
        }
        Ok(())
    }

    fn audit_eligible_empty_thread_source(
        &self,
        store: &HomeStore,
        home: BerylHomeId,
        expected: &PristineThreadCandidate,
    ) -> Result<EligibleEmptyThreadOutcomeAudit, SyndicReadError> {
        self.revision(store)?;
        if home != store.home_id() {
            return Ok(EligibleEmptyThreadOutcomeAudit::Conflict);
        }
        let generation = store.generation_identity()?;
        let audit = match self.audit_eligible_empty_thread_candidate(store, expected)? {
            PristineThreadAudit::Missing => EligibleEmptyThreadOutcomeAudit::Missing,
            PristineThreadAudit::Conflict => EligibleEmptyThreadOutcomeAudit::Conflict,
            PristineThreadAudit::Exact(source) => {
                EligibleEmptyThreadOutcomeAudit::Exact(EligibleEmptyThreadCandidate {
                    source,
                    home,
                    generation,
                })
            }
        };
        if store.generation_identity()? != generation {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "eligible-empty-thread outcome audit",
            });
        }
        Ok(audit)
    }
}

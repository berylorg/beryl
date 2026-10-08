use beryl_home_store::{DomainReader, HomeStore};
use beryl_model::{DomainRevision, ExecutionBinding, SyndicDraftId, SyndicThreadId};

use crate::{
    PristineThreadCandidate, SyndicMutationError, SyndicReadError, SyndicStorage, SyndicTimestamp,
    domain::SyndicDomain,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EligibleEmptyThreadCandidate {
    source: PristineThreadCandidate,
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
        self.inspect_eligible_empty_thread_candidate(store, thread_id, expected_execution)
            .map(|candidate| candidate.map(|source| EligibleEmptyThreadCandidate { source }))
    }
}

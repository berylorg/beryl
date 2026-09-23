use super::*;
use crate::{read::access::ReadAccess, terminal_history::TerminalHistoryReader};
use beryl_home_store::{
    DomainValidator, HomeCandidateRecoveryAccess, ReadError, ValidationContribution,
};

mod probe;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiscussionParentRequest {
    pub child_gate: DiscussionHandoffGateRecord,
    pub parent_thread_id: SyndicThreadId,
    pub context_owner: DiscussionContextOwnerId,
    pub context_digest: DiscussionContextDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionParentDisposition {
    Ready,
    Archived,
}

pub enum DiscussionParentEligibility {
    Waiting,
    Proven(PreparedDiscussionParent),
}

#[derive(Clone)]
pub struct PreparedDiscussionParent {
    handle: DomainHandle<SyndicDomain>,
    revision: DomainRevision,
    home_id: BerylHomeId,
    request: DiscussionParentRequest,
    disposition: DiscussionParentDisposition,
}

impl PreparedDiscussionParent {
    pub fn request(&self) -> DiscussionParentRequest {
        self.request
    }
    pub fn disposition(&self) -> DiscussionParentDisposition {
        self.disposition
    }

    pub fn into_ready_validation(self) -> Result<ValidationContribution, SyndicMutationError> {
        if self.disposition != DiscussionParentDisposition::Ready {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        Ok(self.handle.clone().validation(self.revision, self))
    }

    pub fn into_archived_release(self) -> Result<PreparedDiscussionHandoff, SyndicMutationError> {
        if self.disposition != DiscussionParentDisposition::Archived {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        let old_gate = self.request.child_gate;
        let new_gate = DiscussionHandoffGateRecord::new(
            old_gate.thread_id(),
            old_gate.revision().checked_next()?,
            DiscussionHandoffGateState::Open,
        );
        Ok(PreparedDiscussionHandoff {
            handle: self.handle.clone(),
            revision: self.revision,
            request: DiscussionHandoffMutation::Release { expected: old_gate },
            intent: DiscussionHandoffIntent {
                home_id: self.home_id,
                old_gate,
                new_gate,
                attributes: None,
            },
            proof: ReleaseProof::ParentArchived(self),
        })
    }

    pub(super) fn validate_parent(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        if probe::classify(reader, self.request)? != Some(self.disposition) {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        Ok(())
    }
}

impl DomainValidator<SyndicDomain> for PreparedDiscussionParent {
    type Error = SyndicMutationError;
    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        if self.disposition != DiscussionParentDisposition::Ready {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        self.validate_parent(reader)
    }
}

impl SyndicStorage {
    pub fn prepare_discussion_parent(
        &self,
        store: &HomeStore,
        request: DiscussionParentRequest,
    ) -> Result<DiscussionParentEligibility, SyndicReadError> {
        self.prepare_discussion_parent_with_access(ReadAccess::Ordinary(store), request)
    }

    pub fn prepare_discussion_parent_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        request: DiscussionParentRequest,
    ) -> Result<DiscussionParentEligibility, SyndicReadError> {
        self.prepare_discussion_parent_with_access(ReadAccess::Candidate(access), request)
    }

    fn prepare_discussion_parent_with_access(
        &self,
        access: ReadAccess<'_>,
        request: DiscussionParentRequest,
    ) -> Result<DiscussionParentEligibility, SyndicReadError> {
        let revision = self.revision_with_access(access)?;
        let result = probe::classify(
            &ParentRead {
                storage: self,
                access,
            },
            request,
        );
        if self.revision_with_access(access)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "discussion parent preparation",
            });
        }
        let disposition = result.map_err(|error| match error {
            SyndicMutationError::Read(error) => SyndicReadError::from(error),
            _ => SyndicReadError::Invariant("discussion parent source is missing or contradictory"),
        })?;
        Ok(match disposition {
            None => DiscussionParentEligibility::Waiting,
            Some(disposition) => DiscussionParentEligibility::Proven(PreparedDiscussionParent {
                handle: self.handle.clone(),
                revision,
                home_id: access.home_id(),
                request,
                disposition,
            }),
        })
    }
}

struct ParentRead<'a> {
    storage: &'a SyndicStorage,
    access: ReadAccess<'a>,
}
impl TerminalHistoryReader for ParentRead<'_> {
    fn read<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, ReadError> {
        self.access.read_point::<SyndicDomain, ExactCodec<F>>(
            &self.storage.handle,
            key,
            family_point_limit::<F>(),
        )
    }
}

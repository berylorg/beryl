use super::*;
use crate::{read::access::ReadAccess, terminal_history::TerminalHistoryReader};
use beryl_home_store::{
    DomainValidator, HomeCandidateRecoveryAccess, ReadError, ValidationContribution,
};

mod probe;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscussionChildSettlementDisposition {
    Ready,
    QueuedInput,
}

pub enum DiscussionChildSettlement {
    Waiting,
    Settled(PreparedDiscussionChildSettlement),
}

#[derive(Clone)]
pub struct PreparedDiscussionChildSettlement {
    handle: DomainHandle<SyndicDomain>,
    revision: DomainRevision,
    home_id: BerylHomeId,
    gate: DiscussionHandoffGateRecord,
    disposition: DiscussionChildSettlementDisposition,
}

impl PreparedDiscussionChildSettlement {
    pub fn gate(&self) -> DiscussionHandoffGateRecord {
        self.gate
    }
    pub fn disposition(&self) -> DiscussionChildSettlementDisposition {
        self.disposition
    }

    pub fn into_ready_validation(self) -> Result<ValidationContribution, SyndicMutationError> {
        if self.disposition != DiscussionChildSettlementDisposition::Ready {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        Ok(self.handle.clone().validation(self.revision, self))
    }

    pub fn into_queued_release(self) -> Result<PreparedDiscussionHandoff, SyndicMutationError> {
        if self.disposition != DiscussionChildSettlementDisposition::QueuedInput {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        let new_gate = DiscussionHandoffGateRecord::new(
            self.gate.thread_id(),
            self.gate.revision().checked_next()?,
            DiscussionHandoffGateState::Open,
        );
        Ok(PreparedDiscussionHandoff {
            handle: self.handle.clone(),
            revision: self.revision,
            request: DiscussionHandoffMutation::Release {
                expected: self.gate,
            },
            intent: DiscussionHandoffIntent {
                home_id: self.home_id,
                old_gate: self.gate,
                new_gate,
                attributes: None,
            },
            proof: ReleaseProof::ChildSettlement(self),
        })
    }

    pub(super) fn validate_settlement(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        if probe::classify(reader, self.gate)? != Some(self.disposition) {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        Ok(())
    }
}

impl DomainValidator<SyndicDomain> for PreparedDiscussionChildSettlement {
    type Error = SyndicMutationError;
    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        if self.disposition != DiscussionChildSettlementDisposition::Ready {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        self.validate_settlement(reader)
    }
}

impl SyndicStorage {
    pub fn prepare_discussion_child_settlement(
        &self,
        store: &HomeStore,
        expected: DiscussionHandoffGateRecord,
    ) -> Result<DiscussionChildSettlement, SyndicReadError> {
        self.prepare_discussion_child_settlement_with_access(ReadAccess::Ordinary(store), expected)
    }

    pub fn prepare_discussion_child_settlement_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        expected: DiscussionHandoffGateRecord,
    ) -> Result<DiscussionChildSettlement, SyndicReadError> {
        self.prepare_discussion_child_settlement_with_access(
            ReadAccess::Candidate(access),
            expected,
        )
    }

    fn prepare_discussion_child_settlement_with_access(
        &self,
        access: ReadAccess<'_>,
        expected: DiscussionHandoffGateRecord,
    ) -> Result<DiscussionChildSettlement, SyndicReadError> {
        let revision = self.revision_with_access(access)?;
        let result = probe::classify(
            &SettlementRead {
                storage: self,
                access,
            },
            expected,
        );
        if self.revision_with_access(access)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "discussion child settlement preparation",
            });
        }
        let disposition = result.map_err(|error| match error {
            SyndicMutationError::Read(error) => SyndicReadError::from(error),
            _ => SyndicReadError::Invariant(
                "discussion child settlement source is missing or contradictory",
            ),
        })?;
        Ok(match disposition {
            None => DiscussionChildSettlement::Waiting,
            Some(disposition) => {
                DiscussionChildSettlement::Settled(PreparedDiscussionChildSettlement {
                    handle: self.handle.clone(),
                    revision,
                    home_id: access.home_id(),
                    gate: expected,
                    disposition,
                })
            }
        })
    }
}

struct SettlementRead<'a> {
    storage: &'a SyndicStorage,
    access: ReadAccess<'a>,
}

impl TerminalHistoryReader for SettlementRead<'_> {
    fn read<F: Family>(&self, key: &F::Key) -> Result<Option<F::Value>, ReadError> {
        self.access.read_point::<SyndicDomain, ExactCodec<F>>(
            &self.storage.handle,
            key,
            family_point_limit::<F>(),
        )
    }
}

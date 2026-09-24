use super::required;
use crate::{codec::*, domain::SyndicDomain, *};
use beryl_home_store::{
    DomainHandle, DomainMutation, DomainReader, HomeStore, MutationBuilder, MutationContribution,
    ReconciliationReservation,
};
use beryl_model::{
    BerylHomeId, DiscussionContextDigest, DiscussionContextOwnerId, DomainRevision,
    InputGateRevision, JobId, ResolutionIntentId, SyndicThreadId, ThreadRevision,
};

mod admission;
mod outcome;
mod parent;
mod settlement;
pub use outcome::DiscussionHandoffStatus;
pub use parent::{
    DiscussionParentDisposition, DiscussionParentEligibility, DiscussionParentRequest,
    PreparedDiscussionParent,
    DiscussionParentExecution, DiscussionParentExecutionDisposition, DiscussionParentExecutionRequest,
    PreparedDiscussionParentExecution,
    GeneratedDiscussionInput, GeneratedDiscussionInputDiscovery, GeneratedDiscussionInputIntent,
    GeneratedDiscussionInputLookup, GeneratedDiscussionInputStatus, PreparedGeneratedDiscussionInput,
};
pub use settlement::{
    DiscussionChildSettlement, DiscussionChildSettlementDisposition,
    PreparedDiscussionChildSettlement,
};

#[derive(Clone, Copy, Debug)]
pub struct DiscussionParentFrontierProof {
    pub thread_id: SyndicThreadId,
    pub thread_revision: ThreadRevision,
    pub input_gate_revision: InputGateRevision,
    pub accepted_high_water: u64,
}

#[derive(Clone, Debug)]
pub struct AdmitDiscussionHandoff {
    pub thread_id: SyndicThreadId,
    pub thread_revision: ThreadRevision,
    pub attributes_revision: ThreadAttributesRevision,
    pub input_gate_revision: InputGateRevision,
    pub handoff_gate_revision: DiscussionHandoffGateRevision,
    pub turn_state_revision: TurnStateRevision,
    pub resolving_target: SteeringTargetProof,
    pub parent: DiscussionParentFrontierProof,
    pub context_owner: DiscussionContextOwnerId,
    pub context_digest: DiscussionContextDigest,
    pub intent_id: ResolutionIntentId,
    pub job_id: JobId,
}

#[derive(Clone, Debug)]
pub enum DiscussionHandoffMutation {
    Admit(AdmitDiscussionHandoff),
    Release {
        expected: DiscussionHandoffGateRecord,
    },
    ReleaseAndArchive {
        expected: DiscussionHandoffGateRecord,
        attributes_revision: ThreadAttributesRevision,
        archived_at: SyndicTimestamp,
    },
}

#[derive(Clone)]
pub struct DiscussionHandoffIntent {
    home_id: BerylHomeId,
    old_gate: DiscussionHandoffGateRecord,
    new_gate: DiscussionHandoffGateRecord,
    attributes: Option<(ThreadAttributesRecord, ThreadAttributesRecord)>,
}

impl DiscussionHandoffIntent {
    pub fn old_gate(&self) -> DiscussionHandoffGateRecord {
        self.old_gate
    }
    pub fn new_gate(&self) -> DiscussionHandoffGateRecord {
        self.new_gate
    }
}

pub struct PreparedDiscussionHandoff {
    handle: DomainHandle<SyndicDomain>,
    revision: DomainRevision,
    request: DiscussionHandoffMutation,
    intent: DiscussionHandoffIntent,
    proof: ReleaseProof,
}

enum ReleaseProof {
    None,
    ChildSettlement(PreparedDiscussionChildSettlement),
    ParentArchived(PreparedDiscussionParent),
    ParentExecution(PreparedDiscussionParentExecution),
}

impl PreparedDiscussionHandoff {
    pub fn intent(&self) -> &DiscussionHandoffIntent {
        &self.intent
    }
    pub fn contribution(self) -> MutationContribution {
        self.handle.clone().contribution(self.revision, self)
    }
}

impl SyndicStorage {
    pub fn prepare_discussion_handoff(
        &self,
        store: &HomeStore,
        request: DiscussionHandoffMutation,
    ) -> Result<PreparedDiscussionHandoff, SyndicReadError> {
        let revision = self.revision(store)?;
        let thread = match &request {
            DiscussionHandoffMutation::Admit(value) => value.thread_id,
            DiscussionHandoffMutation::Release { expected }
            | DiscussionHandoffMutation::ReleaseAndArchive { expected, .. } => expected.thread_id(),
        };
        let limit = SyndicPointReadLimit::new(65_536).expect("nonzero record bound");
        let old_gate = self.discussion_handoff_gate(store, thread, limit)?.ok_or(
            SyndicReadError::Invariant("discussion handoff gate is missing"),
        )?;
        let conflict = || SyndicReadError::Invariant("discussion handoff expectation disagrees");
        let (state, attributes) = match &request {
            DiscussionHandoffMutation::Admit(value) => {
                if old_gate.revision() != value.handoff_gate_revision
                    || old_gate.state() != DiscussionHandoffGateState::Open
                {
                    return Err(conflict());
                }
                (
                    DiscussionHandoffGateState::Pending {
                        intent_id: value.intent_id,
                        job_id: value.job_id,
                        resolving_turn_id: value.resolving_target.pending().active_turn_id(),
                    },
                    None,
                )
            }
            DiscussionHandoffMutation::Release { expected }
            | DiscussionHandoffMutation::ReleaseAndArchive { expected, .. } => {
                if old_gate != *expected {
                    return Err(conflict());
                }
                let DiscussionHandoffGateState::Pending { job_id, .. } = old_gate.state() else {
                    return Err(conflict());
                };
                let attributes = if let DiscussionHandoffMutation::ReleaseAndArchive {
                    attributes_revision,
                    archived_at,
                    ..
                } = &request
                {
                    let old = self
                        .thread_attributes(store, thread, limit)?
                        .ok_or_else(conflict)?;
                    if old.revision() != *attributes_revision {
                        return Err(conflict());
                    }
                    let new = old
                        .clone()
                        .archive_branch_discussion(job_id, *archived_at)
                        .map_err(|_| conflict())?;
                    Some((old, new))
                } else {
                    None
                };
                (DiscussionHandoffGateState::Open, attributes)
            }
        };
        let new_gate = DiscussionHandoffGateRecord::new(
            thread,
            old_gate.revision().checked_next().map_err(|_| conflict())?,
            state,
        );
        if self.revision(store)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "discussion handoff preparation",
            });
        }
        Ok(PreparedDiscussionHandoff {
            handle: self.handle.clone(),
            revision,
            request,
            proof: ReleaseProof::None,
            intent: DiscussionHandoffIntent {
                home_id: store.home_id(),
                old_gate,
                new_gate,
                attributes,
            },
        })
    }
}

impl DomainMutation<SyndicDomain> for PreparedDiscussionHandoff {
    type Error = SyndicMutationError;
    type Prepared = DiscussionHandoffIntent;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        match &self.proof {
            ReleaseProof::None => {}
            ReleaseProof::ChildSettlement(settlement) => settlement.validate_settlement(reader)?,
            ReleaseProof::ParentArchived(parent) => parent.validate_parent(reader)?,
            ReleaseProof::ParentExecution(parent) => parent.validate_execution(reader)?,
        }
        let thread = self.intent.old_gate.thread_id();
        let actual = required::<DiscussionHandoffGatesFamily>(reader, &thread)?;
        if actual != self.intent.old_gate {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        let record = required::<ThreadsFamily>(reader, &thread)?;
        let attributes = required::<ThreadAttributesFamily>(reader, &thread)?;
        if record.id() != thread
            || record.parent_thread_id().is_none()
            || record.context_owner_id().is_none()
            || attributes.thread_id() != thread
            || attributes.archive() != ThreadArchiveState::BranchDiscussionOpen
        {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        if let DiscussionHandoffMutation::Admit(request) = &self.request {
            admission::validate(reader, request, &record, &attributes)?;
        } else {
            let DiscussionHandoffGateState::Pending {
                resolving_turn_id, ..
            } = actual.state()
            else {
                return Err(SyndicMutationError::DiscussionHandoffConflict);
            };
            if required::<TurnsFamily>(reader, &resolving_turn_id)?.origin_thread_id() != thread {
                return Err(SyndicMutationError::DiscussionHandoffConflict);
            }
        }
        if self
            .intent
            .attributes
            .as_ref()
            .is_some_and(|(old, _)| old != &attributes)
        {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        Ok(self.intent)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<DiscussionHandoffGatesCodec>(1)?;
        if self.intent.attributes.is_some() {
            reservation.reserve_records::<ThreadAttributesCodec>(1)?;
        }
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let thread = prepared.new_gate.thread_id();
        mutations.put::<DiscussionHandoffGatesCodec>(&thread, &prepared.new_gate)?;
        if let Some((_, next)) = prepared.attributes {
            mutations.put::<ThreadAttributesCodec>(&thread, &next)?;
        }
        Ok(())
    }
}

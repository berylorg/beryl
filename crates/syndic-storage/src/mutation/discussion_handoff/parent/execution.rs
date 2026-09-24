use super::*;
use beryl_model::{SyndicAcceptedInputId, SyndicTurnId};

mod probe;

#[derive(Clone, Debug)]
pub struct DiscussionParentExecutionRequest {
    pub child_gate: DiscussionHandoffGateRecord,
    pub parent_thread_id: SyndicThreadId,
    pub input_id: SyndicAcceptedInputId,
    pub turn_id: SyndicTurnId,
    pub context_owner: DiscussionContextOwnerId,
    pub context_digest: DiscussionContextDigest,
    pub resolution: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscussionParentExecutionDisposition {
    Accepted(CasTurnSource),
    Terminal {
        status: TurnEndStatus,
        cas: Option<CasTurnSource>,
    },
}

pub enum DiscussionParentExecution {
    Waiting,
    Proven(PreparedDiscussionParentExecution),
}

#[derive(Clone)]
pub struct PreparedDiscussionParentExecution {
    handle: DomainHandle<SyndicDomain>,
    revision: DomainRevision,
    home_id: BerylHomeId,
    gate: DiscussionHandoffGateRecord,
    input: AcceptedInputRecord,
    attributes: ThreadAttributesRecord,
    disposition: DiscussionParentExecutionDisposition,
}

impl PreparedDiscussionParentExecution {
    pub fn input(&self) -> &AcceptedInputRecord {
        &self.input
    }
    pub fn disposition(&self) -> &DiscussionParentExecutionDisposition {
        &self.disposition
    }
    pub fn into_validation(self) -> ValidationContribution {
        self.handle.clone().validation(self.revision, self)
    }
    pub fn into_terminal_settlement(
        self,
        at: SyndicTimestamp,
    ) -> Result<PreparedDiscussionHandoff, SyndicMutationError> {
        let DiscussionParentExecutionDisposition::Terminal { status, .. } = self.disposition else {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        };
        let DiscussionHandoffGateState::Pending { job_id, .. } = self.gate.state() else {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        };
        let (request, attributes) = if status.outcome() == TurnTerminalOutcome::Complete {
            let archived = self
                .attributes
                .clone()
                .archive_branch_discussion(job_id, at)?;
            (
                DiscussionHandoffMutation::ReleaseAndArchive {
                    expected: self.gate,
                    attributes_revision: self.attributes.revision(),
                    archived_at: at,
                },
                Some((self.attributes.clone(), archived)),
            )
        } else {
            (
                DiscussionHandoffMutation::Release {
                    expected: self.gate,
                },
                None,
            )
        };
        Ok(PreparedDiscussionHandoff {
            handle: self.handle.clone(),
            revision: self.revision,
            request,
            intent: DiscussionHandoffIntent {
                home_id: self.home_id,
                old_gate: self.gate,
                new_gate: DiscussionHandoffGateRecord::new(
                    self.gate.thread_id(),
                    self.gate.revision().checked_next()?,
                    DiscussionHandoffGateState::Open,
                ),
                attributes,
            },
            proof: ReleaseProof::ParentExecution(self),
        })
    }
    pub(in crate::mutation::discussion_handoff) fn validate_execution(
        &self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<(), SyndicMutationError> {
        let (attributes, disposition) = probe::classify(reader, self.gate, &self.input)?;
        if attributes != self.attributes || disposition.as_ref() != Some(&self.disposition) {
            return Err(SyndicMutationError::DiscussionHandoffConflict);
        }
        Ok(())
    }
}

impl DomainValidator<SyndicDomain> for PreparedDiscussionParentExecution {
    type Error = SyndicMutationError;
    fn validate(&self, reader: &DomainReader<'_, SyndicDomain>) -> Result<(), Self::Error> {
        self.validate_execution(reader)
    }
}

impl SyndicStorage {
    pub fn prepare_discussion_parent_execution(
        &self,
        store: &HomeStore,
        request: DiscussionParentExecutionRequest,
    ) -> Result<DiscussionParentExecution, SyndicReadError> {
        self.prepare_discussion_parent_execution_with_access(ReadAccess::Ordinary(store), request)
    }
    pub fn prepare_discussion_parent_execution_candidate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        request: DiscussionParentExecutionRequest,
    ) -> Result<DiscussionParentExecution, SyndicReadError> {
        self.prepare_discussion_parent_execution_with_access(ReadAccess::Candidate(access), request)
    }
    fn prepare_discussion_parent_execution_with_access(
        &self,
        access: ReadAccess<'_>,
        request: DiscussionParentExecutionRequest,
    ) -> Result<DiscussionParentExecution, SyndicReadError> {
        let revision = self.revision_with_access(access)?;
        let reader = ParentRead {
            storage: self,
            access,
        };
        let input = reader
            .read::<AcceptedInputsFamily>(&request.input_id)?
            .ok_or(SyndicReadError::Invariant(
                "parent handoff input is missing",
            ))?;
        let AcceptedInputSource::DiscussionHandoff(receipt) = input.source() else {
            return Err(SyndicReadError::Invariant(
                "parent handoff input has composer provenance",
            ));
        };
        let DiscussionHandoffGateState::Pending {
            intent_id,
            job_id,
            resolving_turn_id,
        } = request.child_gate.state()
        else {
            return Err(SyndicReadError::Invariant(
                "child handoff gate is not pending",
            ));
        };
        if input.id() != request.input_id || input.id().as_bytes() != job_id.as_bytes() {
            return Err(SyndicReadError::Invariant(
                "parent input and handoff job disagree",
            ));
        }
        let lookup = GeneratedDiscussionInputLookup {
            home_id: access.home_id(),
            parent_thread_id: request.parent_thread_id,
            child_thread_id: request.child_gate.thread_id(),
            intent_id,
            job_id,
            context_owner: request.context_owner,
            context_digest: request.context_digest,
            resolving_turn_id,
            parent_turn_id: request.turn_id,
            canonical_item_id: receipt.canonical_item_id,
            resolution: request.resolution,
        };
        let discovered = match access {
            ReadAccess::Ordinary(store) => {
                self.discover_generated_discussion_input(store, &lookup)?
            }
            ReadAccess::Candidate(access) => {
                self.discover_generated_discussion_input_candidate(access, &lookup)?
            }
        };
        if discovered != GeneratedDiscussionInputDiscovery::Exact {
            return Err(SyndicReadError::Invariant(
                "parent generated input provenance disagrees",
            ));
        }
        let result = probe::classify(&reader, request.child_gate, &input);
        if self.revision_with_access(access)? != revision {
            return Err(SyndicReadError::ConcurrentChange {
                operation: "discussion parent execution preparation",
            });
        }
        let (attributes, disposition) = result.map_err(|error| match error {
            SyndicMutationError::Read(error) => SyndicReadError::from(error),
            _ => SyndicReadError::Invariant("parent execution source is missing or contradictory"),
        })?;
        Ok(match disposition {
            None => DiscussionParentExecution::Waiting,
            Some(disposition) => {
                DiscussionParentExecution::Proven(PreparedDiscussionParentExecution {
                    handle: self.handle.clone(),
                    revision,
                    home_id: access.home_id(),
                    gate: request.child_gate,
                    input,
                    attributes,
                    disposition,
                })
            }
        })
    }
}

use super::*;
use crate::cas_projection::BranchDiscussionResolutionContext;
use beryl_model::ResolutionIntentId;
use beryl_state::{
    BranchHandoffJobAdmission, BranchHandoffJobLifecycle, ParentQueueOrdinal,
    ResolutionAttemptOrdinal, ResolutionText, branch_handoff_job_id,
};
use syndic_storage::{
    AdmitDiscussionHandoff, BindingState, DiscussionHandoffGateState, DiscussionHandoffMutation,
    DiscussionParentFrontierProof, PendingSteeringTargetProof, SteeringTargetProof,
    SyndicPointReadLimit,
};

pub enum DiscussionResolutionAdmission {
    Existing(JobId),
    AlreadyAdmitted(JobId),
    DeferredQueuedInput,
    ParentArchived,
    DiscussionArchived,
    Prepared(PreparedDiscussionSettlement<'static>),
}

impl DiscussionSettlementService {
    #[cfg(feature = "test-faults")]
    pub fn prepare_resolution_for_test(
        &self,
        context: &BranchDiscussionResolutionContext,
        intent_id: ResolutionIntentId,
        resolution: ResolutionText,
        cancellation: CommandCancellation,
    ) -> Result<DiscussionResolutionAdmission, DiscussionSettlementError> {
        self.prepare_resolution(context, intent_id, resolution, cancellation)
    }
    pub(crate) fn prepare_resolution(
        &self,
        context: &BranchDiscussionResolutionContext,
        intent_id: ResolutionIntentId,
        resolution: ResolutionText,
        cancellation: CommandCancellation,
    ) -> Result<DiscussionResolutionAdmission, DiscussionSettlementError> {
        let permit = self.operations.permit();
        let job_id = branch_handoff_job_id(intent_id);
        let flight = permit.commit(|| self.operations.acquire(job_id))??;
        if cancellation.is_cancelled() {
            return Err(DiscussionSettlementError::Cancelled);
        }
        let before = self.store.home_revision()?;
        let result = self.resolution_from_source(
            context,
            intent_id,
            resolution,
            cancellation,
            before,
            permit,
            flight,
        )?;
        if self.store.home_revision()? != before {
            return Err(DiscussionSettlementError::ConcurrentChange);
        }
        Ok(result)
    }

    fn resolution_from_source(
        &self,
        context: &BranchDiscussionResolutionContext,
        intent_id: ResolutionIntentId,
        resolution: ResolutionText,
        cancellation: CommandCancellation,
        before: beryl_model::HomeRevision,
        permit: ProcessExecutionPermit,
        mut flight: Flight,
    ) -> Result<DiscussionResolutionAdmission, DiscussionSettlementError> {
        let mismatch = || DiscussionSettlementError::IdentityMismatch;
        let store = &self.store;
        let jobs = self.state.durable_jobs();
        let child_id = context.ordinary().thread_id();
        let turn_id = context.ordinary().turn_id();
        let request = context.request_identity();
        if let Some(job) = jobs.admitted_handoff_request(store, request)? {
            if job.discussion_thread_id() != child_id || job.resolving_turn_id() != turn_id {
                return Err(mismatch());
            }
            flight.retarget(job.job_id())?;
            return Ok(DiscussionResolutionAdmission::Existing(job.job_id()));
        }
        let limit = SyndicPointReadLimit::new(400_000).expect("bounded admission facts");
        let child = self
            .syndic
            .thread(store, child_id, limit)?
            .ok_or_else(mismatch)?;
        let attributes = self
            .syndic
            .thread_attributes(store, child_id, limit)?
            .ok_or_else(mismatch)?;
        let parent_id = child.parent_thread_id().ok_or_else(mismatch)?;
        let owner = child.context_owner_id().ok_or_else(mismatch)?;
        if child.id() != child_id || attributes.thread_id() != child_id {
            return Err(mismatch());
        }
        if attributes.archive().is_archived() {
            return Ok(DiscussionResolutionAdmission::DiscussionArchived);
        }
        let handoff = self
            .syndic
            .discussion_handoff_gate(store, child_id, limit)?
            .ok_or_else(mismatch)?;
        let latest = jobs.latest_attempt(store, child_id)?;
        let ordinal = if let Some(latest) = latest {
            let prior = jobs.job(store, latest.job_id())?.ok_or_else(mismatch)?;
            let authenticated = jobs
                .admitted_handoff_request(store, prior.request())?
                .ok_or_else(mismatch)?;
            if prior.job_id() != latest.job_id()
                || authenticated != prior
                || prior.discussion_thread_id() != child_id
                || prior.attempt_ordinal() != latest.attempt_ordinal()
            {
                return Err(mismatch());
            }
            if prior.lifecycle().is_live() {
                if handoff.state()
                    != (DiscussionHandoffGateState::Pending {
                        intent_id: prior.intent_id(),
                        job_id: prior.job_id(),
                        resolving_turn_id: prior.resolving_turn_id(),
                    })
                {
                    return Err(mismatch());
                }
                flight.retarget(prior.job_id())?;
                return Ok(DiscussionResolutionAdmission::AlreadyAdmitted(
                    prior.job_id(),
                ));
            }
            if prior.lifecycle() != BranchHandoffJobLifecycle::TerminalFailed {
                return Err(mismatch());
            }
            ResolutionAttemptOrdinal::new(
                prior
                    .attempt_ordinal()
                    .get()
                    .checked_add(1)
                    .ok_or_else(mismatch)?,
            )
            .map_err(|_| mismatch())?
        } else {
            ResolutionAttemptOrdinal::FIRST
        };
        if handoff.thread_id() != child_id || handoff.state() != DiscussionHandoffGateState::Open {
            return Err(mismatch());
        }
        let gate = self
            .syndic
            .input_gate(store, child_id, limit)?
            .ok_or_else(mismatch)?;
        if gate.thread_id() != child_id {
            return Err(mismatch());
        }
        if gate.live_next_turn_count() != 0 {
            return Ok(DiscussionResolutionAdmission::DeferredQueuedInput);
        }
        let parent = self
            .syndic
            .thread(store, parent_id, limit)?
            .ok_or_else(mismatch)?;
        let parent_attributes = self
            .syndic
            .thread_attributes(store, parent_id, limit)?
            .ok_or_else(mismatch)?;
        if parent.id() != parent_id || parent_attributes.thread_id() != parent_id {
            return Err(mismatch());
        }
        if parent_attributes.archive().is_archived() {
            return Ok(DiscussionResolutionAdmission::ParentArchived);
        }
        let parent_gate = self
            .syndic
            .input_gate(store, parent_id, limit)?
            .ok_or_else(mismatch)?;
        let envelope = self
            .syndic
            .context_envelope(store, owner, limit)?
            .ok_or_else(mismatch)?;
        let digest = envelope.envelope().descriptor().digest();
        let binding = self
            .syndic
            .current_binding(store, child_id, limit)?
            .ok_or_else(mismatch)?;
        let BindingState::Active(active) = binding.binding().state() else {
            return Err(mismatch());
        };
        if active.turn_id() != turn_id || active.usable().cas_thread_id() != request.cas_thread_id()
        {
            return Err(mismatch());
        }
        let turn_state = self
            .syndic
            .turn_state(store, turn_id, limit)?
            .ok_or_else(mismatch)?;
        let admission = BranchHandoffJobAdmission::new(
            intent_id,
            ordinal,
            child_id,
            parent_id,
            owner,
            digest,
            turn_id,
            request.clone(),
            ParentQueueOrdinal::new(parent_gate.accepted_high_water()),
            resolution,
        );
        let job_id = admission.job_id();
        let syndic = self.syndic.prepare_discussion_handoff(
            store,
            DiscussionHandoffMutation::Admit(AdmitDiscussionHandoff {
                thread_id: child_id,
                thread_revision: child.revision(),
                attributes_revision: attributes.revision(),
                input_gate_revision: gate.revision(),
                handoff_gate_revision: handoff.revision(),
                turn_state_revision: turn_state.revision(),
                resolving_target: SteeringTargetProof::new(
                    PendingSteeringTargetProof::new(
                        binding.binding().revision(),
                        active.snapshot_id(),
                        turn_id,
                        request.cas_thread_id().clone(),
                    ),
                    request.cas_turn_id().clone(),
                ),
                parent: DiscussionParentFrontierProof {
                    thread_id: parent_id,
                    thread_revision: parent.revision(),
                    input_gate_revision: parent_gate.revision(),
                    accepted_high_water: parent_gate.accepted_high_water(),
                },
                context_owner: owner,
                context_digest: digest,
                intent_id,
                job_id,
            }),
        )?;
        let job = jobs.prepare_handoff_job_admission(store, admission)?;
        let witness = job.witness().clone();
        let intent = syndic.intent().clone();
        let mut command = HomeCommand::new(before).with_cancellation(cancellation);
        command.add(job.contribution())?;
        command.add(syndic.contribution())?;
        Ok(DiscussionResolutionAdmission::Prepared(
            PreparedDiscussionSettlement {
                command: Some(command),
                execution: Execution::Ordinary(self.store.clone()),
                permit: Some(permit),
                audit: DiscussionSettlementAudit(Arc::new(Attempt {
                    home_id: store.home_id(),
                    job: JobWitness::Admission(witness),
                    syndic: Some(SyndicSettlementIntent::Gate(intent)),
                    result: DiscussionSettlementResult::ResolutionAdmitted(job_id),
                    disposition: Mutex::new(Disposition::Prepared),
                    _flight: flight,
                })),
            },
        ))
    }
}

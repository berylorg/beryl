use super::*;
use crate::{
    cas_projection::BranchDiscussionResolutionContext,
    discussion_settlement::{
        DiscussionResolutionAdmission, DiscussionSettlementError, DiscussionSettlementOutcome,
        DiscussionSettlementService,
    },
};
use beryl_home_store::CommandCancellation;
use beryl_model::{JobId, ResolutionIntentId};
use beryl_state::ResolutionText;

pub enum DiscussionResolutionOutcome {
    Existing(JobId),
    AlreadyAdmitted(JobId),
    DeferredQueuedInput,
    ParentArchived,
    DiscussionArchived,
    Command(DiscussionSettlementOutcome),
}

pub struct ScopedDiscussionResolutionOutcome<'a> {
    outcome: DiscussionResolutionOutcome,
    _command: LiveHomeCommand<'a>,
}

impl ScopedDiscussionResolutionOutcome<'_> {
    pub fn publish<T>(self, publish: impl FnOnce(DiscussionResolutionOutcome) -> T) -> T {
        publish(self.outcome)
    }
}

impl ProjectionConnectionService {
    pub fn admit_discussion_resolution(
        &self,
        settlement: &DiscussionSettlementService,
        context: &BranchDiscussionResolutionContext,
        intent_id: ResolutionIntentId,
        resolution: ResolutionText,
        cancellation: CommandCancellation,
    ) -> Result<ScopedDiscussionResolutionOutcome<'_>, DiscussionSettlementError> {
        if !context.ordinary().belongs_to(
            self.home_id,
            self.home_generation,
            self.service_generation,
        ) {
            return Err(DiscussionSettlementError::IdentityMismatch);
        }
        let command = self
            .live_home_command()
            .map_err(|_| DiscussionSettlementError::CustodyUnavailable)?;
        if !settlement.matches_home(command.home()) {
            return Err(DiscussionSettlementError::ForeignHome);
        }
        let outcome =
            match settlement.prepare_resolution(context, intent_id, resolution, cancellation)? {
                DiscussionResolutionAdmission::Existing(job) => {
                    DiscussionResolutionOutcome::Existing(job)
                }
                DiscussionResolutionAdmission::AlreadyAdmitted(job) => {
                    DiscussionResolutionOutcome::AlreadyAdmitted(job)
                }
                DiscussionResolutionAdmission::DeferredQueuedInput => {
                    DiscussionResolutionOutcome::DeferredQueuedInput
                }
                DiscussionResolutionAdmission::ParentArchived => {
                    DiscussionResolutionOutcome::ParentArchived
                }
                DiscussionResolutionAdmission::DiscussionArchived => {
                    DiscussionResolutionOutcome::DiscussionArchived
                }
                DiscussionResolutionAdmission::Prepared(prepared) => {
                    DiscussionResolutionOutcome::Command(prepared.execute())
                }
            };
        Ok(ScopedDiscussionResolutionOutcome {
            outcome,
            _command: command,
        })
    }
}

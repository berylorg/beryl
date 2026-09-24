use super::super::{
    BranchDiscussionResolutionContext, DiscussionResolutionOutcome,
    persistent_failure::LiveCommandAuthorizer,
};
use crate::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
    discussion_settlement::{
        DiscussionSettlementError, DiscussionSettlementOutcome, DiscussionSettlementResult,
        DiscussionSettlementService,
    },
};
use beryl_backend::DynamicToolCallResponse;
use beryl_home_store::{CommandCancellation, HomeGeneration};
use beryl_model::{BerylHomeId, ResolutionIntentId};
use beryl_state::ResolutionText;
use std::sync::{OnceLock, Weak};

pub(in crate::cas_projection) struct ResolutionAuthority {
    home_id: BerylHomeId,
    generation: HomeGeneration,
    commands: LiveCommandAuthorizer,
    settlement: OnceLock<DiscussionSettlementService>,
}

impl ResolutionAuthority {
    pub(in crate::cas_projection) fn new(
        home_id: BerylHomeId,
        generation: HomeGeneration,
        commands: LiveCommandAuthorizer,
    ) -> Self {
        Self {
            home_id,
            generation,
            commands,
            settlement: OnceLock::new(),
        }
    }

    pub(in crate::cas_projection) fn configure(
        &self,
        settlement: DiscussionSettlementService,
    ) -> Result<(), DiscussionSettlementError> {
        if !settlement.matches_home_generation(self.home_id, self.generation) {
            return Err(DiscussionSettlementError::ForeignHome);
        }
        self.settlement
            .set(settlement)
            .map_err(|_| DiscussionSettlementError::DuplicateIdentity)
    }

    fn respond(
        &self,
        context: BranchDiscussionResolutionContext,
        request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        let Ok(_command) = self.commands.authorize() else {
            return unavailable();
        };
        if !context.ordinary().belongs_to(
            self.home_id,
            self.generation,
            self.commands.service_generation(),
        ) {
            return unavailable();
        }
        let Some(settlement) = self.settlement.get() else {
            return unavailable();
        };
        if !settlement.matches_home_generation(self.home_id, self.generation) {
            return unavailable();
        }
        let Ok(resolution) = ResolutionText::new(request.into_resolution()) else {
            return rejected("invalid_resolution");
        };
        let mut identity = [0; 16];
        if getrandom::fill(&mut identity).is_err() {
            return unavailable();
        }
        let outcome = super::super::service::admit_resolution(
            settlement,
            &context,
            ResolutionIntentId::from_bytes(identity),
            resolution,
            CommandCancellation::new(),
        );
        match outcome {
            Ok(DiscussionResolutionOutcome::Existing(job)) => admitted("admitted", job),
            Ok(DiscussionResolutionOutcome::AlreadyAdmitted(job)) => admitted("already_admitted", job),
            Ok(DiscussionResolutionOutcome::DeferredQueuedInput) => DynamicToolCallResponse::success_text(serde_json::json!({
                "status": "deferred_queued_input", "guidance": "Continue the discussion with its queued input. Only a later resolution tool call can try again."
            }).to_string()),
            Ok(DiscussionResolutionOutcome::ParentArchived) => rejected("parent_archived"),
            Ok(DiscussionResolutionOutcome::DiscussionArchived) => rejected("discussion_archived"),
            Ok(DiscussionResolutionOutcome::Command(DiscussionSettlementOutcome::Committed { result: DiscussionSettlementResult::ResolutionAdmitted(job), .. })) => admitted("admitted", job),
            Ok(DiscussionResolutionOutcome::Command(DiscussionSettlementOutcome::Indeterminate { .. })) => DynamicToolCallResponse::failure_text(serde_json::json!({
                "status": "outcome_unresolved", "guidance": "Admission has not been confirmed. Beryl retains its recovery custody; do not treat this as success or repeat the handoff."
            }).to_string()),
            _ => unavailable(),
        }
    }
}

#[derive(Clone)]
pub(super) struct ProcessResolutionHandler {
    authority: Weak<ResolutionAuthority>,
}

impl ProcessResolutionHandler {
    pub(super) fn new(authority: Weak<ResolutionAuthority>) -> Self {
        Self { authority }
    }
}

impl BranchDiscussionResolutionRequestHandler for ProcessResolutionHandler {
    fn respond_branch_discussion_resolution(
        &mut self,
        context: BranchDiscussionResolutionContext,
        request: BranchDiscussionResolutionRequest,
    ) -> DynamicToolCallResponse {
        self.authority
            .upgrade()
            .map_or_else(unavailable, |authority| authority.respond(context, request))
    }
}

fn admitted(status: &'static str, job: beryl_model::JobId) -> DynamicToolCallResponse {
    DynamicToolCallResponse::success_text(
        serde_json::json!({"status": status, "job_id": job.to_string()}).to_string(),
    )
}
fn rejected(status: &'static str) -> DynamicToolCallResponse {
    DynamicToolCallResponse::failure_text(serde_json::json!({"status": status}).to_string())
}
fn unavailable() -> DynamicToolCallResponse {
    rejected("unavailable")
}

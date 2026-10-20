use super::*;
use beryl_home_store::{CommandOutcome, HomeCommand};
use beryl_model::{JobId, ResolutionIntentId};
use beryl_state::{
    AdmitBranchHandoffJob, BranchHandoffJobAdmission, HandoffFailureEvidence, HandoffFailureKind,
    HandoffJobTransition, ParentQueueOrdinal, ResolutionAttemptOrdinal, ResolutionRequestIdentity,
    ResolutionText,
};

pub(super) fn admit_retryable(
    home: &beryl_home_store::HomeStore,
    state: &beryl_state::BerylState,
    storage: &SyndicStorage,
    child: SyndicThreadId,
) -> beryl_state::BranchHandoffJobRecord {
    let mut request = crate::support::discussion_handoff::active_request_for(
        home,
        storage,
        child,
        SyndicDraftId::from_bytes([224; 16]),
        SyndicItemId::from_bytes([225; 16]),
        ResolutionIntentId::from_bytes([226; 16]),
        JobId::from_bytes([227; 16]),
    );
    let admission = BranchHandoffJobAdmission::new(
        request.intent_id,
        ResolutionAttemptOrdinal::FIRST,
        request.thread_id,
        request.parent.thread_id,
        request.context_owner,
        request.context_digest,
        request.resolving_target.pending().active_turn_id(),
        ResolutionRequestIdentity::new(
            request.resolving_target.pending().cas_thread_id().clone(),
            request.resolving_target.cas_turn_id().clone(),
            beryl_model::DynamicToolCallId::new("mounted-resolve").unwrap(),
        ),
        ParentQueueOrdinal::new(request.parent.accepted_high_water),
        ResolutionText::new("Exact mounted handoff payload").unwrap(),
    );
    let job = admission.job_id();
    request.job_id = job;
    let handoff = storage
        .prepare_discussion_handoff(home, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(handoff.contribution()).unwrap();
    command
        .add(state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(home).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let current = state.durable_jobs().job(home, job).unwrap().unwrap();
    let paused = state
        .durable_jobs()
        .prepare_handoff_job_transition(
            home,
            job,
            current.revision(),
            HandoffJobTransition::RetryableFailure(
                HandoffFailureEvidence::new(
                    HandoffFailureKind::RuntimeUnavailable,
                    Some("The parent runtime is temporarily unavailable.".into()),
                )
                .unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(paused.contribution()).unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    state.durable_jobs().job(home, job).unwrap().unwrap()
}

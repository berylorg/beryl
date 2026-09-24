use beryl_app::discussion_settlement::*;
use beryl_home_store::{CommandCancellation, CommandOutcome, HomeCommand};
use beryl_model::{JobId, ResolutionIntentId, SyndicItemId, SyndicTurnId};
use beryl_state::{
    AdmitBranchHandoffJob, BranchHandoffJobAdmission, ParentQueueOrdinal, ResolutionAttemptOrdinal,
    ResolutionRequestIdentity, ResolutionText,
};
use std::num::NonZeroUsize;
use syndic_storage::*;

use crate::{support, syndic::Fixture};

pub fn seed(
    fixture: &Fixture,
    resolution: &str,
) -> (
    DiscussionSettlementService,
    AcceptedInputRecord,
    DiscussionHandoffReceipt,
) {
    seed_with_service(fixture, resolution, None)
}

pub fn seed_with_service(
    fixture: &Fixture,
    resolution: &str,
    service: Option<DiscussionSettlementService>,
) -> (
    DiscussionSettlementService,
    AcceptedInputRecord,
    DiscussionHandoffReceipt,
) {
    let home = fixture.home();
    let storage = &fixture.storage;
    support::seed_populated(&home, storage.clone());
    let mut request = support::discussion_handoff::active_request(
        &home,
        storage,
        ResolutionIntentId::from_bytes([210; 16]),
        JobId::from_bytes([211; 16]),
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
            beryl_model::DynamicToolCallId::new("resolve").unwrap(),
        ),
        ParentQueueOrdinal::new(request.parent.accepted_high_water),
        ResolutionText::new(resolution).unwrap(),
    );
    let job = admission.job_id();
    request.job_id = job;
    let prepared = storage
        .prepare_discussion_handoff(&home, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(prepared.contribution()).unwrap();
    command
        .add(fixture.state.durable_jobs().admit_branch_handoff(
            fixture.state.durable_jobs().revision(&home).unwrap(),
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
    let service = service.unwrap_or_else(|| {
        DiscussionSettlementService::new(
            DiscussionSettlementOperations::new(
                fixture.process_admission.clone(),
                NonZeroUsize::new(1).unwrap(),
            ),
            home.service_reference(),
            fixture.state.clone(),
            storage.clone(),
        )
    });
    support::discussion_handoff::complete_resolving_turn(&home, storage);
    assert!(matches!(
        service
            .prepare(job, CommandCancellation::new())
            .unwrap()
            .unwrap()
            .execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::ReadyForParent,
            later_failure: None,
            ..
        }
    ));
    support::converge_and_release_terminal_history(
        &home,
        storage.clone(),
        support::id(30),
        support::populated::source_turn(),
    );
    let prepared = service
        .prepare_parent_input(
            job,
            DiscussionParentInputRequest {
                turn_id: SyndicTurnId::from_bytes([234; 16]),
                item_id: SyndicItemId::from_bytes([232; 16]),
                admitted_at: support::timestamp(500),
            },
            CommandCancellation::new(),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        prepared.execute(),
        DiscussionSettlementOutcome::Committed {
            result: DiscussionSettlementResult::StartingParent(_),
            later_failure: None,
            ..
        }
    ));
    let input = storage
        .accepted_input(
            &home,
            beryl_model::SyndicAcceptedInputId::from_bytes(*job.as_bytes()),
            SyndicPointReadLimit::new(400_000).unwrap(),
        )
        .unwrap()
        .unwrap();
    let AcceptedInputSource::DiscussionHandoff(receipt) = input.source() else {
        panic!("generated receipt");
    };
    (service, input, receipt)
}
